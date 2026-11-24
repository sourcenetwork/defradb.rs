/-!
# Explicit transaction link reads validate their own decision only

The story, in plain words. A local unique-index save turns scan validation on
for the one lookup it decides on, then puts the setting back. A one-to-one
relation join does the same for the scan range its result depends on. Either
read can refuse a second link after the first took the value, but neither can
leave the whole transaction validating every later scan.

Rust owners: `crates/database/db/src/index/manager/unique.rs`
(`save_local_unique_validated`) and
`crates/engine/query/src/runner/query/nested.rs`
(`has_one_to_one_relation_select`).
-/

namespace RelationLinkConflict

-- The mode of a link lookup: validated by its own decision, or unvalidated.
inductive Mode where
  | validated
  | unvalidated
  deriving DecidableEq, Repr

-- Whether two links have committed.
structure Links where
  first : Bool
  second : Bool

-- Whether the conflict decision lets a second link commit when the first
-- link's state is `taken`.
def admitsSecond : Mode → Bool → Bool
  | .validated, false => true
  | .validated, true => false
  | .unvalidated, _ => true

-- A validated second link commits only when it read an empty index.
theorem validated_admits_only_empty (mode : Mode) (taken : Bool)
    (admits : admitsSecond mode taken = true) (validated : mode = .validated) :
    taken = false := by
  subst validated
  cases taken
  · rfl
  · simp [admitsSecond] at admits

-- Once the first link took the value, a validated second link is refused.
theorem validated_taken_is_refused :
    admitsSecond .validated true = false := by
  rfl

-- Unvalidated mode is the counterexample: it lets both links land.
theorem unvalidated_admits_both :
    admitsSecond .unvalidated true = true := by
  rfl

end RelationLinkConflict
