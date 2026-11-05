import Lean

/-! A graph handoff contains only public blocks and signatures. The native
transaction retains its owner, and each signed create is authorized as the
identity derived from its verified signing key. Native signature verification,
transaction isolation and ACP decisions are external premises. -/
namespace VerifiedCreateHandoff
open Lean

structure Graph where
  signer : String
  collection : String
  document : String
  input : String
  blocks : List String
  deriving BEq, FromJson, ToJson

structure Input where
  approved : Graph
  offered : Graph
  claimedActor : String
  verified : Bool
  typedOverride : Bool
  nativeAuthorized : Bool
  waveConsumed : Bool
  commit : Bool
  deriving FromJson, ToJson

def accepts (input : Input) : Bool :=
  input.verified && !input.typedOverride && input.claimedActor == input.approved.signer && input.approved == input.offered

def creates (input : Input) : Bool :=
  accepts input && input.nativeAuthorized && input.waveConsumed && input.commit

theorem signature_evidence_is_not_authority (input : Input) (h : input.nativeAuthorized = false) :
    creates input = false := by simp [creates, h]

theorem wrong_actor_is_rejected (input : Input) (h : input.claimedActor ≠ input.approved.signer) :
    accepts input = false := by simp [accepts, h]

theorem typed_override_rejected (input : Input) (h : input.typedOverride = true) :
    accepts input = false := by simp [accepts, h]

theorem failed_transaction_publishes_nothing (input : Input) (h : input.commit = false) :
    creates input = false := by simp [creates, h]

theorem unconsumed_graph_publishes_nothing (input : Input) (h : input.waveConsumed = false) :
    creates input = false := by simp [creates, h]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("accepts", toJson (accepts input)), ("creates", toJson (creates input))]
end VerifiedCreateHandoff

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= VerifiedCreateHandoff.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
