/-!
# DiskAnnClusterKeys: cluster-prefixed DiskANN record keys

The story, in plain words. A DiskANN index with resident codes stores one
graph record per node. To keep neighbours next to each other on disk, a
node's record key can start with a cluster tag: four bytes, then the node's
eight-byte id. A record written before clustering existed has the old key:
the id alone. The node's code value says which: the code alone (old layout),
or the code followed by the cluster tag.

A cluster is named by the node that founded it: its tag is the founder's id
cut to 31 bits. Two founders can share a tag; that only puts their clusters
side by side, since the full id still follows the tag in every key.

The in-memory code table keeps one cluster number per code slot, with the
number `2^32 - 1` meaning "no cluster" (an old-layout node).

What must never happen:
* a key does not decode back to the id it was built from, so a scan of the
  records names the wrong node (`graph_id_key`);
* a key does not say its own layout, so a scan cannot tell old from new
  (`graph_cluster_key`);
* two different nodes share a key (`graph_key_injective`);
* a code value does not decode back to its code and cluster
  (`decode_encode_code`, `decode_encode_founder`);
* a code value with a tag no founder can have decodes, where Rust refuses
  it (`decode_rejects_impossible_tag`);
* a real tag collides with the "no cluster" marker, turning a clustered node
  into an old-layout one in memory (`tag_ne_none`, `slot_round_trip`).

Byte orders are modeled abstractly: `Word w` is any fixed-width encoding of
numbers below `2^(8w)` that decodes what it encodes, which Rust's
`to_be_bytes` / `from_be_bytes` and `to_le_bytes` / `from_le_bytes` are. The
laws are fields of the structure, so nothing is assumed beyond them.

Rust anchors: `codec::graph_key`, `codec::graph_id`, `codec::encode_code`,
`codec::decode_code`, `codec::cluster_tag` in
`crates/database/db/src/index/vector/engine/diskann/codec.rs`, and the
cluster slots of `Codes` (`Codes::insert`, `Codes::get`) in `cache.rs`.
-/

namespace DiskAnnClusterKeys

-- A fixed-width number encoding of `w` bytes, with the laws Rust's give.
structure Word (w : Nat) where
  -- encode a number into bytes
  enc : Nat → List Nat
  -- decode bytes into a number
  dec : List Nat → Nat
  -- every encoding is exactly `w` bytes long
  enc_len : ∀ x, (enc x).length = w
  -- decoding an encoding of a number that fits gives the number back
  dec_enc : ∀ x, x < 2 ^ (8 * w) → dec (enc x) = x

-- The two encodings the keys use: four-byte tags and eight-byte ids.
variable (t4 : Word 4) (t8 : Word 8)

-- A record key: the id alone, or the cluster tag followed by the id.
def graphKey (id : Nat) : Option Nat → List Nat
  -- the old layout: eight bytes of id
  | none => t8.enc id
  -- the clustered layout: four bytes of tag, then eight of id
  | some tag => t4.enc tag ++ t8.enc id

-- The id a record key names: the last eight bytes, whichever the layout.
-- (Rust: an eight-byte key is the id; a twelve-byte key is tag then id.)
def graphId (key : List Nat) : Nat :=
  -- skip the tag of a twelve-byte key; an eight-byte key skips nothing
  t8.dec (key.drop (key.length - 8))

-- The cluster a record key names: none for eight bytes, the tag for twelve.
def graphCluster (key : List Nat) : Option Nat :=
  -- the length decides the layout
  if key.length = 12 then some (t4.dec (key.take 4)) else none

-- Promise: a record key decodes back to the id it was built from.
theorem graph_id_key (id : Nat) (c : Option Nat) (hid : id < 2 ^ 64) :
    graphId t8 (graphKey t4 t8 id c) = id := by
  -- one case per layout
  cases c with
  | none =>
    -- the key is the id's eight bytes, nothing is dropped
    simp [graphKey, graphId, t8.enc_len, t8.dec_enc id hid]
  | some tag =>
    -- the key is four tag bytes then eight id bytes; the four are dropped
    simp [graphKey, graphId, t4.enc_len, t8.enc_len, List.drop_left' (t4.enc_len tag),
      t8.dec_enc id hid]

