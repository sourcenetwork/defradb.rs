import Lean

/-! A relationship mutation belongs to its authenticated, writable transaction.
The transaction's ACP provider must share its store; the native NAC and DAC
owners still decide authority. The caller owns commit or discard. -/
namespace TransactionalRelationship
open Lean

/-- The facts the transaction-bound relationship owner must check. -/
structure Input where
  sameOwner : Bool
  writable : Bool
  nodeAllowed : Bool
  documentAllowed : Bool
  boundProvider : Bool
  authorizationFresh : Bool
  transactionBackend : Bool
  deriving FromJson, ToJson

/-- A mutation may be staged only when every required fact holds. -/
def mayStage (i : Input) : Bool :=
  i.sameOwner && i.writable && i.nodeAllowed && i.documentAllowed && i.boundProvider &&
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

/-- A provider outside this transaction cannot produce an atomic grant. -/
theorem unbound_refused (i : Input) (h : i.boundProvider = false) :
    mayStage i = false := by simp [mayStage, h]

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
