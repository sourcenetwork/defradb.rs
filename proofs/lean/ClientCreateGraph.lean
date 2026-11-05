import Lean

/-!
An unencrypted create starts from a trusted schema and the client's complete
intended field values. The schema marks some fields immutable. DefraDB groups
all present immutable fields into one block and gives every other present
field its own CRDT block; list values remain values of their fields. The
composite must link every signed child the client approved and no other child.

The model checks the logical partition and the exact link list. DAG-CBOR
encoding, CID hashing, CRDT encoding, and cryptographic signatures are separate
external premises. The concrete client must independently reconstruct the
native blocks using the trusted schema; a remote collection version alone is
not a trusted schema. Empty and null arrays follow the native document encoder.
Rust owners: crates/db/src/block/builder/immutable.rs and compute.rs; proposed
client create-graph validator. No client validator is implemented yet.
-/

namespace ClientCreateGraph
open Lean

-- A field carries its name, complete JSON value, and trusted immutable flag.
structure Field where
  name : String
  value : Json
  immutable : Bool
  deriving BEq, FromJson, ToJson

-- Partition the intended fields into the single immutable block and other blocks.
def immutableFields (fields : List Field) : List Field :=
  fields.filter fun field => field.immutable

-- Every remaining field, including a list, gets its own field block.
def mutableFields (fields : List Field) : List Field :=
  fields.filter fun field => !field.immutable

-- An immutable intended field appears in the immutable block.
theorem immutable_included (fields : List Field) (field : Field)
    (present : field ∈ fields) (immutable : field.immutable = true) :
    field ∈ immutableFields fields := by
  simp [immutableFields, present, immutable]

-- A mutable intended field appears among the separate field blocks.
theorem mutable_included (fields : List Field) (field : Field)
    (present : field ∈ fields) (mutable : field.immutable = false) :
    field ∈ mutableFields fields := by
  simp [mutableFields, present, mutable]

-- No field outside the approved input appears in either group.
theorem immutable_has_no_extra (fields : List Field) (field : Field)
    (present : field ∈ immutableFields fields) : field ∈ fields := by
  exact (List.mem_filter.mp present).1

-- No field outside the approved input appears in a mutable block.
theorem mutable_has_no_extra (fields : List Field) (field : Field)
    (present : field ∈ mutableFields fields) : field ∈ fields := by
  exact (List.mem_filter.mp present).1

-- The independently computed signed child CIDs are the entire expected graph.
def acceptsComposite (expected : List String) (offered : List String) : Bool :=
  expected == offered

-- An accepted composite contains precisely the independently computed links.
theorem accepted_exact_links (expected offered : List String)
    (accepted : acceptsComposite expected offered = true) : expected = offered := by
  simpa [acceptsComposite] using accepted

-- Actual inputs are supplied by the conformance caller, including list values.
structure Input where
  fields : List Field
  expectedLinks : List String
  offeredLinks : List String
  deriving FromJson, ToJson

-- Native tests derive expectations from these outputs, never a second policy.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input),
     ("immutableFields", toJson (immutableFields input.fields)),
     ("mutableFields", toJson (mutableFields input.fields)),
     ("acceptsComposite", toJson (acceptsComposite input.expectedLinks input.offeredLinks))]

end ClientCreateGraph

-- The conformance caller sends its concrete inputs as JSON on standard input.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= ClientCreateGraph.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
