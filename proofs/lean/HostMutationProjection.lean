import Lean

/-!
A host-installed projection is trusted node code, not a request parameter. A
trusted host repair authorizes its source changes as the node administrator;
an ordinary request uses the actual writer's native authority. Both stage in one transaction:
neither a failed projection nor a failed commit publishes either half.

This contract assumes the host receives every relevant resolved document
change, including replicated merges, and can read and modify native ACP through
the same transaction. It does not establish that a storage hook supplies these
mechanics or that the host's projection computes the correct affected keys.
-/
namespace HostMutationProjection

structure Source where
  document : String
  purpose : String
  actor : String
  permission : String
  deriving BEq, DecidableEq

structure State where
  document : String
  direct : Source → Bool
  sourced : Source → Bool

inductive Route where
  | managed | raw | replicated | schema
  deriving BEq, DecidableEq

structure Admission where
  documentAuthorized : Bool
  nodeAdministrator : Bool
  sameTransaction : Bool
  verifiedRegistration : Bool
  projectionSucceeded : Bool
  commitSucceeded : Bool

/-- `affected` and `desired` come from the installed host projector, not the
mutation or transport. Source identities isolate independent grant paths. -/
def publish (s : State) (_route : Route) (a : Admission) (document : String)
    (affected desired : Source → Bool) : State :=
  if a.documentAuthorized && a.nodeAdministrator && a.sameTransaction && a.verifiedRegistration &&
      a.projectionSucceeded && a.commitSucceeded then
    { document := document, direct := s.direct,
      sourced := fun key => if affected key then desired key else s.sourced key }
  else s

theorem rejected_document_changes_nothing (s : State) (route : Route)
    (a : Admission) (document : String) (affected desired : Source → Bool)
    (h : a.documentAuthorized = false) :
    publish s route a document affected desired = s := by
  simp [publish, h]

theorem failed_projection_changes_nothing (s : State) (route : Route)
    (a : Admission) (document : String) (affected desired : Source → Bool)
    (h : a.projectionSucceeded = false) :
    publish s route a document affected desired = s := by
  simp [publish, h]

theorem unregistered_owner_changes_nothing (s : State) (route : Route)
    (a : Admission) (document : String) (affected desired : Source → Bool)
    (h : a.verifiedRegistration = false) :
    publish s route a document affected desired = s := by
  simp [publish, h]

theorem failed_commit_changes_nothing (s : State) (route : Route)
    (a : Admission) (document : String) (affected desired : Source → Bool)
    (h : a.commitSucceeded = false) :
    publish s route a document affected desired = s := by
  simp [publish, h]

theorem direct_grants_unchanged (s : State) (route : Route) (a : Admission)
    (document : String) (affected desired : Source → Bool) :
    (publish s route a document affected desired).direct = s.direct := by
  simp only [publish]
  split <;> rfl

theorem unrelated_source_unchanged (s : State) (route : Route) (a : Admission)
    (document : String) (affected desired : Source → Bool) (key : Source)
    (h : affected key = false) :
    (publish s route a document affected desired).sourced key = s.sourced key := by
  simp only [publish]
  split <;> simp [h]

theorem route_does_not_select_the_projector (s : State) (first second : Route)
    (a : Admission) (document : String) (affected desired : Source → Bool) :
    publish s first a document affected desired = publish s second a document affected desired := by
  rfl

theorem disabled_source_is_removed (s : State) (route : Route) (a : Admission)
    (document : String) (affected desired : Source → Bool) (key : Source)
    (authorized : a.documentAuthorized = true) (admin : a.nodeAdministrator = true)
    (bound : a.sameTransaction = true) (registered : a.verifiedRegistration = true)
    (projected : a.projectionSucceeded = true)
    (committed : a.commitSucceeded = true) (selected : affected key = true)
    (disabled : desired key = false) :
    (publish s route a document affected desired).sourced key = false := by
  simp [publish, authorized, admin, bound, registered, projected, committed, selected, disabled]

/-- The embedded host capability is unavailable to HTTP and GraphQL callers.
The authenticated transaction actor must have native authority for the edge on
the same writable transaction: `actorAuthorized` is `NodeAcpDelegation.authorized`
for that edge (an admin, or a delegate holding the operation and the edge's
permission outside the admin class). A repair acts as Node only when
Node is the actual transaction actor. The capability changes source edges only. -/
structure HostSourceAdmission where
  trustedHost : Bool
  ownerMatches : Bool
  fresh : Bool
  writable : Bool
  configuredNode : Bool
  actorAuthorized : Bool
  sameTransaction : Bool
  edgeChanged : Bool
  deriving Lean.FromJson, Lean.ToJson

