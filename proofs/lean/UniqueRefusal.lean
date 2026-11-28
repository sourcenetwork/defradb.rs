/-!
# How a lost unique race is refused

The story: two clients of one node write the same non-nil value into a field
with a unique index. One of them wins. This file is about what the other one
is told, which depends only on how it committed:

- an explicit transaction the client commits itself loses at its commit and is
  told `transaction conflict. Please retry`, with no error code;
- an autocommit write the node runs is run again by the node, finds the
  winner's entry, and is refused as a unique violation, whose GraphQL error
  carries the code `UNIQUE_CONSTRAINT_VIOLATION`. That needs a retry budget of
  at least one re-run (`DbOptions::with_max_txn_retries`, 5 by default); with
  none, the lost first commit ends in `TransactionRetryExhausted` instead,
  which this file does not cover.

What must never happen: an autocommit write answered with a conflict, an
explicit commit answered with a unique code, or two refusals with the same
code that mean different things. `ClientUniqueAdmission.tla` checks the race
itself; this file pins the words and codes, and `Conformance.lean` hands them
to `tests/lean_conformance.rs`, which compares them with the live Rust
constants.

Rust it models: `crates/engine/query/src/executor.rs` `TXN_CONFLICT_MESSAGE`,
`UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE` and
`QueryResponseError::from_query_error`;
`crates/engine/storage/src/corekv/errors.rs`
`UNIQUE_CONSTRAINT_VIOLATION_MESSAGE`;
`crates/database/db/src/error/mod.rs` `index_write_query_error`.
-/
namespace UniqueRefusal

-- The two ways a client's write is committed.
inductive CommitMode where
  -- The client began the transaction and commits it.
  | explicitTxn
  -- The node runs the write in a transaction of its own.
  | autocommit
  deriving DecidableEq, Repr

-- The two answers a client that lost the race can get.
inductive RefusalKind where
  -- The commit lost a conflict.
  | txnConflict
  -- The write found another document's unique entry.
  | uniqueViolation
  deriving DecidableEq, Repr

namespace RefusalKind

-- The Rust name of each kind, which the contract carries.
def toDefraDB : RefusalKind → String
  -- The explicit commit's refusal.
  | .txnConflict     => "TxnConflict"
  -- The unique index's refusal.
  | .uniqueViolation => "UniqueViolation"

-- The text each kind starts with. A unique violation adds the writer's DocID
-- and value after it.
def message : RefusalKind → String
  -- `TXN_CONFLICT_MESSAGE`.
  | .txnConflict     => "transaction conflict. Please retry"
  -- `UNIQUE_CONSTRAINT_VIOLATION_MESSAGE`.
  | .uniqueViolation => "can not index a doc's field(s) that violates unique index."

-- The error code each kind carries: none on a commit, the unique code on a
-- mutation.
def code : RefusalKind → Option String
  -- A commit answers with its message alone.
  | .txnConflict     => none
  -- `UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE`.
  | .uniqueViolation => some "UNIQUE_CONSTRAINT_VIOLATION"

end RefusalKind

namespace CommitMode

-- The Rust-facing name of each mode, which the contract carries.
def toDefraDB : CommitMode → String
  -- An explicit transaction.
  | .explicitTxn => "Explicit"
  -- An autocommit write.
  | .autocommit  => "Autocommit"

end CommitMode

open RefusalKind

-- What the losing client of each mode is told, given a retry budget of at
-- least one re-run (see `autocommitLoser`).
def loserKind : CommitMode → RefusalKind
  -- Its own commit lost, so it hears of the conflict.
  | .explicitTxn => .txnConflict
  -- The node ran it again, and the re-run met the winner's entry.
  | .autocommit  => .uniqueViolation

-- What an autocommit loser is told with `reruns` re-runs allowed: none when
-- the budget allows no re-run, since the lost commit then ends in
-- `TransactionRetryExhausted` before it can meet the winner's entry.
def autocommitLoser (reruns : Nat) : Option RefusalKind :=
  -- No re-run: the budget is spent at the lost commit.
  if reruns = 0 then none
  -- A re-run meets the winner's entry.
  else some (loserKind .autocommit)

-- Every kind, for the contract.
def kinds : List RefusalKind := [.txnConflict, .uniqueViolation]

-- Every mode, for the contract.
def modes : List CommitMode := [.explicitTxn, .autocommit]

/-- With at least one re-run allowed, an autocommit loser is told of the unique
    violation, never of a conflict. -/
theorem autocommit_loser_is_unique (reruns : Nat) (h : 0 < reruns) :
    autocommitLoser reruns = some .uniqueViolation := by
  -- A positive budget is not zero, so the re-run branch is taken.
  have hz : reruns ≠ 0 := by omega
  -- Unfold the answer and the loser's kind.
  simp [autocommitLoser, loserKind, hz]

/-- With no re-run allowed, this file promises the autocommit loser nothing. -/
theorem no_rerun_is_not_covered : autocommitLoser 0 = none := rfl

/-- An explicit loser is told its commit conflicted. -/
theorem explicit_loser_is_conflict : loserKind .explicitTxn = .txnConflict := rfl

/-- A commit's refusal carries no code. -/
theorem conflict_has_no_code : code .txnConflict = none := rfl

/-- A unique refusal carries `UNIQUE_CONSTRAINT_VIOLATION`. -/
theorem unique_has_its_code : code .uniqueViolation = some "UNIQUE_CONSTRAINT_VIOLATION" := rfl

/-- Two kinds with the same code are the same kind, so a client can tell the
    answers apart by code alone. -/
theorem code_injective (a b : RefusalKind) (h : code a = code b) : a = b := by
  -- Check the four pairs of kinds one by one.
  cases a <;> cases b <;> first | rfl | (simp [code] at h)

/-- Two kinds with the same message are the same kind. -/
theorem message_injective (a b : RefusalKind) (h : message a = message b) : a = b := by
  -- Check the four pairs of kinds one by one; distinct strings differ.
  cases a <;> cases b <;> first | rfl | (simp [message] at h)

/-- The two modes tell their losers different things. -/
theorem modes_differ : loserKind .explicitTxn ≠ loserKind .autocommit := by
  -- The two answers are different constructors.
  decide

-- One contract entry per kind: its name, message and code (empty for none).
def kindEntry (k : RefusalKind) : String :=
  k.toDefraDB ++ "|" ++ k.message ++ "|" ++ (k.code.getD "")

-- One contract entry per mode: the mode and its loser's kind.
def modeEntry (m : CommitMode) : String :=
  m.toDefraDB ++ "->" ++ (loserKind m).toDefraDB

-- The vocabulary `tests/lean_conformance.rs` compares with the Rust constants.
def contract : List String := kinds.map kindEntry ++ modes.map modeEntry

end UniqueRefusal
