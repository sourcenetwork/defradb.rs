import Lean

/-!
A protected document's deleted history retains its authorization until that
history is removed, and revocation remains effective after deletion. Reusing
the same identity while old history remains cannot distinguish incarnations.
The recreation fence here is one candidate, not a decision about native purge
semantics. The model does not prove storage, remote ACP, or replication behavior. The
proposed native owners are unregister_deleted_doc in
crates/db/src/write/autocommit/helpers.rs and DirectChecker in
crates/acp/src/read_access.rs.
-/
namespace DeletedHistory
open Lean

-- The owner may grant a reader; the foreign actor has no grant.
inductive Actor where
  | owner | reader | foreign
  deriving DecidableEq, BEq, FromJson, ToJson

-- These flags describe one document identity and its retained history.
structure State where
  live : Bool
  history : Bool
  registered : Bool
  readerGranted : Bool
  deriving DecidableEq, BEq, FromJson, ToJson

-- The current unregistered-object rule makes history public; registered history checks grants.
def read (s : State) (actor : Actor) : Bool :=
  if !s.history then false
  else if !s.registered then true
  else match actor with
    | .owner => true
    | .reader => s.readerGranted
    | .foreign => false

-- Deletion keeps the history and its authorization.
def delete (s : State) : State := { s with live := false }
-- Revocation removes the reader grant.
def revoke (s : State) : State := { s with readerGranted := false }
-- Purging can leave history behind; only retained history keeps authorization.
def purge (s : State) (retainsHistory : Bool) : State :=
  { live := false, history := retainsHistory,
    registered := retainsHistory && s.registered,
    readerGranted := retainsHistory && s.readerGranted }
-- This candidate refuses to reuse an identity while its history remains.
def recreate (s : State) : Option State :=
  if s.history then none
  else some { live := true, history := true, registered := true, readerGranted := false }

-- Deletion preserves each actor’s access to retained history.
theorem delete_keeps_access (s : State) (actor : Actor) :
    read (delete s) actor = read s actor := by simp [delete, read]

-- A protected history is inaccessible to an unrelated actor.
theorem protected_foreign_denied (s : State) (isProtected : s.registered = true) :
    read s .foreign = false := by simp [read, isProtected]

-- Revoking a registered reader denies the reader.
theorem revoked_reader_denied (s : State) (isProtected : s.registered = true) :
    read (revoke s) .reader = false := by simp [read, revoke, isProtected]

-- This candidate fences identity reuse while history remains.
theorem retained_history_cannot_recreate (s : State) (retained : s.history = true) :
    recreate s = none := by simp [recreate, retained]

-- A new generation does not inherit a reader grant.
theorem fresh_generation_has_no_reader (s : State) (fresh : s.history = false) :
    (recreate s).map (fun next => read next .reader) = some false := by
  simp [recreate, fresh, read]

-- The adapter evaluates the supplied action and actor against the same model.
structure Input where
  state : State
  actor : Actor
  action : String
  retainsHistory : Bool
  deriving FromJson, ToJson

-- Return each transition and its resulting read decision.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Input ← fromJson? json
  pure <| toJson <| inputs.map fun input =>
    let after := match input.action with
      | "delete" => some (delete input.state)
      | "revoke" => some (revoke input.state)
      | "purge" => some (purge input.state input.retainsHistory)
      | "recreate" => recreate input.state
      | _ => none
    Json.mkObj [("input", toJson input), ("after", toJson after),
      ("read", toJson (after.map fun state => read state input.actor))]
end DeletedHistory

def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= DeletedHistory.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
