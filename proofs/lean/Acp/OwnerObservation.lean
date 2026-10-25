import Std

/-! A metadata reader may return an owner only after the normal read owners
approve the exact document and the authorization provider is bound to the row
transaction. An owner read from a separate live provider cannot certify the
transaction's snapshot. This models `QueryRunner::observe_owners`, reached through
`QueryExecutor::observe_document_owners_in_txn`.
-/
namespace Acp.OwnerObservation

-- Each fact comes from its existing authorization or transaction owner.
structure Evidence where
  sameActor : Bool -- The caller owns the transaction handle.
  nodeAllowed : Bool -- Node policy permits document reads.
  appAllowed : Bool -- The application permits this collection and document.
  documentAllowed : Bool -- The normal document read owner permits this actor.
  liveMember : Bool -- The exact document is live in the requested collection.
  registered : Bool -- An authenticated ownership registration exists.
  bound : Bool -- Authorization uses the same transaction view as the row.
  deriving DecidableEq, Repr

-- All gates must hold; an advertised capability alone is not a binding.
def eligible (e : Evidence) : Bool :=
  e.sameActor && e.nodeAllowed && e.appAllowed && e.documentAllowed &&
    e.liveMember && e.registered && e.bound

-- Only the owner from that transaction can enter the observable result.
def observe (e : Evidence) (snapshotOwner : Nat) : Option Nat :=
  if eligible e then some snapshotOwner else none

-- Borrowing another actor's transaction cannot reveal ownership.
theorem foreign_actor_hidden (e : Evidence) (owner : Nat) (h : e.sameActor = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- Node read denial remains a denial on this metadata path.
theorem node_denied_hidden (e : Evidence) (owner : Nat) (h : e.nodeAllowed = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- Application read restrictions cannot be bypassed by a known ID.
theorem app_denied_hidden (e : Evidence) (owner : Nat) (h : e.appAllowed = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- Document read restrictions also protect its owner.
theorem document_denied_hidden (e : Evidence) (owner : Nat) (h : e.documentAllowed = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- Deleted, missing and cross-collection IDs establish no live membership.
theorem nonmember_hidden (e : Evidence) (owner : Nat) (h : e.liveMember = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- An unregistered document has no authenticated owner observation.
theorem unregistered_hidden (e : Evidence) (owner : Nat) (h : e.registered = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- A detached provider cannot certify even an otherwise readable row.
theorem unbound_hidden (e : Evidence) (owner : Nat) (h : e.bound = false) :
    observe e owner = none := by simp [observe, eligible, h]

-- Every returned owner is exactly the supplied transaction observation.
theorem exact_snapshot (e : Evidence) (snapshotOwner returned : Nat)
    (h : observe e snapshotOwner = some returned) : returned = snapshotOwner := by
  simp only [observe] at h
  split at h
  · exact (Option.some.inj h).symm
  · contradiction

end Acp.OwnerObservation
