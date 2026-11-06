import Lean

/-!
A node administrator grants one actor a permission for one named source. Other
sources, direct actor grants, node ownership, and administration remain
independent. A transaction stages the source relationship only when its actor,
authority, provider, and writable transaction are valid. Regolith's transaction
validation and the native Zanzibar relationship store are external premises.
Rust owners: acp node source relationships and defra-node transactional NAC.
-/
namespace SourceScopedNac
open Lean

-- One source grant is identified by its source, actor, and node permission.
structure Key where
  source : String
  actor : String
  permission : String
  deriving BEq, DecidableEq, FromJson, ToJson

-- The grant set stores each source independently from every other grant.
def remove (grants : Key → Bool) (key : Key) : Key → Bool :=
  fun item => if item = key then false else grants item

-- Removing a source leaves every other source and actor untouched.
theorem remove_other (grants : Key → Bool) (key other : Key) (h : other ≠ key) :
    remove grants key other = grants other := by simp [remove, h]

-- A removed source cannot still grant the same actor and permission.
theorem remove_self (grants : Key → Bool) (key : Key) :
    remove grants key key = false := by simp [remove]

-- Direct, administrator, and owner grants are separate effective paths.
def allowed (direct admin owner sourced : Bool) : Bool := direct || admin || owner || sourced

-- Removing a source does not remove any independent direct grant.
theorem direct_survives (admin owner sourced : Bool) :
    allowed true admin owner sourced = true := by simp [allowed]

-- Removing a source does not remove the owner's implicit permission.
theorem owner_survives (direct admin sourced : Bool) :
    allowed direct admin true sourced = true := by simp [allowed]

-- Removing a source does not remove an administrator's implicit permission.
theorem admin_survives (direct owner sourced : Bool) :
    allowed direct true owner sourced = true := by simp [allowed]

-- A nested source path grants only while each relationship remains present.
def nested (sourceEdge contextEdge membership : Bool) : Bool :=
  sourceEdge && contextEdge && membership

-- Disabling a shared source makes every membership on its path dormant.
theorem disabled_source_denies (contextEdge membership : Bool) :
    nested false contextEdge membership = false := by simp [nested]

-- An unlinked context cannot grant through an otherwise enabled source.
theorem unlinked_context_denies (sourceEdge membership : Bool) :
    nested sourceEdge false membership = false := by simp [nested]

-- Native authority is checked in the same writable, authenticated transaction.
structure Admission where
  sameOwner : Bool
  writable : Bool
  authorized : Bool
  boundStore : Bool
  fresh : Bool
  validSource : Bool
  deriving FromJson, ToJson

-- Every admission fact must hold before the relationship can be staged.
def mayStage (i : Admission) : Bool :=
  i.sameOwner && i.writable && i.authorized && i.boundStore && i.fresh && i.validSource

-- Unauthorized actors cannot create or revoke a source grant.
theorem denied (i : Admission) (h : i.authorized = false) : mayStage i = false := by
  simp [mayStage, h]

-- A foreign transaction never accepts a source mutation.
theorem wrongOwner (i : Admission) (h : i.sameOwner = false) : mayStage i = false := by
  simp [mayStage, h]

-- The source must identify a valid independent relationship.
theorem invalidSource (i : Admission) (h : i.validSource = false) : mayStage i = false := by
  simp [mayStage, h]

-- Emit modeled admission outcomes for serialized native conformance inputs.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Admission ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("mayStage", toJson (mayStage input))]
end SourceScopedNac

-- The native test sends its actual inputs to this executable model.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= SourceScopedNac.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
