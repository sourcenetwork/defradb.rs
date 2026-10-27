/-!
# SharedQuantizer.Tagged: a trained record that names its training, and a cache keyed by it

The story, in plain words. A trained DiskANN index stores a small trained
record: its width, its subspace count and, since this change, a tag the
training drew at random when it wrote its codebooks. Older records have no
tag. A node keeps one quantizer per index in memory, under the tag of the
codebooks it was built from, and a writer uses it only when its own
snapshot's record carries the same tag.

Two things have to hold for that to be safe. The record must read back as
written, in both its encodings, so a tag is never lost or invented on the
way through the store. And the cache must only ever answer a tag with the
codebooks that tag names, which holds as long as every share pairs a tag
with that tag's codebooks.

A tiny example. Tag 7 names codebooks A. The cache holds (7, A). A writer
whose record says 7 gets A; a writer whose record says 9 gets nothing and
builds its own.

What must never happen:
* a record with a tag reads back with another tag, or none
  (`decode_encode_tagged`);
* a record without a tag reads back with one (`decode_encode_untagged`);
* the cache answers a tag with codebooks other than that tag's
  (`lookup_is_books`), or answers a tag it does not hold
  (`lookup_other_tag`);
* and the law does not survive one tag shared with two sets of codebooks,
  which is why a training draws its tag in the attempt that writes them
  (`reused_tag_serves_other_books`).

The order of trainings, retries and writers is the TLA+ model
`proofs/tla/SharedQuantizer.tla`; this file proves the laws it rests on.

Rust anchors: `Trained`, `encode_trained`, `decode_trained`
(`crates/db/src/index/vector/engine/diskann/codec.rs`), and
`CodeCache::quantizer`, `CodeCache::share_quantizer`
(`crates/db/src/index/vector/engine/diskann/cache.rs`).
-/

namespace SharedQuantizer

-- Bytes are numbers below 256, one per list element.
abbrev Bytes := List Nat

-- `le n v` is `v` as `n` little-endian bytes: `u32::to_le_bytes` for n = 4,
-- `u128::to_le_bytes` for n = 16.
def le : Nat → Nat → Bytes
  | 0, _ => []
  | n + 1, v => v % 256 :: le n (v / 256)

-- `unle` reads little-endian bytes back: `from_le_bytes`.
def unle : Bytes → Nat
  | [] => 0
  | b :: bs => b + 256 * unle bs

-- Writing `n` bytes gives `n` bytes.
theorem le_length : ∀ n v, (le n v).length = n := by
  intro n
  induction n with
  | zero => intro v; rfl
  | succ n ih => intro v; simp [le, ih]

-- A value that fits in `n` bytes reads back as itself.
theorem unle_le : ∀ n v, v < 256 ^ n → unle (le n v) = v := by
  intro n
  induction n with
  | zero => intro v h; simp at h; simp [le, unle, h]
  | succ n ih =>
    intro v h
    -- the high bytes hold `v / 256`, which fits in `n` bytes
    have hdiv : v / 256 < 256 ^ n := by
      rw [Nat.div_lt_iff_lt_mul (by decide)]
      rw [Nat.pow_succ] at h
      exact h
    simp only [le, unle, ih _ hdiv]
    -- the low byte and the high bytes put back together are `v`
    omega

-- What training fixed: the width, the subspace count, and the tag if any.
structure Trained where
  dimensions : Nat
  m : Nat
  training : Option Nat
  deriving DecidableEq

-- The version byte of a record without a tag (`VERSION`) and with one
-- (`TRAINED_ID_VERSION`).
def untaggedVersion : Nat := 1
def taggedVersion : Nat := 2

-- `encode_trained`: the version, the width and the count as four bytes
-- each, then the tag as sixteen bytes when there is one.
def encode (t : Trained) : Bytes :=
  match t.training with
  | none => untaggedVersion :: (le 4 t.dimensions ++ le 4 t.m)
  | some tag => taggedVersion :: (le 4 t.dimensions ++ le 4 t.m ++ le 16 tag)

-- `decode_trained`: the version and the length pick the encoding; anything
-- else is refused.
def decode : Bytes → Option Trained
  | [] => none
  | v :: rest =>
    if v = untaggedVersion ∧ rest.length = 8 then
      some ⟨unle (rest.take 4), unle ((rest.drop 4).take 4), none⟩
    else if v = taggedVersion ∧ rest.length = 24 then
      some ⟨unle (rest.take 4), unle ((rest.drop 4).take 4), some (unle (rest.drop 8))⟩
    else none

