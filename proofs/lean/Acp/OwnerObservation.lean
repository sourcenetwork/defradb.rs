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

-- Visibility is decided by the ordinary read gates, independently of registration.
def readable (e : Evidence) : Bool :=
  e.sameActor && e.nodeAllowed && e.appAllowed && e.documentAllowed &&
    e.liveMember && e.bound

-- Returning an authenticated owner additionally requires its registration.
def eligible (e : Evidence) : Bool := readable e && e.registered

-- Only the owner from that transaction can enter the observable result.
def observe (e : Evidence) (snapshotOwner : Nat) : Option Nat :=
  if eligible e then some snapshotOwner else none

-- The representation retains read visibility without inventing an owner.
structure Observation where
  visible : Bool -- The exact known document passed the transaction's read gates.
  owner : Option Nat -- Registration independently determines whether ownership is known.
  deriving DecidableEq, Repr

/-- Both facts describe a successful native read. The transaction owner rejects
actor mismatches before returning any observation; this projection does not
represent that error channel. -/
def observation (e : Evidence) (snapshotOwner : Nat) : Observation :=
  ⟨readable e, observe e snapshotOwner⟩

-- Borrowing another actor's transaction cannot reveal ownership.
theorem foreign_actor_hidden (e : Evidence) (owner : Nat) (h : e.sameActor = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- Node read denial remains a denial on this metadata path.
theorem node_denied_hidden (e : Evidence) (owner : Nat) (h : e.nodeAllowed = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- Application read restrictions cannot be bypassed by a known ID.
theorem app_denied_hidden (e : Evidence) (owner : Nat) (h : e.appAllowed = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- Document read restrictions also protect its owner.
theorem document_denied_hidden (e : Evidence) (owner : Nat) (h : e.documentAllowed = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- Deleted, missing and cross-collection IDs establish no live membership.
theorem nonmember_hidden (e : Evidence) (owner : Nat) (h : e.liveMember = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- An unregistered document has no authenticated owner observation.
theorem unregistered_hidden (e : Evidence) (owner : Nat) (h : e.registered = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- A detached provider cannot certify even an otherwise readable row.
theorem unbound_hidden (e : Evidence) (owner : Nat) (h : e.bound = false) :
    observe e owner = none := by simp [observe, eligible, readable, h]

-- A successful request cannot expose a denied read merely because its ID is known.
theorem unreadable_observation_hidden (e : Evidence) (owner : Nat)
    (_sameActor : e.sameActor = true)
    (h : readable e = false) : observation e owner = ⟨false, none⟩ := by
  simp [observation, observe, eligible, h]

-- Readable public data has visibility even when its owner is unknown.
theorem unregistered_read_keeps_visibility (e : Evidence) (owner : Nat)
    (hread : readable e = true) (hregistered : e.registered = false) :
    observation e owner = ⟨true, none⟩ := by
  simp [observation, observe, eligible, hread, hregistered]

-- A known owner can never be returned beside a false visibility observation.
theorem owner_implies_visibility (e : Evidence) (owner returned : Nat)
    (h : (observation e owner).owner = some returned) :
    (observation e owner).visible = true := by
  by_cases hr : readable e = true
  · simpa [observation] using hr
  · have hf : readable e = false := Bool.eq_false_iff.mpr hr
    simp [observation, observe, eligible, hf] at h

-- Every returned owner is exactly the supplied transaction observation.
theorem exact_snapshot (e : Evidence) (snapshotOwner returned : Nat)
    (h : observe e snapshotOwner = some returned) : returned = snapshotOwner := by
  simp only [observe] at h
  split at h
  · exact (Option.some.inj h).symm
  · contradiction

/-- A native gateway authenticates the delegated actor separately from the
transaction principal. Node read authority belongs to the transaction principal;
document and application authority belong to the delegated actor. The result is
valid only in that transaction and never exports owner metadata. -/
structure DelegatedEvidence where
  ownsTransaction : Bool
  principalNodeAllowed : Bool
  actorAppAllowed : Bool
  actorDocumentAllowed : Bool
  liveMember : Bool
  bound : Bool
  explicitContext : Bool
  deriving DecidableEq, Repr

inductive DelegatedResult where
  | unavailable
  | visible (allowed : Bool)
  deriving DecidableEq, Repr

def delegated (e : DelegatedEvidence) : DelegatedResult :=
  if e.ownsTransaction && e.principalNodeAllowed && e.bound && e.explicitContext then
    .visible (e.actorAppAllowed && e.actorDocumentAllowed && e.liveMember)
  else .unavailable

theorem delegated_requires_node_authority (e : DelegatedEvidence)
    (h : e.principalNodeAllowed = false) : delegated e = .unavailable := by
  simp [delegated, h]

theorem delegated_requires_own_transaction (e : DelegatedEvidence)
    (h : e.ownsTransaction = false) : delegated e = .unavailable := by
  simp [delegated, h]

theorem delegated_document_denial (e : DelegatedEvidence)
    (h : e.actorDocumentAllowed = false) : delegated e ≠ .visible true := by
  unfold delegated
  split <;> simp_all

theorem delegated_requires_bound_explicit_context (e : DelegatedEvidence)
    (h : e.bound = false ∨ e.explicitContext = false) : delegated e = .unavailable := by
  rcases h with h | h <;> simp [delegated, h]

example : delegated ⟨true, true, true, true, true, true, true⟩ = .visible true := by decide
example : delegated ⟨true, true, true, false, true, true, true⟩ = .visible false := by decide
example : delegated ⟨true, false, true, true, true, true, true⟩ = .unavailable := by decide

end Acp.OwnerObservation
