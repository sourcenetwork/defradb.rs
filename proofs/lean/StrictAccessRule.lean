/-! What a read does with one row's access check, by default and when the
caller asked for a strict read. Models `AcpReadCheck::allows` and the
classification of a failed check in
`crates/engine/query/src/plan/permission_filter.rs`;
`proofs/tla/StrictAccessRead.tla` models the scan around it.
-/
namespace StrictAccessRule

-- What one row's check came to.
inductive Check where
  -- The provider answered: this row may be read.
  | allow
  -- The provider answered: this row may not be read.
  | deny
  -- The provider did not answer in time, after its own waits.
  | late
  -- No answer for another reason (storage, a malformed answer).
  | broken
  deriving DecidableEq, Repr

-- What the read does with the row.
inductive Step where
  -- The row stays in the read's answer.
  | keep
  -- The row is filtered out, without turning the read into an error.
  | drop
  -- The request fails, retryable when the provider was late.
  | fail (retryable : Bool)
  deriving DecidableEq, Repr

-- The rule: an answer decides the row in both modes; no answer drops it by
-- default and fails a strict read.
def step (strict : Bool) : Check → Step
  | .allow => .keep
  | .deny => .drop
  | .late => if strict then .fail true else .drop
  | .broken => if strict then .fail false else .drop

-- A denial only ever filters the row, in both modes.
theorem denial_drops (strict : Bool) : step strict .deny = .drop := rfl

-- An allowed row is kept in both modes.
theorem allow_keeps (strict : Bool) : step strict .allow = .keep := rfl

-- The default never fails a read on a check.
theorem default_never_fails (c : Check) (r : Bool) : step false c ≠ .fail r := by
  cases c <;> simp [step]

-- A strict read never drops a row whose check had no answer.
theorem strict_never_drops_unanswered (c : Check) (h : c = .late ∨ c = .broken) :
    step true c ≠ .drop := by
  rcases h with h | h <;> subst h <;> simp [step]

-- A strict read fails only on a check that had no answer.
theorem strict_fails_only_unanswered (c : Check) (r : Bool) (h : step true c = .fail r) :
    c = .late ∨ c = .broken := by
  cases c <;> simp [step] at h ⊢

-- Only a late provider makes the failure retryable.
theorem retryable_iff_late (c : Check) (r : Bool) (h : step true c = .fail r) :
    r = true ↔ c = .late := by
  cases c <;> simp [step] at h ⊢ <;> simp [h]

-- No step keeps a row its check did not allow.
theorem kept_only_allowed (strict : Bool) (c : Check) (h : step strict c = .keep) :
    c = .allow := by
  cases c <;> cases strict <;> simp [step] at h ⊢

end StrictAccessRule
