import Lean

/-!
A user approves one update against a trusted schema, document and head
snapshot. The client constructs all block bytes from that intent and releases
signatures only for the complete expected wave. The committing transaction validates its relevant field dependencies and
authorization. A concurrent disjoint write can merge beside the approved graph. Encodings,
cryptography, source authenticity and the native transactional read set are
external premises. Ordinary block signatures can be replayed elsewhere.
Rust owners: crates/db/src/block/builder/client_update.rs,
client_update_snapshot.rs and client_waves.rs; crates/http/src/client_signed_update.rs;
the transactional update path and its ACP checks.
-/

namespace ClientSignedUpdate
open Lean

-- The client's locally approved target and complete wave.
structure Intent where
  collection : String
  document : String
  version : String
  heads : List String
  changes : List String
  blocks : List String
  deriving BEq, FromJson, ToJson

-- A wave is signed only for this target and these exact bytes.
def signs (intent offered : Intent) : Bool :=
  intent.collection == offered.collection && intent.document == offered.document &&
  intent.version == offered.version && intent.heads == offered.heads &&
  intent.changes == offered.changes && offered.blocks.isPerm intent.blocks

-- Signing a wave requires the approved target.
theorem signed_document (intent offered : Intent)
    (accepted : signs intent offered = true) : intent.document = offered.document := by
  simp [signs] at accepted
  exact accepted.1.1.1.1.2

-- Signing a wave requires the exact approved field changes.
theorem signed_changes (intent offered : Intent)
    (accepted : signs intent offered = true) : intent.changes = offered.changes := by
  simp [signs] at accepted
  exact accepted.1.2

-- Signing a wave requires every approved block and no other block.
theorem signed_exact_wave (intent offered : Intent)
    (accepted : signs intent offered = true) : offered.blocks.Perm intent.blocks := by
  simp [signs] at accepted
  exact List.isPerm_iff.mp accepted.2

-- The snapshot provides the materialized value, if the field has one.
structure Change where
  name : String
  before : Option String
  after : String
  deriving BEq, FromJson, ToJson

-- An unchanged field has no new LWW block.
def writes (change : Change) : Bool :=
  change.before != some change.after

-- A value already present in the snapshot is not rewritten.
theorem unchanged_skipped (change : Change)
    (unchanged : change.before = some change.after) : writes change = false := by
  simp [writes, unchanged]

-- The graph contains exactly the changes that still need writes.
def writtenFields (changes : List Change) : List String :=
  (changes.filter writes).map Change.name

-- The test supplies the observations to the executable owner.
structure Input where
  intent : Intent
  offered : Intent
  changes : List Change
  deriving FromJson, ToJson

-- Emit the signing decision for each real input.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("signs", toJson (signs input.intent input.offered)),
     ("writtenFields", toJson (writtenFields input.changes))]

end ClientSignedUpdate

-- Conformance callers supply cases on standard input.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientSignedUpdate.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
