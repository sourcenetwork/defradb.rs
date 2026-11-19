import Lean

/-! A trusted embedder owns the transaction and authenticates the selected
actor. The transaction owner admits the request, while NAC and DAC judge the
selected actor. HTTP does not expose this delegated operation. The provider
takes the change in the transaction (bound) or, for a remote provider that
cannot join it, stages it there as owed; a provider that does neither is
refused. -/
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
  stagesOwed : Bool
  deriving FromJson, ToJson

/-- The provider takes an in-transaction change: it applies it in the
transaction, or it stages it there as owed. -/
def providerTakes (i : Input) : Bool := i.boundProvider || i.stagesOwed

/-- All facts must hold before the actor's relationship write is staged. -/
def mayStage (i : Input) : Bool :=
  i.trustedEmbedder && i.ownsTransaction && i.authenticatedActor &&
    i.actorNac && i.actorDac && providerTakes i

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

/-- A provider that neither applies the change in the transaction nor stages
it there as owed is refused. -/
theorem neither_bound_nor_staging_refused (i : Input)
    (hb : i.boundProvider = false) (hs : i.stagesOwed = false) :
    mayStage i = false := by simp [mayStage, providerTakes, hb, hs]

/-- A provider that stages the change as owed is accepted, though it is not
bound, when every other fact holds. -/
theorem owed_staging_accepted (i : Input) (hs : i.stagesOwed = true)
    (he : i.trustedEmbedder = true) (ho : i.ownsTransaction = true)
    (ha : i.authenticatedActor = true) (hn : i.actorNac = true)
    (hd : i.actorDac = true) :
    mayStage i = true := by simp [mayStage, providerTakes, hs, he, ho, ha, hn, hd]

/-- A change an owed-staging provider accepts still had every other guard
hold: staging as owed bypasses no embedder, ownership, actor, NAC or DAC
check. -/
theorem owed_staging_keeps_guards (i : Input) (h : mayStage i = true) :
    i.trustedEmbedder = true ∧ i.ownsTransaction = true ∧
      i.authenticatedActor = true ∧ i.actorNac = true ∧ i.actorDac = true := by
  simp [mayStage, providerTakes] at h
  exact ⟨h.1.1.1.1.1, h.1.1.1.1.2, h.1.1.1.2, h.1.1.2, h.1.2⟩

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