-- Promise: a record key says its layout, and a clustered one its tag.
theorem graph_cluster_key (id : Nat) (c : Option Nat) (htag : ∀ t, c = some t → t < 2 ^ 32) :
    graphCluster t4 (graphKey t4 t8 id c) = c := by
  -- one case per layout
  cases c with
  | none =>
    -- eight bytes is not twelve, so no cluster
    simp [graphKey, graphCluster, t8.enc_len]
  | some tag =>
    -- twelve bytes; the first four are the tag's
    have h := htag tag rfl
    simp [graphKey, graphCluster, t4.enc_len, t8.enc_len,
      List.take_left' (t4.enc_len tag), t4.dec_enc tag h]

-- Promise: two different nodes never share a record key, whatever clusters
-- they are in.
theorem graph_key_injective (a b : Nat) (ca cb : Option Nat)
    (ha : a < 2 ^ 64) (hb : b < 2 ^ 64)
    (h : graphKey t4 t8 a ca = graphKey t4 t8 b cb) : a = b := by
  -- both keys decode to their ids, and they are the same key
  rw [← graph_id_key t4 t8 a ca ha, ← graph_id_key t4 t8 b cb hb, h]

-- A code value: the code alone (old layout), or the code then the tag.
def encodeCode (code : List Nat) : Option Nat → List Nat
  -- the old layout: just the code
  | none => code
  -- the clustered layout: the code, then four bytes of tag
  | some tag => code ++ t4.enc tag

-- Decode a code value of a code `m` bytes long: `none` when the length is
-- neither `m` nor `m + 4`, or when the tag is above `0x7fff_ffff`, which no
-- founder can have (Rust: an error, `CLUSTER_TAG_MASK` bounds the tag).
def decodeCode (m : Nat) (bytes : List Nat) : Option (List Nat × Option Nat) :=
  -- just the code: the old layout
  if bytes.length = m then some (bytes, none)
  -- the code and a tag no founder can have: corrupt
  else if bytes.length = m + 4 then
    -- the tag a founder can have: the clustered layout
    if t4.dec (bytes.drop m) ≤ 0x7fff_ffff then some (bytes.take m, some (t4.dec (bytes.drop m)))
    -- a tag with its top bit set: corrupt
    else none
  -- anything else is corrupt
  else none

-- Promise: a code value decodes back to the code and cluster it was built
-- from, for every tag below `2^31`, which is every tag a founder can have.
theorem decode_encode_code (code : List Nat) (c : Option Nat)
    (htag : ∀ t, c = some t → t < 2 ^ 31) :
    decodeCode t4 code.length (encodeCode t4 code c) = some (code, c) := by
  -- one case per layout
  cases c with
  | none =>
    -- the value is the code, whose length is `m`
    simp [encodeCode, decodeCode]
  | some tag =>
    -- the tag is below `2^31`, so it also fits the four bytes it is given
    have h := htag tag rfl
    -- and so it is at most `0x7fff_ffff`, which the decoder admits
    have hfit : tag < 2 ^ (8 * 4) := Nat.lt_trans h (by decide)
    -- the bound the decoder checks, in its own form
    have hle : tag ≤ 0x7fff_ffff := by omega
    -- the value is the code and four tag bytes: `m + 4`, not `m`
    simp [encodeCode, decodeCode, t4.enc_len, t4.dec_enc tag hfit, hle]

-- Promise: a code value whose tag is above `0x7fff_ffff` decodes to nothing,
-- as Rust's `decode_code` refuses it.
theorem decode_rejects_impossible_tag (code : List Nat) (tag : Nat)
    (hfit : tag < 2 ^ 32) (hbig : 0x7fff_ffff < tag) :
    decodeCode t4 code.length (encodeCode t4 code (some tag)) = none := by
  -- the four tag bytes decode back to the tag
  have hdec : t4.dec (t4.enc tag) = tag := t4.dec_enc tag hfit
  -- which is not at most `0x7fff_ffff`
  have hnot : ¬ tag ≤ 0x7fff_ffff := by omega
  -- the value is `m + 4` long and its tag fails the check
  simp [encodeCode, decodeCode, t4.enc_len, hdec, hnot]

/-! ## Tags and the "no cluster" slot marker -/

-- The tag of a cluster founded by node `founder`: its id cut to 31 bits.
-- (Rust: `(founder as u32) & 0x7fff_ffff`, which is the id mod `2^31`.)
def clusterTag (founder : Nat) : Nat := founder % 2 ^ 31

-- The slot marker for a node without a cluster (Rust: `u32::MAX`).
def noCluster : Nat := 2 ^ 32 - 1

-- Promise: every tag fits the four bytes a key gives it.
theorem tag_fits (founder : Nat) : clusterTag founder < 2 ^ 32 := by
  -- a remainder mod `2^31` is below `2^31`, which is below `2^32`
  have h : founder % 2 ^ 31 < 2 ^ 31 := Nat.mod_lt _ (by decide)
  -- and `2^31 < 2^32`
  exact Nat.lt_trans h (by decide)

-- Promise: every tag is below `2^31`, the bound `decodeCode` admits.
theorem tag_below_bound (founder : Nat) : clusterTag founder < 2 ^ 31 := by
  -- a remainder mod `2^31` is below `2^31`
  exact Nat.mod_lt _ (by decide)

-- Promise: a code value with a founder's tag decodes back to its code and
-- that tag.
theorem decode_encode_founder (code : List Nat) (founder : Nat) :
    decodeCode t4 code.length (encodeCode t4 code (some (clusterTag founder))) =
      some (code, some (clusterTag founder)) := by
  -- the founder's tag is below `2^31`, so the round trip applies
  exact decode_encode_code t4 code _ (by intro t h; cases h; exact tag_below_bound founder)

-- Promise: no tag is the "no cluster" marker.
theorem tag_ne_none (founder : Nat) : clusterTag founder ≠ noCluster := by
  -- a tag is below `2^31`, the marker is `2^32 - 1`
  have h : founder % 2 ^ 31 < 2 ^ 31 := Nat.mod_lt _ (by decide)
  -- so they cannot be equal
  intro heq
  -- rewrite the tag as the marker and compare
  simp only [clusterTag, noCluster] at heq
  -- `2^32 - 1 < 2^31` is false
  omega

-- What a code slot stores for a cluster (Rust: `Codes::insert`).
def toSlot : Option Nat → Nat
  -- no cluster: the marker
  | none => noCluster
  -- a cluster: its tag
  | some tag => tag

-- What a code slot reads back as (Rust: `Codes::get`).
def fromSlot (n : Nat) : Option Nat :=
  -- the marker reads as no cluster; anything else is the tag
  if n = noCluster then none else some n

-- Promise: a slot reads back the cluster it was given, for every tag a
-- founder can have and for no cluster.
theorem slot_round_trip (c : Option Nat) (hc : ∀ t, c = some t → ∃ f, t = clusterTag f) :
    fromSlot (toSlot c) = c := by
  -- one case per layout
  cases c with
  | none =>
    -- the marker reads as no cluster
    simp [toSlot, fromSlot]
  | some tag =>
    -- a tag is some founder's, so it is not the marker
    obtain ⟨f, rfl⟩ := hc _ rfl
    simp [toSlot, fromSlot, tag_ne_none f]

end DiskAnnClusterKeys
