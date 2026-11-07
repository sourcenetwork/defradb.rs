import HostMutationProjection
import Lean

/-!
A node administrator grants one actor a permission for one named source. Other
sources, direct actor grants, node ownership, and administration remain
independent. A transaction stages the source relationship only when its actor,
authority, provider, and writable transaction are valid. Regolith's transaction
validation and the native Zanzibar relationship store are external premises.
Rust owners: acp node source relationships, `source_permission_job_for` in
query `txn/source_nac.rs`, and defra-node transactional NAC.

After a trusted embedder stages or restates a root, the host's projector
replays the root's descendants (the membership edges beneath it) as the same
actor, in the same transaction. A descendant that is already present needs no
authority; a missing one needs the actor's own authority; node authority is
never borrowed. A refused descendant rolls the whole statement back. An
untrusted request caller gets no replay. Rust owners: `source_permission_job_for`
in query `txn/source_nac.rs` and `DocumentMutationContext::change_source_permission`
in db `document_projection.rs`.
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

-- One desired membership edge beneath the root, as the projector sees it.
-- `present` says the transaction already holds the edge. `actorAuthorized` is
-- the actor's own native authority for it (`NodeAcpDelegation.authorized`).
-- `nodeAuthorized` is what the node itself could do; replay must ignore it.
structure Descendant where
  present : Bool
  actorAuthorized : Bool
  nodeAuthorized : Bool
  deriving FromJson, ToJson

-- Only the trusted embedder replays; an untrusted request caller does not.
def replays (i : Admission) : Bool := i.trustedEmbedder

/-- A descendant replays under the held-bounded rule the host replay uses: an
unchanged edge needs nothing, a missing one needs the actor's own authority.
`nodeAuthorized` is deliberately absent, so the node is never borrowed. -/
def descendantAllowed (d : Descendant) : Bool :=
  HostMutationProjection.hostReplayAllowed d.present d.actorAuthorized

-- Replay succeeds only when every descendant is allowed.
def replayAllowed (ds : List Descendant) : Bool := ds.all descendantAllowed

-- After a successful replay each desired descendant is present.
def reconcile (d : Descendant) : Descendant := { d with present := true }

/-- The statement's outcome: `none` is a refusal that rolls the root and every
descendant back; `some` is what the transaction holds afterwards. -/
def outcome (i : Admission) (ds : List Descendant) : Option (List Descendant) :=
  if !mayStage i then none
  else if !replays i then some ds
  else if replayAllowed ds then some (ds.map reconcile)
  else none

-- Replay reconciles: after a trusted embedder commit every descendant exists.
theorem embedder_replay_reconciles (i : Admission) (ds out : List Descendant)
    (trusted : i.trustedEmbedder = true) (h : outcome i ds = some out) :
    ∀ d ∈ out, d.present = true := by
  unfold outcome at h
  by_cases stage : mayStage i <;> simp [stage, replays, trusted] at h
  by_cases ok : replayAllowed ds <;> simp [ok] at h
  subst h
  intro d hd
  simp only [List.mem_map] at hd
  obtain ⟨_, _, rfl⟩ := hd
  rfl

-- Replay never widens: every descendant it had to create was the actor's to create.
theorem embedder_replay_never_widens (i : Admission) (ds out : List Descendant)
    (trusted : i.trustedEmbedder = true) (h : outcome i ds = some out) :
    ∀ d ∈ ds, d.present = false → d.actorAuthorized = true := by
  unfold outcome at h
  by_cases stage : mayStage i <;> simp [stage, replays, trusted] at h
  by_cases ok : replayAllowed ds <;> simp [ok] at h
  intro d hd missing
  have := List.all_eq_true.mp ok d hd
  simpa [descendantAllowed, HostMutationProjection.hostReplayAllowed, missing] using this

-- A missing descendant the actor cannot create refuses the whole statement.
theorem embedder_missing_unauthorized_refused (i : Admission) (ds : List Descendant)
    (d : Descendant) (mem : d ∈ ds) (trusted : i.trustedEmbedder = true)
    (missing : d.present = false) (denied : d.actorAuthorized = false) :
    outcome i ds = none := by
  have : replayAllowed ds = false := by
    cases hall : replayAllowed ds
    · rfl
    · have := List.all_eq_true.mp hall d mem
      simp [descendantAllowed, HostMutationProjection.hostReplayAllowed, missing, denied] at this
  unfold outcome
  by_cases stage : mayStage i <;> simp [stage, replays, trusted, this]

-- A complete projection replays without any authority at all.
theorem embedder_complete_replay_needs_no_authority (ds : List Descendant)
    (complete : ∀ d ∈ ds, d.present = true) : replayAllowed ds = true := by
  apply List.all_eq_true.mpr
  intro d hd
  simp [descendantAllowed, HostMutationProjection.hostReplayAllowed, complete d hd]

-- Node authority never decides a descendant: replay cannot borrow it.
theorem embedder_replay_ignores_node_authority (d : Descendant) (node : Bool) :
    descendantAllowed { d with nodeAuthorized := node } = descendantAllowed d := by
  rfl

-- An untrusted request caller gets no replay: its descendants stay as they were.
theorem untrusted_no_replay (i : Admission) (ds out : List Descendant)
    (untrusted : i.trustedEmbedder = false) (h : outcome i ds = some out) : out = ds := by
  unfold outcome at h
  by_cases stage : mayStage i <;> simp [stage, replays, untrusted] at h
  exact h.symm

-- Emit modeled admission outcomes for serialized native conformance inputs.
-- An input may name the root's `descendants`; without them the root has none.
def evaluate (json : Json) : Except String Json := do
  let rows ← json.getArr?
  let outputs ← rows.mapM fun row => do
    let input : Admission ← fromJson? row
    let descendants : List Descendant :=
      ((row.getObjValAs? (List Descendant) "descendants").toOption).getD []
    let result := outcome input descendants
    pure <| Json.mkObj
      [("input", toJson input), ("mayStage", toJson (mayStage input)),
       ("committed", toJson result.isSome),
       ("reconciled", toJson (result.map fun out => out.all (·.present)))]
  pure <| toJson outputs
end SourceScopedNac

-- The native test sends its actual inputs to this executable model.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= SourceScopedNac.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
