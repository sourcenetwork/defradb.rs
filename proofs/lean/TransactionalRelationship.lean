import Lean

/-! A relationship mutation belongs to its authenticated, writable transaction.
The transaction's ACP provider takes the change in one of two ways: it shares
the transaction's store and applies the change there (bound), or it is a
remote provider that cannot join the transaction and stages the change in it
as owed, to be sent after the commit (owed staging). A provider that does
neither is refused. Either way the native NAC and DAC owners still decide
authority, the request must still be fresh and the server able to stage it.
The caller owns commit or discard: a discarded transaction takes an owed
change with it. -/
namespace TransactionalRelationship
open Lean

/-- The facts the transaction-bound relationship owner must check. -/
structure Input where
  sameOwner : Bool
  writable : Bool
  nodeAllowed : Bool
  documentAllowed : Bool
  boundProvider : Bool
  stagesOwed : Bool
  authorizationFresh : Bool
  transactionBackend : Bool
  deriving FromJson, ToJson

/-- The provider takes an in-transaction change: it applies it in the
transaction, or it stages it there as owed. -/
def providerTakes (i : Input) : Bool := i.boundProvider || i.stagesOwed

/-- A mutation may be staged only when every required fact holds. -/
def mayStage (i : Input) : Bool :=
  i.sameOwner && i.writable && i.nodeAllowed && i.documentAllowed && providerTakes i &&
    i.authorizationFresh && i.transactionBackend

/-- A transaction owned by another actor cannot be changed. -/
theorem wrong_owner_refused (i : Input) (h : i.sameOwner = false) :
    mayStage i = false := by simp [mayStage, h]

/-- A read-only transaction cannot stage a relationship write. -/
theorem readonly_refused (i : Input) (h : i.writable = false) :
    mayStage i = false := by simp [mayStage, h]

/-- NAC denial cannot be bypassed by the transaction API. -/
theorem node_denied (i : Input) (h : i.nodeAllowed = false) :
    mayStage i = false := by simp [mayStage, h]

/-- DAC denial cannot be bypassed by the transaction API. -/
theorem document_denied (i : Input) (h : i.documentAllowed = false) :
    mayStage i = false := by simp [mayStage, h]

/-- A provider that neither applies the change in the transaction nor stages
it there as owed is refused. -/
theorem neither_bound_nor_staging_refused (i : Input)
    (hb : i.boundProvider = false) (hs : i.stagesOwed = false) :
    mayStage i = false := by simp [mayStage, providerTakes, hb, hs]

/-- A provider that stages the change as owed is accepted, though it is not
bound, when every other fact holds. -/
theorem owed_staging_accepted (i : Input) (hs : i.stagesOwed = true)
    (ho : i.sameOwner = true) (hw : i.writable = true) (hn : i.nodeAllowed = true)
    (hd : i.documentAllowed = true) (hf : i.authorizationFresh = true)
    (ht : i.transactionBackend = true) :
    mayStage i = true := by simp [mayStage, providerTakes, hs, ho, hw, hn, hd, hf, ht]

/-- A change an owed-staging provider accepts still had every other guard
hold: staging as owed bypasses no owner, writability, NAC, DAC, freshness or
backend check. -/
theorem owed_staging_keeps_guards (i : Input) (h : mayStage i = true) :
    i.sameOwner = true ∧ i.writable = true ∧ i.nodeAllowed = true ∧
      i.documentAllowed = true ∧ i.authorizationFresh = true ∧
      i.transactionBackend = true := by
  simp [mayStage, providerTakes] at h
  exact ⟨h.1.1.1.1.1.1, h.1.1.1.1.1.2, h.1.1.1.1.2, h.1.1.1.2, h.1.2, h.2⟩

/-- A request that expired before its transaction turn cannot stage effects. -/
theorem expired_refused (i : Input) (h : i.authorizationFresh = false) :
    mayStage i = false := by simp [mayStage, h]

/-- A server that cannot stage relationships must refuse the transaction. -/
theorem unsupported_backend_refused (i : Input) (h : i.transactionBackend = false) :
    mayStage i = false := by simp [mayStage, h]

/-- Only a visible document owned by the authenticated caller answers true. -/
def isCurrentOwner (visible sameOwner : Bool) : Bool := visible && sameOwner

/-- An unreadable or missing document never establishes ownership. -/
theorem hidden_is_not_owner (sameOwner : Bool) :
    isCurrentOwner false sameOwner = false := by rfl

/-- Someone else's visible document never establishes caller ownership. -/
theorem foreign_is_not_owner (visible : Bool) :
    isCurrentOwner visible false = false := by simp [isCurrentOwner]

/-- The embedder-only ownership mode is safe only when HTTP is not served. -/
def compatibleServer (http embedder : Bool) : Bool := !http || !embedder

/-- A shared HTTP server cannot use embedder-only transaction ownership. -/
theorem http_rejects_embedder : compatibleServer true true = false := by rfl

/-- An embedded node without HTTP may keep its embedder ownership mode. -/
theorem local_embedder_allowed : compatibleServer false true = true := by rfl

/-- The model emits its decisions for native conformance tests. -/
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("mayStage", toJson (mayStage i))]
end TransactionalRelationship

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= TransactionalRelationship.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
