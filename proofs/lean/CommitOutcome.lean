/-!
The story. When a commit fails, the node tells the caller one of three
things: it committed, it did not (`aborted`), or nobody can know yet
(`indeterminate`). The answer travels as a word in a JSON body and as the
CLI's exit status. A storage failure decides the class: the engine refuses
some commits before it applies anything, and any other failure may come
after the commit reached the log.

What must never happen: a word or an exit status that names two classes,
a class whose word reads back as another, or a failure that may have landed
classed as `aborted`.

Models crates/foundation/defra-core/src/commit_outcome.rs (CommitOutcome:
as_str, from_wire, exit_code) and
crates/engine/storage/src/backends/regolith/transaction.rs (commit_failure,
refused_before_apply).
-/
namespace CommitOutcome

-- The three classes a commit can end in.
inductive Outcome where
  | committed      -- durable and visible
  | notCommitted   -- nothing landed: running it again is safe
  | indeterminate  -- it may have landed: only a later read tells
  deriving DecidableEq, Repr

-- The wire word for each class, as `CommitOutcome::as_str` writes it.
def asStr : Outcome → String
  | .committed => "committed"
  | .notCommitted => "aborted"
  | .indeterminate => "indeterminate"

-- The class a wire word names, as `CommitOutcome::from_wire` reads it;
-- any other word names none.
def fromWire : String → Option Outcome
  | "committed" => some .committed
  | "aborted" => some .notCommitted
  | "indeterminate" => some .indeterminate
  | _ => none

-- The CLI's exit status for each class, as `CommitOutcome::exit_code` gives
-- it: 0 for success, 3 for aborted, 4 for indeterminate.
def exitCode : Outcome → Nat
  | .committed => 0
  | .notCommitted => 3
  | .indeterminate => 4

-- The engine's failures that matter here: refused before applying
-- anything, or failed while applying (an I/O error, a torn record).
inductive EngineFailure where
  | conflict     -- another commit won: nothing applied
  | refusal      -- a stall, a closed handle, a content mismatch: nothing applied
  | io           -- an I/O error while syncing the log: may have landed
  | corruption   -- a torn or unreadable record: may have landed
  deriving DecidableEq, Repr

-- The class of a failed commit, as `commit_failure` and
-- `Error::commit_outcome` decide it.
def classify : EngineFailure → Outcome
  | .conflict => .notCommitted
  | .refusal => .notCommitted
  | .io => .indeterminate
  | .corruption => .indeterminate

-- Whether a failure may have left the commit on disk.
def mayHaveLanded : EngineFailure → Bool
  | .io => true
  | .corruption => true
  | _ => false

-- Every class reads back from its own wire word.
theorem round_trip (o : Outcome) : fromWire (asStr o) = some o := by
  cases o <;> rfl

-- Two classes never share a wire word.
theorem asStr_injective (a b : Outcome) (h : asStr a = asStr b) : a = b := by
  cases a <;> cases b <;> first | rfl | (simp [asStr] at h)

-- Two classes never share an exit status.
theorem exitCode_injective (a b : Outcome) (h : exitCode a = exitCode b) : a = b := by
  cases a <;> cases b <;> first | rfl | (simp [exitCode] at h)

-- No class exits 1 (any other error) or 2 (a usage error), so a script
-- tells a commit's class from every other failure.
theorem exitCode_distinct_from_plain_errors (o : Outcome) :
    exitCode o ≠ 1 ∧ exitCode o ≠ 2 := by
  cases o <;> decide

-- The old wire word is not read as any class.
theorem old_word_is_no_class : fromWire "transaction_conflict_aborted" = none := rfl

-- A failure that may have landed is never classed aborted: a caller that
-- runs an aborted transaction again never applies it twice.
theorem landed_is_never_aborted (f : EngineFailure) (h : mayHaveLanded f = true) :
    classify f ≠ .notCommitted := by
  cases f <;> simp [mayHaveLanded, classify] at h ⊢

-- A refusal is always classed aborted, so the caller may run it again.
theorem refusal_is_aborted (f : EngineFailure) (h : mayHaveLanded f = false) :
    classify f = .notCommitted := by
  cases f <;> simp [mayHaveLanded, classify] at h ⊢

end CommitOutcome
