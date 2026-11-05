import Lean

/-!
An explicit embedded signer is admitted only when its verified local key belongs
 to the selected actor. The transaction registry still uses the node principal;
 authorization by the selected actor remains an independent requirement.
Rust owner: `crates/defra-node/src/embedded_signer.rs` and `Node::execute_request_with_signer`.
-/
namespace EmbeddedSigner
open Lean

-- The inputs are facts established at the native boundary, not user identity policy.
structure Input where
  actor : Option String
  signer : Option String
  keyValid : Bool
  bypass : Bool
  deriving FromJson, ToJson

-- A missing actor, invalid or different key, or requested DAC bypass refuses dispatch.
def admitted (input : Input) : Bool :=
  if input.bypass then false else
    match input.actor, input.signer with
    | some actor, some signer => input.keyValid && actor == signer
    | _, _ => false

-- A dispatched write has a selected actor and an identical verified signer.
theorem admitted_binds (input : Input) (h : admitted input = true) :
    ∃ actor, input.actor = some actor ∧ input.signer = some actor ∧
      input.keyValid = true ∧ input.bypass = false := by
  cases input with
  | mk actor signer valid bypass =>
    cases bypass with
    | true => simp [admitted] at h
    | false =>
      cases actor with
      | none => simp [admitted] at h
      | some a =>
        cases signer with
        | none => simp [admitted] at h
        | some s =>
          simp [admitted] at h
          rcases h with ⟨hv, hs⟩
          subst s
          exact ⟨a, rfl, rfl, hv, rfl⟩

-- Expected results come from this executable owner, for native conformance tests.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("admitted", toJson (admitted input))]
end EmbeddedSigner

-- The native conformance test supplies cases through standard input.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= EmbeddedSigner.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
