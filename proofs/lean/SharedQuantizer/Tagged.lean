/-!
# SharedQuantizer.Tagged: a trained record that names its training, and a cache keyed by it

The story, in plain words. A trained DiskANN index stores a small trained
record: its width, its subspace count and, since this change, a tag the
training drew at random when it wrote its codebooks. Older records have no
tag. A node keeps an index's quantizers in memory, each under the tag of
the codebooks it was built from, and a writer uses one only when its own
snapshot's record carries the same tag. It keeps the last two trainings
shared, so writers that began before a rebuild and writers that began after
it each find theirs.

Two things have to hold for that to be safe. The record must read back as
written, in both its encodings, so a tag is never lost or invented on the
way through the store. And the cache must only ever answer a tag with the
codebooks that tag names, which holds as long as every share pairs a tag
with that tag's codebooks.

A tiny example. Tag 7 names codebooks A. The cache holds (7, A). A writer
whose record says 7 gets A; a writer whose record says 9 gets nothing,
builds its own B and shares it: the cache holds (9, B) and (7, A), and the
next writer reading 7 still gets A.

What must never happen:
* a record with a tag reads back with another tag, or none
  (`decode_encode_tagged`);
* a record without a tag reads back with one (`decode_encode_untagged`);
* the cache answers a tag with codebooks other than that tag's
  (`lookup_is_books`), or a share makes another tag answer with codebooks
  it did not have (`lookup_other_tag`);
* a share of a new training evicts the one shared just before it, so
  writers on the two sides of a rebuild build a quantizer on every
  transaction (`share_keeps_previous`; with one slot it does,
  `one_slot_evicts`);
* and the law does not survive one tag shared with two sets of codebooks,
  which is why a training draws its tag in the attempt that writes them
  (`reused_tag_serves_other_books`).

The order of trainings, retries and writers is the TLA+ model
`proofs/tla/SharedQuantizer.tla`; this file proves the laws it rests on.

Rust anchors: `Trained`, `encode_trained`, `decode_trained`
(`crates/db/src/index/vector/engine/diskann/codec.rs`), and
`CodeCache::quantizer`, `CodeCache::share_quantizer`, `SHARED_TRAININGS`
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

-- The node's shared quantizers for one index, newest first: each a tag and
-- the codebooks it was built from (`Atom<[Option<SharedQuantizer>; 2]>`).
abbrev Shared (β : Type) := List (Nat × β)

-- How many trainings' quantizers the node keeps (`SHARED_TRAININGS`).
def slots : Nat := 2

-- `CodeCache::quantizer`: the codebooks of the entry under the tag, if any.
def lookup {β : Type} : Shared β → Nat → Option β
  | [], _ => none
  | (t, b) :: rest, tag => if t = tag then some b else lookup rest tag

-- `CodeCache::share_quantizer` with room for `n`: a tag already held changes
-- nothing; any other goes in front, and the oldest falls off past `n`.
def shareIn {β : Type} (n : Nat) (s : Shared β) (tag : Nat) (b : β) : Shared β :=
  match lookup s tag with
  | some _ => s
  | none => ((tag, b) :: s).take n

-- The node's share: room for `slots`.
def share {β : Type} (s : Shared β) (tag : Nat) (b : β) : Shared β :=
  shareIn slots s tag b

-- A hit is an entry of the list under that tag.
theorem lookup_mem {β : Type} (s : Shared β) (tag : Nat) (b : β)
    (hit : lookup s tag = some b) : (tag, b) ∈ s := by
  induction s with
  | nil => simp [lookup] at hit
  | cons e rest ih =>
    obtain ⟨t, c⟩ := e
    by_cases heq : t = tag
    · -- the front entry is under the tag, so it is the hit
      simp [lookup, heq] at hit
      subst heq
      simp [hit]
    · -- otherwise the hit is further on
      simp only [lookup, heq, if_false] at hit
      exact List.mem_cons_of_mem _ (ih hit)

-- A lookup in a prefix finds only what the whole list finds.
theorem lookup_take {β : Type} (other : Nat) (x : β) :
    ∀ (n : Nat) (r : Shared β), lookup (r.take n) other = some x →
      lookup r other = some x := by
  intro n
  induction n with
  | zero => intro r hr; simp [lookup] at hr
  | succ n ih =>
    intro r hr
    cases r with
    | nil => simp [lookup] at hr
    | cons e rest =>
      obtain ⟨t, c⟩ := e
      by_cases heq : t = other
      · simpa [lookup, heq] using hr
      · simp only [List.take_succ_cons, lookup, heq, if_false] at hr ⊢
        exact ih rest hr

