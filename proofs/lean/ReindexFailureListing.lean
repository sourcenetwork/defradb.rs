/-!
The story, in plain words. A collection may owe a reindex: a record with a
token and, once a rebuild stopped on a unique clash, the failure it met (the
index and the reason). The index listing shows that failure as a failed
build. Four operations touch the record: owing a reindex writes a new record
without a failure; a stopped rebuild writes its failure onto the record only
if the record still has the rebuild's token; a finished rebuild removes the
record if it still has its token; the listing reads the failure off the
record.

What must never happen: the listing shows a failure the current record did
not get from a rebuild of its own token, or shows one when nothing is owed.

Rust owner: crates/database/db/src/definition/migration/reindex/owed.rs,
owe_reindex / owe_full_reindex (failure: None), record_failure (owed_under
the token), clear_owed_reindex (removes the record), owed_failure (what the
listing shows, through crates/database/db/src/database/action.rs
index_listing_executions).
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

-- What the listing shows: the record's failure, if a record is there.
def listed (state : Option Owed) : Option Failure :=
  state.bind (·.failure)

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

-- Whenever the listing shows a failure, a record is owed and holds it.
theorem listed_needs_record (state : Option Owed) (f : Failure)
    (shown : listed state = some f) :
    ∃ o, state = some o ∧ o.failure = some f := by
  -- No record means nothing listed; a record lists exactly its failure.
  cases state with
  | none => simp [listed] at shown
  | some o => exact ⟨o, rfl, by simpa [listed] using shown⟩

end ReindexFailureListing
