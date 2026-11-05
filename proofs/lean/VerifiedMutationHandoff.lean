import Lean

/-! Signature evidence binds a single update or delete to the caller's exact
intent and observed history. Native execution independently checks DAC and
the resulting signing waves against the transaction's current document; a
stale history cannot authorize a different graph. A no-op has no signed wave
and cannot establish the caller's authority for an embedded handoff. -/
namespace VerifiedMutationHandoff
open Lean

structure Graph where
  signer : String
  operation : String
  collection : String
  document : String
  input : String
  history : String
  blocks : List String
  deriving BEq, FromJson, ToJson

structure Input where
  approved : Graph
  offered : Graph
  claimedActor : String
  verified : Bool
  nativeAuthorized : Bool
  authorityBound : Bool
  waveMatches : Bool
  commit : Bool
  deriving FromJson, ToJson

def accepts (input : Input) : Bool :=
  input.verified && !input.approved.blocks.isEmpty &&
    input.claimedActor == input.approved.signer && input.approved == input.offered

def publishes (input : Input) : Bool :=
  accepts input && input.nativeAuthorized && input.authorityBound && input.waveMatches && input.commit

theorem stale_wave_does_not_publish (input : Input) (h : input.waveMatches = false) :
    publishes input = false := by simp [publishes, h]

/-- A successful response is not proof that an update consumed a signing wave:
an unchanged row can be returned without writing any block. -/
theorem unconsumed_wave_does_not_publish (input : Input) (h : input.waveMatches = false) :
    publishes input = false := by simp [publishes, h]

theorem evidence_is_not_authority (input : Input) (h : input.nativeAuthorized = false) :
    publishes input = false := by simp [publishes, h]

theorem unavailable_authority_does_not_publish (input : Input) (h : input.authorityBound = false) :
    publishes input = false := by simp [publishes, h]

theorem no_signature_is_not_authority (input : Input) (h : input.approved.blocks = []) :
    accepts input = false := by simp [accepts, h]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("accepts", toJson (accepts input)), ("publishes", toJson (publishes input))]
end VerifiedMutationHandoff

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= VerifiedMutationHandoff.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
