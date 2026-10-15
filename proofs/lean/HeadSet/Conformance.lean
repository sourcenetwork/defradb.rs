import HeadSet.Cache
import Lean

open Lean
namespace HeadSet.Conformance

structure Action where
  op : String
  txn : Nat
  block : Nat
  parents : List Nat
  deriving FromJson

inductive Mutation where
  | append (block : Block)
  | deleteHead (block : BlockId)
  | deleteMarker (parent child : BlockId)

def mutate (s : Store) : Mutation → Store
  | .append block => applyDerived s block
  | .deleteHead b => { s with headKeys := s.headKeys.filter (· != b) }
  | .deleteMarker p c => { s with supersedes := s.supersedes.filter (· != (p, c)) }

structure Txn where
  snapshot : Store
  pending : List Mutation := []

def visible (t : Txn) : Store := t.pending.foldl mutate t.snapshot

structure State where
  durable : Store := ⟨[], []⟩
  txns : List (Nat × Txn) := []

def putTxn (s : State) (id : Nat) (t : Txn) : State :=
  { s with txns := (id, t) :: s.txns.filter (fun row => row.1 != id) }

def result (s : Store) : Json :=
  let heads := (derivedHeads s).eraseDups.mergeSort (· ≤ ·)
  Json.mkObj [
    ("heads", toJson heads),
    ("superseded", toJson (s.headKeys.eraseDups.filter (isSuperseded s)).length),
    ("max_priority", toJson (heads.foldl (fun n b => max n (b + 1)) 0))]

def step (s : State) (a : Action) : Except String (State × Option Json) := do
  if a.op == "begin" then
    return (putTxn s a.txn ⟨s.durable, []⟩, none)
  if a.op == "evict" || a.op == "reopen" then return (s, none)
  let some row := s.txns.find? (fun row => row.1 == a.txn)
    | throw s!"unknown transaction {a.txn}"
  let t := row.2
  if a.op == "append" then
    return (putTxn s a.txn { t with pending := t.pending ++ [.append ⟨a.block, a.parents⟩] }, none)
  if a.op == "prune" then
    let view := visible t
    let doomed := view.headKeys.eraseDups.filter (isSuperseded view)
    let markers := view.supersedes.eraseDups.filter (fun pair => doomed.contains pair.1)
    let delta := doomed.map Mutation.deleteHead ++
      markers.map (fun pair => Mutation.deleteMarker pair.1 pair.2)
    return (putTxn s a.txn { t with pending := t.pending ++ delta }, none)
  if a.op == "read" then return (s, some (result (visible t)))
  if a.op == "commit" then
    return ({ durable := t.pending.foldl mutate s.durable
              txns := s.txns.filter (fun row => row.1 != a.txn) }, none)
  if a.op == "abort" then
    return ({ s with txns := s.txns.filter (fun row => row.1 != a.txn) }, none)
  throw s!"unknown action {a.op}"

def run (actions : List Action) : Except String (List Json) := do
  let mut state : State := {}
  let mut outputs := []
  for action in actions do
    let (next, output) ← step state action
    state := next
    if let some output := output then outputs := output :: outputs
  return outputs.reverse

end HeadSet.Conformance

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  let parsed := do
    let json ← Json.parse input
    let actions : List HeadSet.Conformance.Action ← fromJson? json
    HeadSet.Conformance.run actions
  match parsed with
  | .ok results => (← IO.getStdout).putStrLn (toJson results).compress
  | .error message => throw (IO.userError message)
