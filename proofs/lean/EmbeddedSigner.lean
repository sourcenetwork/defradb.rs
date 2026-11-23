import Lean

/-!
The story: an embedded caller sends the node a request with a signing key of
 its own. The node lets that key sign the request's blocks only when the key is
 a local Ed25519 or secp256k1 key, the key is usable, it belongs to the actor the
 request names, and the request does not bypass DAC. What must never happen: a
 key of another type (Secp256r1, BLS), a broken key, someone else's key, or a
 DAC-bypassing request gets to sign. The transaction registry still uses the
 node principal; authorization by the selected actor is a separate check.
Rust owner: `EmbeddedSigner::from_config` (key type, local key, key check) and
 `admitted_signer` (actor match, bypass) in
 `crates/entry-points/defra-node/src/embedded_signer.rs`, which
 `Node::execute_prepared_request` runs for every request carrying a caller's key.
-/
-- Everything below lives in its own namespace.
namespace EmbeddedSigner
-- We use Lean's JSON support for the conformance cases.
open Lean

-- The kinds of key a caller can bring: the two the node signs with, and every
-- other kind (Secp256r1, BLS), which all behave the same.
inductive KeyType where
  -- An Ed25519 key.
  | ed25519
  -- A secp256k1 key.
  | secp256k1
  -- Any other kind of key.
  | other
  -- Read and written as JSON, and comparable.
  deriving FromJson, ToJson, DecidableEq, Repr

-- Whether the node signs with a key of this kind: only Ed25519 and secp256k1.
def KeyType.supported : KeyType → Bool
  -- Ed25519 keys may sign.
  | .ed25519 => true
  -- secp256k1 keys may sign.
  | .secp256k1 => true
  -- No other kind may.
  | .other => false

-- The facts the node checks about one request, established at the native boundary.
structure Input where
  -- The actor the request names, if any.
  actor : Option String
  -- The identity the caller's key belongs to, if any.
  signer : Option String
  -- The kind of the caller's key.
  keyType : KeyType
  -- Whether the key is local and its private and public halves match.
  keyValid : Bool
  -- Whether the request asks to bypass DAC.
  bypass : Bool
  -- Read and written as JSON.
  deriving FromJson, ToJson

-- The node lets the key sign only with no bypass, a supported usable key, and
-- an actor that is the key's own identity.
def admitted (input : Input) : Bool :=
  -- A DAC bypass is always refused.
  if input.bypass then false else
    -- Otherwise both the actor and the key's identity must be present...
    match input.actor, input.signer with
    -- ...the key supported and usable, and the two identities the same.
    | some actor, some signer => input.keyType.supported && input.keyValid && actor == signer
    -- A missing actor or key identity is refused.
    | _, _ => false

-- Promise: a key that is let in names the request's actor, is a supported
-- usable key, and the request does not bypass DAC.
theorem admitted_binds (input : Input) (h : admitted input = true) :
    ∃ actor, input.actor = some actor ∧ input.signer = some actor ∧
      input.keyType.supported = true ∧ input.keyValid = true ∧ input.bypass = false := by
  -- Look at each field of the request in turn.
  cases input with
  | mk actor signer keyType valid bypass =>
    -- A bypass is never admitted, so that case contradicts `h`.
    cases bypass with
    | true => simp [admitted] at h
    | false =>
      -- With no actor nothing is admitted either.
      cases actor with
      | none => simp [admitted] at h
      | some a =>
        -- With no key identity nothing is admitted either.
        cases signer with
        | none => simp [admitted] at h
        | some s =>
          -- Otherwise `h` says the key is supported, usable and the identities agree.
          simp [admitted] at h
          rcases h with ⟨⟨hk, hv⟩, hs⟩
          -- The key's identity is the actor.
          subst s
          -- So every part of the promise holds.
          exact ⟨a, rfl, rfl, hk, hv, rfl⟩

-- Promise: a key of an unsupported kind is never let in, whatever else holds.
theorem unsupported_refused (input : Input) (h : input.keyType = .other) :
    admitted input = false := by
  -- Look at each field of the request in turn.
  cases input with
  | mk actor signer keyType valid bypass =>
    -- The key's kind is `other`.
    simp at h
    subst h
    -- Every combination of the remaining fields is then refused.
    cases bypass <;> cases actor <;> cases signer <;> simp [admitted, KeyType.supported]

-- Expected results come from this executable owner, for native conformance tests:
-- each input is answered with whether it is admitted.
def evaluate (json : Json) : Except String Json := do
  -- Read the list of cases.
  let inputs : Array Input ← fromJson? json
  -- Answer each case with its verdict.
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("admitted", toJson (admitted input))]
-- End of the namespace.
end EmbeddedSigner

-- The native conformance test supplies cases through standard input.
def main : IO Unit := do
  -- Read one line of JSON.
  let input ← (← IO.getStdin).getLine
  -- Evaluate it and print the answers, or fail with the parse error.
  match Lean.Json.parse input >>= EmbeddedSigner.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
