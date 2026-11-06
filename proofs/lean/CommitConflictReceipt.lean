import Lean

/-!
The transaction registry maps a storage conflict to `typedConflict` only
when the commit lost validation before its log record. This storage property
and the HTTP server's dispatch of the typed result are native premises.
-/
namespace CommitConflictReceipt

inductive CommitResult where
  | typedConflict
  | executionFailure
  | unknownAfterDispatch
  deriving DecidableEq

/-- Only the direct typed commit conflict carries a confirmed-abort receipt. -/
def confirmedAbortReceipt : CommitResult → Bool
  | .typedConflict => true
  | .executionFailure => false
  | .unknownAfterDispatch => false

theorem receipt_implies_typed_conflict (result : CommitResult)
    (receipt : confirmedAbortReceipt result = true) : result = .typedConflict := by
  cases result <;> simp_all [confirmedAbortReceipt]

theorem execution_failure_has_no_receipt :
    confirmedAbortReceipt .executionFailure = false := rfl

theorem unknown_after_dispatch_has_no_receipt :
    confirmedAbortReceipt .unknownAfterDispatch = false := rfl

end CommitConflictReceipt

def main : IO Unit := do
  let results := [CommitConflictReceipt.CommitResult.typedConflict,
    .executionFailure, .unknownAfterDispatch]
  IO.println (Lean.toJson (results.map CommitConflictReceipt.confirmedAbortReceipt)).compress
