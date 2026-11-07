import Lean

/-!
A node administrator grants one actor a permission for one named source. Other
sources, direct actor grants, node ownership, and administration remain
independent. A transaction stages the source relationship only when its actor,
authority, provider, and writable transaction are valid. Regolith's transaction
validation and the native Zanzibar relationship store are external premises.
Rust owners: acp node source relationships, `source_permission_job_for` in
query `txn/source_nac.rs`, and defra-node transactional NAC.
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
-- `trustedEmbedder` marks the embedder-authenticated actor call on a node-owned
-- transaction; an HTTP or caller-owned call is not trusted. `edgeChanged` says
-- whether the edge differs from the one the transaction already reads.
structure Admission where
  sameOwner : Bool
  writable : Bool
  authorized : Bool
  boundStore : Bool
  fresh : Bool
  validSource : Bool
  trustedEmbedder : Bool
  edgeChanged : Bool
  deriving FromJson, ToJson

/-- An exactly present add or exactly absent delete changes no relationship, so
the trusted embedder needs no authority to restate it, as
`HostMutationProjection.unchanged_host_source_allowed` says for the host. The
read that found the edge unchanged stays in the transaction and is validated at
commit. An untrusted caller still needs authority, so a no-op cannot probe
whether an edge it may not administer exists. -/
def needsAuthority (i : Admission) : Bool :=
  i.edgeChanged || !i.trustedEmbedder

-- Every admission fact must hold before the relationship can be staged.
def mayStage (i : Admission) : Bool :=
  i.sameOwner && i.writable && i.boundStore && i.fresh && i.validSource &&
    (!needsAuthority i || i.authorized)

-- Unauthorized actors cannot create or revoke a source grant.
theorem denied (i : Admission) (h : i.authorized = false) (changed : i.edgeChanged = true) :
    mayStage i = false := by
  simp [mayStage, needsAuthority, h, changed]

-- An untrusted caller cannot restate an edge it has no authority for.
theorem untrusted_noop_denied (i : Admission) (h : i.authorized = false)
    (untrusted : i.trustedEmbedder = false) : mayStage i = false := by
  simp [mayStage, needsAuthority, h, untrusted]

-- The trusted embedder restates an unchanged edge without authority.
theorem embedded_unchanged_allowed (i : Admission)
    (owner : i.sameOwner = true) (write : i.writable = true) (bound : i.boundStore = true)
    (fresh : i.fresh = true) (valid : i.validSource = true)
    (trusted : i.trustedEmbedder = true) (unchanged : i.edgeChanged = false) :
    mayStage i = true := by
  simp [mayStage, needsAuthority, owner, write, bound, fresh, valid, trusted, unchanged]

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
