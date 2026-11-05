import Lean

/-!
A client wants one flat, unencrypted create to carry its own DefraDB block
signatures. The client independently computes the canonical field blocks for
its approved values and, after signing them, the composite block referencing
those signed fields. The server may ask for signatures in more than one wave.
Only the authenticated owner of the original live operation may answer its
current wave, and every returned signature must verify against that owner's key
and the exact requested bytes. The server resumes only the original operation.

This model treats canonical encoding, key-to-DID derivation, signature
verification, a trusted schema, and the client construction of each expected
wave as external premises. It does not prove them. Defaults, generated fields,
encryption, nested writes, updates, deletes, and filtered or multi-document
mutations are outside this first client-verifiable create subset. DefraDB block
signatures sign block bytes only: a server that receives one can reuse it, and
existing P2P peers do not enforce the HTTP session, expiry, or intent.

Proposed Rust owners: crates/http request-scoped signature continuation,
crates/query prepared-input context, and crates/db block creation and validation.
No Rust implementation is asserted by this model yet.
-/

namespace ClientSignedCreate
open Lean

-- The capsule is established from the authenticated original request.
structure Capsule where
  actor : String
  transaction : String
  operation : String
  session : String
  wave : Nat
  offered : List String
  live : Bool
  deriving FromJson, ToJson

-- The client computes this wave independently from its approved create.
structure ClientIntent where
  expected : List String
  deriving FromJson, ToJson

-- The reply carries the complete current wave and independent verification facts.
structure Reply where
  actor : String
  transaction : String
  operation : String
  session : String
  wave : Nat
  blocks : List String
  keyMatchesActor : Bool
  signaturesValid : Bool
  deriving FromJson, ToJson

-- Accept only the original capsule's complete, currently approved wave.
def accepts (capsule : Capsule) (reply : Reply) : Bool :=
  capsule.live && reply.actor == capsule.actor &&
  reply.transaction == capsule.transaction && reply.operation == capsule.operation &&
  reply.session == capsule.session && reply.wave == capsule.wave &&
  reply.blocks == capsule.offered && reply.keyMatchesActor &&
  reply.signaturesValid

-- Acceptance entails the operation was still live.
theorem accepted_live (capsule : Capsule) (reply : Reply)
    (accepted : accepts capsule reply = true) : capsule.live = true := by
  simp [accepts] at accepted
  exact accepted.1.1.1.1.1.1.1.1

-- Acceptance entails the submitted bytes equal the server's outstanding wave.
theorem accepted_exact_blocks (capsule : Capsule) (reply : Reply)
    (accepted : accepts capsule reply = true) : reply.blocks = capsule.offered := by
  simp [accepts] at accepted
  exact accepted.1.1.2

-- The client signs only a wave it independently reconstructed from its intent.
def clientSigns (intent : ClientIntent) (blocks : List String) : Bool :=
  blocks == intent.expected

-- A client that signs has compared the whole ordered wave to its expectation.
theorem signed_exact_blocks (intent : ClientIntent) (blocks : List String)
    (signed : clientSigns intent blocks = true) : blocks = intent.expected := by
  simpa [clientSigns] using signed

-- Cases carry independent server and client observations.
structure Input where
  capsule : Capsule
  intent : ClientIntent
  reply : Reply
  deriving FromJson, ToJson

-- Expectations are computed by this executable owner from supplied cases.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input),
     ("accepted", toJson (accepts input.capsule input.reply)),
     ("clientSigns", toJson (clientSigns input.intent input.reply.blocks))]

end ClientSignedCreate

-- Conformance tools send their actual inputs to the model on standard input.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSignedCreate.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
