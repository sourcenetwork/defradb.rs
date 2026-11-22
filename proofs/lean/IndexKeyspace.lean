/-!
# IndexKeyspace: tagged secondary-index keys stay isolated

A secondary-index key used to start with the collection's short-ID bytes.
A short ID below 240 is one raw byte, so collection 100 made index keys
that begin `/d/`, and collection 118 made keys that begin `/v/`. The fixed
`/index/` tag separates new index keys from those families regardless of the
bytes that follow it. The legacy-key theorem covers one-byte collection IDs.

The short-ID model covers values below 240 and the value 240 only. It proves
prefix-freedom for that subset, not for every width of the Rust encoder. The
model's `IndexKey.bytes` is an index prefix, not a complete index entry; it
omits encoded fields and document IDs. It does not prove index-prefix
isolation between different collection or index IDs.

Rust anchors: `crates/engine/storage/src/keys/datastore/index_key.rs`
`INDEX_KEYSPACE_PREFIX`, `IndexDataStoreKey::index_prefix`,
`IndexDataStoreKey::collection_prefix` and `IndexDataStoreKey::try_bytes`.
-/

namespace IndexKeyspace

-- A byte, as a natural number.
abbrev Byte := Nat

-- The fixed tag in front of every secondary-index key: `/index/`.
def indexTag : List Byte := [47, 105, 110, 100, 101, 120, 47]

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

-- Within the modeled subset, one ID's bytes prefix another only when the
-- IDs are the same.
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

-- An index prefix names a collection and an index.
structure IndexKey where
  collection : ShortId
  index : ShortId

def IndexKey.bytes (key : IndexKey) : List Byte :=
  indexTag
    ++ key.collection.bytes
    ++ [47]
    ++ key.index.bytes
    ++ [47]

-- Prefixes compose: if `a` starts `b` and `b` starts `c`, `a` starts `c`.
theorem prefix_trans {a b c : List Byte}
    (hab : IsPrefix a b) (hbc : IsPrefix b c) : IsPrefix a c := by
  obtain ⟨ab, hab⟩ := hab
  obtain ⟨bc, hbc⟩ := hbc
  exact ⟨ab ++ bc, by rw [← List.append_assoc, hab, hbc]⟩

-- Every modeled index prefix starts with the fixed `/index/` tag.
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

-- A legacy key's one-byte collection ID is followed immediately by a slash.
-- The new tag has another character there, even when that byte is `i`.
theorem tag_not_prefix_of_legacy (first : Byte) (tail : List Byte) :
    ¬ IsPrefix indexTag ([47, first, 47] ++ tail) := by
  rintro ⟨suffix, hbytes⟩
  simp [indexTag] at hbytes

-- The families collection 100 and 118 used to reach.
def documentFamily : List Byte := [47, 100, 47]
def versionFamily : List Byte := [47, 118, 47]

-- The family prefixes cannot start any tagged index key, regardless of the
-- collection ID, index ID, field values or document ID that follow the tag.
theorem key_not_in_document_family (suffix : List Byte) :
    ¬ IsPrefix documentFamily (indexTag ++ suffix) := by
  rintro ⟨tail, hbytes⟩
  simp [documentFamily, indexTag] at hbytes

theorem key_not_in_version_family (suffix : List Byte) :
    ¬ IsPrefix versionFamily (indexTag ++ suffix) := by
  rintro ⟨tail, hbytes⟩
  simp [versionFamily, indexTag] at hbytes

end IndexKeyspace
