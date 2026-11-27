/-!
The story, in plain words. A collection may owe a reindex: a record with a
token and, once a rebuild stopped on a unique clash, the failure it met (the
index and the reason). The index listing shows that failure as a failed
build, and writers stop maintaining that index, as Go abandons a failed
index. Five operations touch the record: owing a reindex writes a new record
without a failure; a stopped rebuild writes its failure onto the record only
if the record still has the rebuild's token; a rebuild that tries the
reindex again removes the failure on its first page; a finished rebuild
removes the record if it still has its token; the listing reads the failure
off the record.

What must never happen: the listing shows a failure the current record did
not get from a rebuild of its own token, or shows one when nothing is owed;
writers maintain the failed index, or stop maintaining any other one.

Rust owner: crates/database/db/src/definition/migration/reindex/owed.rs,
owe_reindex / owe_full_reindex (failure: None), record_failure (owed_under
the token), retry_failed (removes the failure), clear_owed_reindex (removes
the record), owed_status (which index writers skip, through
crates/database/db/src/collection/index_status.rs IndexStatuses::takes_writes),
owed_failure (what the listing shows, through
crates/database/db/src/database/action.rs index_listing_executions).
-/
namespace ReindexFailureListing

-- The failure a stopped rebuild met: the unique index and the reason text.
structure Failure where
  index : Nat
  reason : String
  deriving DecidableEq

-- The owed record: its token and, maybe, a failure.
structure Owed where
  token : Nat
  failure : Option Failure
  deriving DecidableEq

-- Owing a reindex under `token` writes a record without a failure, whatever
-- was owed before.
def owe (_before : Option Owed) (token : Nat) : Option Owed :=
  some { token := token, failure := none }

-- A rebuild under `token` that stopped with `f` writes it onto the record
-- only if the record still has that token; otherwise it changes nothing.
def recordFailure (state : Option Owed) (token : Nat) (f : Failure) : Option Owed :=
  match state with
  | some o => if o.token = token then some { o with failure := some f } else some o
  | none => none

-- A rebuild under `token` that finished removes the record if it still has
-- that token.
def clear (state : Option Owed) (token : Nat) : Option Owed :=
  match state with
  | some o => if o.token = token then none else some o
  | none => none

-- A rebuild under `token` that tries the reindex again removes the failure
-- if the record still has that token; otherwise it changes nothing.
def retry (state : Option Owed) (token : Nat) : Option Owed :=
  match state with
  | some o => if o.token = token then some { o with failure := none } else some o
  | none => none

-- What the listing shows: the record's failure, if a record is there.
def listed (state : Option Owed) : Option Failure :=
  state.bind (·.failure)

-- Whether writers maintain the index `index`: every index but the one the
-- listed failure names.
def takesWrites (state : Option Owed) (index : Nat) : Bool :=
  match listed state with
  | some f => f.index != index
  | none => true

-- A newly owed reindex lists no failure.
theorem owe_lists_nothing (before : Option Owed) (token : Nat) :
    listed (owe before token) = none := by
  -- The new record's failure is `none` by construction.
  rfl

-- A failure recorded under the record's own token is what the listing shows.
theorem record_own_token_lists (o : Owed) (f : Failure) :
    listed (recordFailure (some o) o.token f) = some f := by
  -- The token matches, so the failure is written and read back.
  simp [recordFailure, listed]

-- A failure from a rebuild of another token leaves the listing as it was.
theorem record_other_token_unchanged (o : Owed) (token : Nat) (f : Failure)
    (other : o.token ≠ token) :
    listed (recordFailure (some o) token f) = listed (some o) := by
  -- The token differs, so the record is kept as it is.
  simp [recordFailure, other]

-- With nothing owed, recording a failure writes nothing.
theorem record_without_record (token : Nat) (f : Failure) :
    recordFailure none token f = none := by
  -- There is no record to write onto.
  rfl

-- A finished rebuild of the record's own token leaves no failure listed.
theorem clear_own_token_lists_nothing (o : Owed) :
    listed (clear (some o) o.token) = none := by
  -- The token matches, so the record and its failure go.
  simp [clear, listed]

-- A finished rebuild of another token leaves the record, and so the listing
-- and which indexes take writes, exactly as they were.
theorem clear_other_token_unchanged (o : Owed) (token : Nat)
    (other : o.token ≠ token) :
    clear (some o) token = some o := by
  -- The token differs, so the record is kept as it is.
  simp [clear, other]

-- Writers do not maintain the index a listed failure names.
theorem failed_index_takes_no_writes (state : Option Owed) (f : Failure)
    (shown : listed state = some f) :
    takesWrites state f.index = false := by
  -- The listed failure names this very index.
  simp [takesWrites, shown]

-- Writers keep maintaining every index the listed failure does not name.
theorem other_index_takes_writes (state : Option Owed) (f : Failure) (index : Nat)
    (shown : listed state = some f) (other : f.index ≠ index) :
    takesWrites state index = true := by
  -- The failure names another index, so this one is still maintained.
  simp [takesWrites, shown, other]

-- After a rebuild of the record's own token tries again, writers maintain
-- every index.
theorem retry_own_token_takes_writes (o : Owed) (index : Nat) :
    takesWrites (retry (some o) o.token) index = true := by
  -- The token matches, so the failure is removed and nothing is listed.
  simp [retry, takesWrites, listed]

-- Trying again keeps the record, under the same token, so the rebuild still
-- owns the reindex it retries.
theorem retry_keeps_token (o : Owed) :
    (retry (some o) o.token).map (·.token) = some o.token := by
  -- The token matches, and only the failure changes.
  simp [retry]

-- A newly owed reindex has writers maintain every index.
theorem owe_takes_writes (before : Option Owed) (token index : Nat) :
    takesWrites (owe before token) index = true := by
  -- The new record has no failure, so nothing is listed.
  rfl

-- Whenever the listing shows a failure, a record is owed and holds it.
theorem listed_needs_record (state : Option Owed) (f : Failure)
    (shown : listed state = some f) :
    ∃ o, state = some o ∧ o.failure = some f := by
  -- No record means nothing listed; a record lists exactly its failure.
  cases state with
  | none => simp [listed] at shown
  | some o => exact ⟨o, rfl, by simpa [listed] using shown⟩

end ReindexFailureListing