def hostSourceAllowed (a : HostSourceAdmission) : Bool :=
  a.trustedHost && a.ownerMatches && a.fresh && a.writable && a.configuredNode &&
    a.sameTransaction && (!a.edgeChanged || a.actorAuthorized)

theorem untrusted_host_source_denied (a : HostSourceAdmission)
    (h : a.trustedHost = false) : hostSourceAllowed a = false := by
  simp [hostSourceAllowed, h]

theorem wrong_transaction_owner_denied (a : HostSourceAdmission)
    (h : a.ownerMatches = false) : hostSourceAllowed a = false := by
  simp [hostSourceAllowed, h]

theorem host_source_requires_edge_authority (a : HostSourceAdmission)
    (changed : a.edgeChanged = true) (h : a.actorAuthorized = false) : hostSourceAllowed a = false := by
  simp [hostSourceAllowed, changed, h]

/-- Edge admission alone needs no new grant for an identical edge. Its
dependent projection still has to succeed in the same transaction. -/
theorem unchanged_host_source_allowed (a : HostSourceAdmission)
    (host : a.trustedHost = true) (owner : a.ownerMatches = true)
    (fresh : a.fresh = true) (write : a.writable = true)
    (node : a.configuredNode = true) (bound : a.sameTransaction = true)
    (unchanged : a.edgeChanged = false) : hostSourceAllowed a = true := by
  simp [hostSourceAllowed, host, owner, fresh, write, node, bound, unchanged]

/-- Replaying an existing root also checks its desired descendants. A missing
descendant is a real source change and uses the actual operator's authority. -/
def hostReplayAllowed (descendantPresent actorAuthorized : Bool) : Bool :=
  descendantPresent || actorAuthorized

/-- A complete projection can be replayed by a non-admin without a new grant. -/
theorem host_replay_complete_allowed (authorized : Bool) :
    hostReplayAllowed true authorized = true := by
  simp [hostReplayAllowed]

/-- A missing descendant cannot be created by an operator without authority for it. -/
theorem host_replay_missing_denied :
    hostReplayAllowed false false = false := by
  simp [hostReplayAllowed]

/-- A request's source change is authorized by its actual actor. Startup,
replication and callbacks with no authenticated actor are not repairs. An
explicit repair is a separate trusted host operation. The transaction owns
both the document and every required source edge. -/
inductive ProjectionOrigin where
  | request | repair | absent
  deriving BEq, DecidableEq

structure ProjectionAdmission where
  origin : ProjectionOrigin
  actorAuthorized : Bool
  repairAuthorized : Bool
  sameTransaction : Bool

def projectionSourceAllowed (a : ProjectionAdmission) : Bool :=
  a.sameTransaction &&
    match a.origin with
    | .request => a.actorAuthorized
    | .repair => a.repairAuthorized
    | .absent => false

/-- An absent actor can never be replaced by the node. -/
theorem absent_origin_denied (a : ProjectionAdmission)
    (h : a.origin = .absent) : projectionSourceAllowed a = false := by
  simp [projectionSourceAllowed, h]

/-- The writer needs native authority for the edge (`NodeAcpDelegation.authorized`). -/
theorem unauthorized_request_denied (a : ProjectionAdmission)
    (h : a.origin = .request) (denied : a.actorAuthorized = false) :
    projectionSourceAllowed a = false := by
  simp [projectionSourceAllowed, h, denied]

/-- Every source change shares the document transaction. -/
theorem unbound_projection_denied (a : ProjectionAdmission)
    (h : a.sameTransaction = false) : projectionSourceAllowed a = false := by
  simp [projectionSourceAllowed, h]

/-- An unchanged source needs no source-administration operation. -/
def projectionStatementAllowed (sourceChanged : Bool) (a : ProjectionAdmission) : Bool :=
  !sourceChanged || projectionSourceAllowed a

theorem unchanged_source_allowed (a : ProjectionAdmission) :
    projectionStatementAllowed false a = true := by
  simp [projectionStatementAllowed]

end HostMutationProjection
