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

/-- One native gateway observation supports delegated reads and named access.
The gateway authenticates the actor separately and never returns document metadata.
The transaction owner and a local authorization view are required in both modes.
Delegated reads additionally require the transaction principal's node authority
and use the actor's application and document read gates. Named observations
instead require live registration and an active actor node permission; they do
not silently assume that a named capability implies read access. -/
inductive AccessMode where
  | delegatedRead
  | named
  deriving DecidableEq, Repr

-- An unavailable result is distinct from an active permission denial.
inductive AccessDecision where
  | allowed
  | denied
  | unavailable
  | invalid
  deriving DecidableEq, Repr

-- Each input is supplied by its existing transaction or authorization owner.
structure AccessEvidence where
  ownsTransaction : Bool
  bound : Bool
  explicitContext : Bool
  principalNodeBound : Bool
  principalNodeRead : Bool
  actorApplicationRead : Bool
  actorDocumentRead : Bool
  liveMember : Bool
  registered : Bool
  namedSupported : Bool
  activeNodePolicy : Bool
  actorNodePermission : Bool
  actorNamedPermission : Bool
  deriving DecidableEq, Repr

-- Select exactly the gates the caller's explicit mode requires.
def accessDecision (mode : AccessMode) (e : AccessEvidence) : AccessDecision :=
  if e.ownsTransaction && e.explicitContext then
    match mode with
    | .delegatedRead =>
      if !e.bound || !e.principalNodeBound || !e.principalNodeRead then .invalid
      else if e.actorApplicationRead && e.actorDocumentRead && e.liveMember then .allowed
      else .unavailable
    | .named =>
      if !e.bound || !e.liveMember || !e.registered || !e.namedSupported || !e.activeNodePolicy then .unavailable
      else if e.actorNodePermission && e.actorNamedPermission then .allowed
      else .denied
  else .invalid

-- A borrowed transaction never yields a decision.
theorem access_requires_transaction_owner (mode : AccessMode) (e : AccessEvidence)
    (h : e.ownsTransaction = false) : accessDecision mode e = .invalid := by
  simp [accessDecision, h]

-- A detached authorization view never certifies an observation.
theorem access_requires_bound_view (mode : AccessMode) (e : AccessEvidence)
    (h : e.bound = false) : accessDecision mode e ≠ .allowed := by
  cases mode <;> simp [accessDecision, h] <;> (try split) <;> (try simp_all)

-- An unsupported named authorization provider cannot be classified as denied.
theorem named_unbound_not_denied (e : AccessEvidence) (h : e.bound = false) :
    accessDecision .named e ≠ .denied := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)

-- Ambient bypass cannot replace the explicit transaction context.
theorem access_requires_explicit_context (mode : AccessMode) (e : AccessEvidence)
    (h : e.explicitContext = false) : accessDecision mode e = .invalid := by
  simp [accessDecision, h]

-- A delegated read cannot certify detached principal node authorization.
theorem delegated_access_requires_bound_principal (e : AccessEvidence)
    (h : e.principalNodeBound = false) :
    accessDecision .delegatedRead e ≠ .allowed := by
  simp [accessDecision, h]

-- A delegated read without the principal node gate is not a visibility result.
theorem delegated_access_requires_principal (e : AccessEvidence)
    (h : e.principalNodeRead = false) :
    accessDecision .delegatedRead e ≠ .allowed := by
  simp [accessDecision, h]

-- Application read denial cannot be made visible by naming a document.
theorem delegated_access_requires_application (e : AccessEvidence)
    (h : e.actorApplicationRead = false) :
    accessDecision .delegatedRead e ≠ .allowed := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- Document read denial cannot be made visible by naming a document.
theorem delegated_access_requires_document (e : AccessEvidence)
    (h : e.actorDocumentRead = false) :
    accessDecision .delegatedRead e ≠ .allowed := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- A missing document cannot be granted or reported as an active denial.
theorem named_missing_not_denied (e : AccessEvidence) (h : e.liveMember = false) :
    accessDecision .named e ≠ .denied := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- A missing registration cannot be reported as an active permission denial.
theorem named_unregistered_not_denied (e : AccessEvidence) (h : e.registered = false) :
    accessDecision .named e ≠ .denied := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- An unsupported named permission cannot be reported as a denial, even when
-- the actor also lacks its node permission.
theorem named_unsupported_not_denied (e : AccessEvidence) (h : e.namedSupported = false) :
    accessDecision .named e ≠ .denied := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)

-- An inactive NAC cannot be reported as an active permission denial.
theorem named_inactive_not_denied (e : AccessEvidence) (h : e.activeNodePolicy = false) :
    accessDecision .named e ≠ .denied := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- A named grant still requires the actor's selected node permission.
theorem named_requires_actor_node_permission (e : AccessEvidence)
    (h : e.actorNodePermission = false) : accessDecision .named e ≠ .allowed := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- A named grant still requires the named document permission.
theorem named_requires_document_permission (e : AccessEvidence)
    (h : e.actorNamedPermission = false) : accessDecision .named e ≠ .allowed := by
  simp [accessDecision, h]
  all_goals (try split) <;> (try simp_all)
  all_goals (try split) <;> (try simp_all)

-- The decision type exports no owner, document fields or hidden identifiers.
example : accessDecision .named ⟨true, true, true, true, false, false, false, true, true, true, true, true, true⟩ = .allowed := by decide
example : accessDecision .delegatedRead ⟨true, true, true, true, true, false, false, true, true, true, true, true, true⟩ = .unavailable := by decide

end Acp.OwnerObservation
