import Lean

/-!
A host-installed projection is trusted node code, not a request parameter. The
native transaction authorizes the document mutation as its actual writer and
the source changes as the node administrator. Both stage in one transaction:
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

end HostMutationProjection
