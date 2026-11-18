import Lean

/-! A trusted embedder owns the transaction and authenticates the selected
actor. The transaction owner admits the request, while NAC and DAC judge the
selected actor. HTTP does not expose this delegated operation. -/
namespace DelegatedRelationship
open Lean

/-- Facts used by the embedded relationship boundary. -/
structure Input where
  trustedEmbedder : Bool
  ownsTransaction : Bool
  authenticatedActor : Bool
  actorNac : Bool
  actorDac : Bool
  boundProvider : Bool
  deriving FromJson, ToJson

/-- All facts must hold before the actor's relationship write is staged. -/
def mayStage (i : Input) : Bool :=
  i.trustedEmbedder && i.ownsTransaction && i.authenticatedActor &&
    i.actorNac && i.actorDac && i.boundProvider

/-- An untrusted request cannot use the embedded delegation boundary. -/
theorem untrusted_refused (i : Input) (h : i.trustedEmbedder = false) :
    mayStage i = false := by simp [mayStage, h]

/-- A transaction not owned by the embedder cannot be used. -/
theorem foreign_transaction_refused (i : Input) (h : i.ownsTransaction = false) :
    mayStage i = false := by simp [mayStage, h]

/-- A selected actor must be authenticated by the embedder. -/
theorem unauthenticated_refused (i : Input) (h : i.authenticatedActor = false) :
    mayStage i = false := by simp [mayStage, h]

/-- The transaction owner's NAC permission cannot replace the actor's. -/
theorem actor_nac_required (i : Input) (h : i.actorNac = false) :
    mayStage i = false := by simp [mayStage, h]

/-- The transaction owner's DAC permission cannot replace the actor's. -/
theorem actor_dac_required (i : Input) (h : i.actorDac = false) :
    mayStage i = false := by simp [mayStage, h]

/-- Authorization must bind to the same storage transaction. -/
theorem unbound_refused (i : Input) (h : i.boundProvider = false) :
    mayStage i = false := by simp [mayStage, h]

/-- Emit executable decisions for the native conformance test. -/
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun i => Json.mkObj
    [("input", toJson i), ("mayStage", toJson (mayStage i))]
end DelegatedRelationship

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= DelegatedRelationship.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