-- The fields fit their Rust types: `u32`, `u32`, `u128`.
def Fits (t : Trained) : Prop :=
  t.dimensions < 256 ^ 4 ∧ t.m < 256 ^ 4 ∧ ∀ tag, t.training = some tag → tag < 256 ^ 16

-- A record without a tag reads back as written, still without a tag.
theorem decode_encode_untagged (d m : Nat) (hd : d < 256 ^ 4) (hm : m < 256 ^ 4) :
    decode (encode ⟨d, m, none⟩) = some ⟨d, m, none⟩ := by
  have l4 := le_length 4
  have t4 : ∀ v, (le 4 v).take 4 = le 4 v := fun v => List.take_of_length_le (by simp [l4])
  simp [encode, decode, untaggedVersion, l4, t4, unle_le 4 d hd, unle_le 4 m hm,
    List.take_left', List.drop_left']

-- A record with a tag reads back with that same tag.
theorem decode_encode_tagged (d m tag : Nat) (hd : d < 256 ^ 4) (hm : m < 256 ^ 4)
    (ht : tag < 256 ^ 16) :
    decode (encode ⟨d, m, some tag⟩) = some ⟨d, m, some tag⟩ := by
  have l4 := le_length 4
  have l16 := le_length 16
  -- the tag starts after the width and the count, eight bytes in
  have drop8 : (le 4 d ++ (le 4 m ++ le 16 tag)).drop 8 = le 16 tag := by
    rw [← List.append_assoc]
    exact List.drop_left' (by simp [l4])
  simp [encode, decode, taggedVersion, untaggedVersion, l4, l16, drop8,
    unle_le 4 d hd, unle_le 4 m hm, unle_le 16 tag ht, List.take_left', List.drop_left',
    List.append_assoc]

-- Every record that fits its types reads back as written.
theorem decode_encode (t : Trained) (h : Fits t) : decode (encode t) = some t := by
  obtain ⟨d, m, training⟩ := t
  obtain ⟨hd, hm, ht⟩ := h
  cases training with
  | none => exact decode_encode_untagged d m hd hm
  | some tag => exact decode_encode_tagged d m tag hd hm (ht tag rfl)

-- The node's shared quantizer for one index: nothing, or a tag and the
-- codebooks it was built from (`Atom<Option<Arc<SharedQuantizer>>>`).
abbrev Shared (β : Type) := Option (Nat × β)

-- `CodeCache::quantizer`: the codebooks if the tag matches, else nothing.
def lookup {β : Type} (s : Shared β) (tag : Nat) : Option β :=
  match s with
  | some (t, b) => if t = tag then some b else none
  | none => none

-- `CodeCache::share_quantizer`: the last share replaces whatever was there.
def share {β : Type} (_ : Shared β) (tag : Nat) (b : β) : Shared β := some (tag, b)

-- A share is what the next lookup of its tag finds.
theorem lookup_share {β : Type} (s : Shared β) (tag : Nat) (b : β) :
    lookup (share s tag b) tag = some b := by
  simp [share, lookup]

-- A lookup of another tag finds nothing, so that writer builds its own.
theorem lookup_other_tag {β : Type} (s : Shared β) (tag other : Nat) (b : β)
    (h : tag ≠ other) : lookup (share s tag b) other = none := by
  simp [share, lookup, h]

-- When every share pairs a tag with that tag's codebooks (`books`), a hit
-- is always the codebooks of the tag asked for.
theorem lookup_is_books {β : Type} (books : Nat → β) (s : Shared β)
    (consistent : ∀ t b, s = some (t, b) → b = books t) (tag : Nat) (b : β)
    (hit : lookup s tag = some b) : b = books tag := by
  match s, hit with
  | some (t, b'), hit =>
    simp only [lookup] at hit
    split at hit
    · rename_i heq
      cases hit
      subst heq
      exact consistent _ _ rfl
    · cases hit

-- Sharing a tag with that tag's codebooks keeps the cache consistent.
theorem share_keeps_consistent {β : Type} (books : Nat → β) (s : Shared β) (tag : Nat) :
    ∀ t b, share s tag (books tag) = some (t, b) → b = books t := by
  intro t b h
  simp [share] at h
  obtain ⟨rfl, rfl⟩ := h
  rfl

-- Without one tag per set of codebooks the law fails: an aborted attempt
-- shares (7, a), its retry commits 7 with b, and a writer reading 7 gets a.
theorem reused_tag_serves_other_books {β : Type} (books : Nat → β) (a : β)
    (retried : a ≠ books 7) :
    lookup (share none 7 a) 7 = some a ∧ a ≠ books 7 :=
  ⟨lookup_share none 7 a, retried⟩

end SharedQuantizer