-- A share of a tag the node does not hold is what the next lookup finds.
theorem lookup_share {β : Type} (s : Shared β) (tag : Nat) (b : β)
    (fresh : lookup s tag = none) : lookup (share s tag b) tag = some b := by
  simp [share, shareIn, slots, fresh, lookup]

-- A share of a tag the node holds changes nothing, so its quantizer stays.
theorem share_held {β : Type} (s : Shared β) (tag : Nat) (b b' : β)
    (held : lookup s tag = some b') : share s tag b = s := by
  simp [share, shareIn, held]

-- A share never makes another tag answer with codebooks it did not have.
theorem lookup_other_tag {β : Type} (s : Shared β) (tag other : Nat) (b x : β)
    (h : tag ≠ other) (hit : lookup (share s tag b) other = some x) :
    lookup s other = some x := by
  unfold share shareIn at hit
  cases hs : lookup s tag with
  | some _ => simpa [hs] using hit
  | none =>
    -- the new front entry is under `tag`, so `other` is found further on
    rw [hs] at hit
    have whole := lookup_take other x slots _ hit
    simpa [lookup, h] using whole

-- Writers on both sides of a rebuild: the newest share stays found when a
-- training it does not name is shared next.
theorem share_keeps_previous {β : Type} (t0 tag : Nat) (b0 b : β) (rest : Shared β)
    (fresh : lookup ((t0, b0) :: rest) tag = none) :
    lookup (share ((t0, b0) :: rest) tag b) t0 = some b0 := by
  have hne : t0 ≠ tag := by
    intro heq
    subst heq
    simp [lookup] at fresh
  unfold share shareIn
  rw [fresh]
  simp [slots, lookup, Ne.symm hne]

-- With room for one, the same two shares evict the older: both sides of a
-- rebuild would build again.
theorem one_slot_evicts {β : Type} (a b : β) :
    lookup (shareIn 1 (shareIn 1 [] 1 a) 2 b) 1 = none := by
  simp [shareIn, lookup]

-- A share keeps the node within its slots.
theorem share_length {β : Type} (s : Shared β) (tag : Nat) (b : β)
    (h : s.length ≤ slots) : (share s tag b).length ≤ slots := by
  unfold share shareIn
  cases hs : lookup s tag with
  | some _ => simpa [hs] using h
  | none => simpa [hs] using Nat.min_le_left slots (s.length + 1)

-- Every entry pairs its tag with that tag's codebooks (`books`).
def Consistent {β : Type} (books : Nat → β) (s : Shared β) : Prop :=
  ∀ t b, (t, b) ∈ s → b = books t

-- When the node is consistent, a hit is always the codebooks of the tag
-- asked for.
theorem lookup_is_books {β : Type} (books : Nat → β) (s : Shared β)
    (consistent : Consistent books s) (tag : Nat) (b : β)
    (hit : lookup s tag = some b) : b = books tag :=
  consistent tag b (lookup_mem s tag b hit)

-- Sharing a tag with that tag's codebooks keeps the node consistent.
theorem share_keeps_consistent {β : Type} (books : Nat → β) (s : Shared β)
    (consistent : Consistent books s) (tag : Nat) :
    Consistent books (share s tag (books tag)) := by
  intro t b mem
  unfold share shareIn at mem
  cases hs : lookup s tag with
  | some _ =>
    -- nothing changed
    rw [hs] at mem
    exact consistent t b mem
  | none =>
    -- the new entry is the tag's own codebooks; the rest were consistent
    rw [hs] at mem
    rcases List.mem_cons.mp (List.mem_of_mem_take mem) with h | h
    · cases h
      rfl
    · exact consistent t b h

-- Without one tag per set of codebooks the law fails: an aborted attempt
-- shares (7, a), its retry commits 7 with b, and a writer reading 7 gets a.
theorem reused_tag_serves_other_books {β : Type} (books : Nat → β) (a : β)
    (retried : a ≠ books 7) :
    lookup (share [] 7 a) 7 = some a ∧ a ≠ books 7 :=
  ⟨lookup_share [] 7 a rfl, retried⟩

end SharedQuantizer
