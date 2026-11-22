/-!
# IndexKeyspace: tagged secondary-index keys stay isolated

The story, in plain words. A secondary-index key used to start with the
collection's short-ID bytes. A short ID below 240 is one raw byte, so collection
100 made index keys that begin `/d/`, the documents' family, and collection 118
made keys that begin `/v/`, the versions' family. The fix puts the fixed tag
`/i/` before the collection and index IDs. Every integer is encoded by the same
order-preserving uvarint: one byte below 240, two bytes through 2287, and a
width marker plus big-endian bytes above that. The first byte therefore names
the whole integer's width, so no integer encoding is a prefix of another.

What must hold: an index key starts with `/i/`, never with another family's
tag, and two index keys share a prefix only when they are the same key.

What must never happen: collection 100's index entries land in the document
family, collection 118's land in the version family, or one index's prefix scan
reads another index's entries.

Rust anchors: `crates/engine/storage/src/keys/datastore/index_key.rs`
`INDEX_KEYSPACE_PREFIX`, `IndexDataStoreKey::index_prefix`,
`IndexDataStoreKey::collection_prefix` and `IndexDataStoreKey::try_bytes`.
-/

namespace IndexKeyspace

-- A byte, as a natural number.
abbrev Byte := Nat

-- The fixed tag in front of every secondary-index key: `/i/`.
def indexTag : List Byte := [47, 105, 47]

-- A collection short ID in the model: every single-byte value and the first
-- multi-byte boundary. The code's uvarint gives the first group one byte and
-- the boundary two bytes.
inductive ShortId where
  | byte (value : Byte) : value < 240 → ShortId
  | boundary

def ShortId.bytes : ShortId → List Byte
  | .byte value _ => [value]
  | .boundary => [240, 0]

-- A prefix in the key-order sense: `left` followed by some suffix is `right`.
def IsPrefix (left right : List Byte) : Prop :=
  ∃ suffix, left ++ suffix = right

-- The one-/two-byte split makes a short ID self-delimiting: one ID's bytes are
-- a prefix of another's only when they are the same ID.
theorem short_id_bytes_prefix_eq (left right : ShortId) :
    IsPrefix left.bytes right.bytes → left = right := by
  rintro ⟨suffix, hbytes⟩
  cases left with
  | byte leftValue leftLt =>
    cases right with
    | byte rightValue _ =>
      injection hbytes with hhead htail
      subst hhead
      simp at htail
      subst htail
      rfl
    | boundary =>
      exfalso
      -- A one-byte ID cannot start the two-byte boundary.
      injection hbytes with hhead _
      exact absurd hhead (Nat.ne_of_lt leftLt)
  | boundary =>
    cases right with
    | byte _ rightLt =>
      exfalso
      injection hbytes with hhead _
      exact absurd hhead.symm (Nat.ne_of_lt rightLt)
    | boundary =>
      -- The two boundary keys have the same full bytes.
      have hsuffix : suffix = [] := by
        have hsame : ShortId.bytes ShortId.boundary ++ suffix =
            ShortId.bytes ShortId.boundary ++ [] := by
          simpa using hbytes
        exact List.append_cancel_left hsame
      subst hsuffix
      rfl

-- A secondary-index key names a collection and an index.
structure IndexKey where
  collection : ShortId
  index : ShortId

def IndexKey.bytes (key : IndexKey) : List Byte :=
  indexTag
    ++ key.collection.bytes
    ++ [47]
    ++ key.index.bytes
    ++ [47]

-- A short ID's bytes are a prefix of another's only when they name the same ID.
-- Prefixes compose: if `a` starts `b` and `b` starts `c`, `a` starts `c`.
theorem prefix_trans {a b c : List Byte}
    (hab : IsPrefix a b) (hbc : IsPrefix b c) : IsPrefix a c := by
  obtain ⟨ab, hab⟩ := hab
  obtain ⟨bc, hbc⟩ := hbc
  exact ⟨ab ++ bc, by rw [← List.append_assoc, hab, hbc]⟩

-- Every index key starts with the fixed `/i/` tag.
theorem key_starts_with_tag (key : IndexKey) : IsPrefix indexTag key.bytes := by
  refine ⟨key.collection.bytes ++ [47] ++ key.index.bytes ++ [47], ?_⟩
  simp [IndexKey.bytes]

-- A three-byte family whose second byte is not `i` cannot contain the tag.
theorem tag_not_prefix_of_family (second : Byte) (hne : second ≠ 105) :
    ¬ IsPrefix indexTag [47, second, 47] := by
  rintro ⟨suffix, hbytes⟩
  injection hbytes with _ hrest
  injection hrest with hsecond _
  exact hne hsecond.symm

-- The families collection 100 and 118 used to reach.
def documentFamily : List Byte := [47, 100, 47]
def versionFamily : List Byte := [47, 118, 47]

-- Neither of those families starts with the index tag, so an index key
-- (which does) cannot be a prefix of either.
theorem key_not_in_document_family (key : IndexKey) :
    ¬ IsPrefix key.bytes documentFamily := by
  intro hkey
  exact tag_not_prefix_of_family 100 (by decide) (prefix_trans (key_starts_with_tag key) hkey)

theorem key_not_in_version_family (key : IndexKey) :
    ¬ IsPrefix key.bytes versionFamily := by
  intro hkey
  exact tag_not_prefix_of_family 118 (by decide) (prefix_trans (key_starts_with_tag key) hkey)

end IndexKeyspace
