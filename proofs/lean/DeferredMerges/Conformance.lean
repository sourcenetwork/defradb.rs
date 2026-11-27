import DeferredMerges.Park
import Lean

/-! Runs the index model on operation traces, for
`crates/database/db/src/merge/governance/deferred_conformance_tests.rs` to run the
same traces through the Rust `DeferredMerges` and compare.

The story: the Rust test writes one line of JSON, a list of traces. Each
trace names a capacity and a list of operations. This program applies them
to the empty index with the production bounds (that capacity in place of
the production one), and after each operation reports what the Rust can
observe: the operation's own result, how many composites are indexed
(`len`) and whether anything waits (`has_waiters`). It also reports the
production bounds, which the test checks against the Rust constants.
Run by `lake env lean --run DeferredMerges/Conformance.lean`. -/

open Lean
namespace DeferredMerges.Conformance

/-- A key as the test writes it: a kind and a number. -/
structure KeyIn where
  /-- "composite", "field" or "acp". -/
  kind : String
  /-- The CID, value or document number. -/
  n : Nat
  deriving FromJson

/-- The key a written key names. -/
def KeyIn.toKey (k : KeyIn) : Except String Key :=
  -- By its kind.
  match k.kind with
  | "composite" => pure (.composite k.n)
  | "field" => pure (.field k.n)
  | "acp" => pure (.acpObject k.n)
  | other => throw s!"unknown key kind {other}"

/-- One operation as the test writes it. -/
structure OpIn where
  /-- Which operation. -/
  op : String
  /-- The composite, when it names one. -/
  cid : Nat := 0
  /-- The keys, for a defer or a release. -/
  keys : List KeyIn := []
  /-- The document, for a park or a fill. -/
  doc : Nat := 0
  /-- The refused composite, for a park. -/
  refused : Nat := 0
  /-- How many documents the sweep may release, or how many to fill. -/
  n : Nat := 0
  deriving FromJson

/-- One trace: a capacity and its operations. -/
structure TraceIn where
  /-- The capacity the index runs with. -/
  capacity : Nat
  /-- The operations, in order. -/
  ops : List OpIn
  deriving FromJson

/-- Take every handed-back composite, in order (`take_ready` until empty). -/
def drain (s : State) : List Cid × State := Id.run do
  -- Take one at a time, at most as many as are queued.
  let mut s := s
  let mut out := []
  for _ in [0:s.ready.length] do
    -- The next one, if any.
    match takeReady s with
    | some (c, t) => out := out ++ [c]; s := t
    | none => pure ()
  -- What was taken, and the index after.
  return (out, s)

/-- Apply one written operation; return the new index and its result. -/
def apply (L : Limits) (s : State) (o : OpIn) : Except String (State × Json) := do
  -- By the operation's name.
  match o.op with
  | "defer" =>
    -- Governance defers under the keys given.
    let ks ← o.keys.mapM KeyIn.toKey
    pure (defer L o.cid ks s, Json.null)
  | "park" =>
    -- The gate parks under a document and a refused composite.
    pure (park L o.cid o.doc o.refused s, Json.null)
  | "fill" =>
    -- Park `n` composites from `cid` on, each refused itself, under one document.
    pure ((List.range o.n).foldl (fun s i => park L (o.cid + i) o.doc (o.cid + i) s) s, Json.null)
  | "release" =>
    -- These keys arrived.
    let ks ← o.keys.mapM KeyIn.toKey
    pure (release s ks, Json.null)
  | "releaseParked" =>
    -- The sweep; its result is how many documents it released.
    let r := releaseParked o.n s
    pure (r.2, toJson r.1)
  | "enqueue" =>
    -- Direct queueing; its result is whether it queued.
    let r := enqueueReady o.cid s
    pure (r.2, toJson r.1)
  | "forget" =>
    -- A merged composite; its result is whether it had an entry.
    let r := forget o.cid s
    pure (r.2, toJson r.1)
  | "drain" =>
    -- Take everything handed back; the result is what, sorted.
    let r := drain s
    pure (r.2, toJson (r.1.mergeSort (· ≤ ·)))
  | other => throw s!"unknown operation {other}"

/-- What the Rust can see of the index after an operation. -/
def observe (s : State) (result : Json) : Json :=
  -- The result, the count, and whether anything waits (`has_waiters_iff`).
  Json.mkObj [("result", result), ("len", toJson s.entries.length),
    ("has_waiters", toJson !s.entries.isEmpty)]

/-- Run one trace from the empty index. -/
def runTrace (t : TraceIn) : Except String (List Json) := do
  -- The production bounds, with the trace's capacity.
  let L : Limits := { production with capacity := t.capacity }
  let mut s := empty
  let mut out := []
  for o in t.ops do
    -- Apply it and record what can be seen.
    let (next, result) ← apply L s o
    s := next
    out := out ++ [observe s result]
  return out

end DeferredMerges.Conformance

/-- Read the traces, run them, print the bounds and every observation. -/
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  let parsed := do
    let json ← Json.parse input
    let traces : List DeferredMerges.Conformance.TraceIn ← fromJson? json
    traces.mapM DeferredMerges.Conformance.runTrace
  match parsed with
  | .ok runs =>
    let p := DeferredMerges.production
    (← IO.getStdout).putStrLn (Json.mkObj [
      ("limits", toJson [p.capacity, p.perKey, p.perComposite]),
      ("runs", toJson runs)]).compress
  | .error message => throw (IO.userError message)
