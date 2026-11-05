import Lean

/-!
A delete signs one exact status-2 composite computed from a verified live
history, trusted schema, and caller-held key. Native authorization and transaction
commit decide whether deletion takes effect. Once dispatched, a lost reply
cannot distinguish a committed delete from a rolled-back delete. Cryptographic
verification, physical block encoding, history availability and native ACP are
external premises of this model.
-/
namespace ClientSignedDelete
open Lean

structure Intent where
  collection : String
  document : String
  version : String
  heads : List String
  priority : Nat
  block : String
  signer : String
  deriving BEq, DecidableEq, FromJson, ToJson

structure Input where
  approved : Intent
  offered : Intent
  authenticated : String
  snapshotValid : Bool
  authorized : Bool
  committed : Bool
  responseReceived : Bool
  deriving FromJson, ToJson

def signs (input : Input) : Bool :=
  input.snapshotValid && input.authenticated == input.approved.signer &&
    decide (input.approved = input.offered)

def outcome (input : Input) : String :=
  if !signs input || !input.authorized then "unchanged"
  else if !input.responseReceived then "unknown"
  else if input.committed then "deleted" else "unchanged"

theorem signed_exact_delete (input : Input) (h : signs input = true) :
    input.approved = input.offered := by
  simp [signs] at h
  exact h.2

theorem mismatched_caller_cannot_sign (input : Input)
    (h : input.authenticated ≠ input.approved.signer) : signs input = false := by
  simp [signs, h]

theorem denied_delete_unchanged (input : Input) (h : input.authorized = false) :
    outcome input = "unchanged" := by simp [outcome, h]

theorem lost_response_is_unknown (input : Input) (signed : signs input = true)
    (authorized : input.authorized = true) (lost : input.responseReceived = false) :
    outcome input = "unknown" := by simp [outcome, signed, authorized, lost]

def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("signs", toJson (signs input)), ("outcome", toJson (outcome input))]
end ClientSignedDelete

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSignedDelete.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
