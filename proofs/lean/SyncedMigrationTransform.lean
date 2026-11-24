/-! The definition merge in `crates/database/db/src/merge/merge_handler/definition.rs`
replaces a placeholder with a synced definition. The incoming block gives its
predecessor but not the migration transform; only a placeholder naming that
predecessor may supply it. An already held real definition is not rebuilt.
This model assumes the read and store succeed and abstracts all other fields. -/
namespace SyncedMigrationTransform

-- An edge names its predecessor and optionally carries a migration transform.
structure Edge where
  predecessor : Nat
  transform : Option Nat
  deriving DecidableEq

-- A locally held version is either a migration placeholder or a real definition.
structure Version where
  placeholder : Bool
  previous : Option Edge
  deriving DecidableEq

-- A placeholder may supply its transform only for the incoming head.
def matchingTransform (head : Nat) (held : Option Version) : Option Nat :=
  match held with
  | some ⟨true, some edge⟩ => if edge.predecessor = head then edge.transform else none
  | _ => none

-- A real version is kept; otherwise a patch uses the incoming head and the
-- matching placeholder transform. A headless definition keeps its base edge.
def merge (base : Version) (head : Option Nat) (held : Option Version) : Version :=
  match held with
  | some ⟨false, previous⟩ => ⟨false, previous⟩
  | _ => ⟨false, match head with
      | none => base.previous
      | some h => some ⟨h, matchingTransform h held⟩⟩

-- A matching placeholder's transform survives the incoming definition.
theorem matching_preserved (base : Version) (head transform : Nat) :
    (merge base (some head) (some ⟨true, some ⟨head, some transform⟩⟩)).previous =
      some ⟨head, some transform⟩ := by
  simp [merge, matchingTransform]

-- A placeholder for another predecessor cannot attach its transform.
theorem mismatched_not_carried (base : Version) (head other transform : Nat)
    (different : other ≠ head) :
    (merge base (some head) (some ⟨true, some ⟨other, some transform⟩⟩)).previous =
      some ⟨head, none⟩ := by
  simp [merge, matchingTransform, different]

-- A placeholder with no edge cannot supply a transform.
theorem no_edge_no_transform (base : Version) (head : Nat) :
    (merge base (some head) (some ⟨true, none⟩)).previous = some ⟨head, none⟩ := by
  rfl

-- The same version already held as a real definition is not rebuilt.
theorem real_unchanged (base : Version) (head : Option Nat) (previous : Option Edge) :
    merge base head (some ⟨false, previous⟩) = ⟨false, previous⟩ := by
  rfl

-- With no local version, a patch has no migration transform to carry.
theorem absent_no_transform (base : Version) (head : Nat) :
    (merge base (some head) none).previous = some ⟨head, none⟩ := by
  rfl

end SyncedMigrationTransform
