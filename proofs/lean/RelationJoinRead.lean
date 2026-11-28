/-!
# A query join never validates its scan

The story, in plain words. Two explicit transactions add different books and
run `Book { name publisher { name } }`. The join scans every book. A third
writer can add the other book before either transaction commits.

What must happen:

- the join's query scan decides nothing, so it never fails a commit;
- adding a book validates its own check that the publisher has no other book,
  so two books cannot commit for the same publisher.

What must never happen:

- a transaction fails only because the join scan saw another book;
- both transactions commit a book for the same one-to-one publisher.

Rust owners:

- `crates/engine/query/src/runner/query/nested.rs`
  (`QueryRunner::execute_nested_select_with_planner`)
- `crates/database/db/src/index/manager/unique.rs`
  (`IndexManager::save_local_unique_validated`)

The temporal interleavings are in `RelationJoinRead.tla`; this file checks
the decision table those interleavings use at commit.
-/

namespace RelationJoinRead

-- The two guards carried into a second writer's commit decision.
structure Guards where
  -- Whether the query's join range counts at commit.
  joinScanValidated : Bool
  -- Whether the writer's publisher-entry lookup counts at commit.
  ownEntryValidated : Bool

-- What a second writer's commit does.
inductive Outcome where
  -- The second book lands.
  | commit
  -- The second book is refused with a transaction conflict.
  | abort
  deriving DecidableEq, Repr

-- The shipped guards: the join decides nothing; the writer's own link check
-- does decide.
def shipped : Guards where
  joinScanValidated := false
  ownEntryValidated := true

-- A second writer is refused when its own link check is validated and sees a
-- holder, or when it only saw another book through a validated join scan.
def refused (guards : Guards) (samePublisher : Bool) : Bool :=
  -- A taken publisher link is a write-side conflict when that lookup counts.
  guards.ownEntryValidated && samePublisher ||
  -- A book elsewhere in the join's range is only a conflict when that query
  -- read wrongly counts.
  guards.joinScanValidated && !samePublisher

-- Turn the decision table into the second writer's commit outcome.
def secondOutcome (guards : Guards) (samePublisher : Bool) : Outcome :=
  -- A checked crossing write aborts; otherwise the second book lands.
  if refused guards samePublisher then .abort else .commit

-- Promise: with a different publisher, the second book commits because the
-- query join is not validated.
theorem distinct_publisher_commits :
    secondOutcome shipped false = .commit := by
  -- The shipped join and link checks do not cross a different publisher.
  rfl

-- Promise: with the same publisher, the writer's validated link check refuses
-- the second book.
theorem same_publisher_aborts :
    secondOutcome shipped true = .abort := by
  -- The validated own-entry check sees the first book's link.
  rfl

-- Promise: the shipped guards keep one book per publisher.
theorem one_book_per_publisher (samePublisher : Bool)
    (same : samePublisher = true) :
    secondOutcome shipped samePublisher = .abort := by
  -- Substitute the only publisher-sharing case and use the check above.
  subst samePublisher
  -- The second writer's own link validation refuses it.
  exact same_publisher_aborts

-- Counterexample: validating the query join refuses a different publisher,
-- although that transaction's own link is valid.
def validatedJoin : Guards where
  joinScanValidated := true
  ownEntryValidated := true

-- Promise: the join-validation bug makes an unrelated book abort the commit.
theorem validated_join_refuses_a_different_publisher :
    secondOutcome validatedJoin false = .abort := by
  -- The validated join treats the other book in its scan as a conflict.
  rfl

-- Counterexample: dropping the writer's own validation lets two books link
-- the same one-to-one publisher.
def missingOwnCheck : Guards where
  joinScanValidated := false
  ownEntryValidated := false

-- Promise: without the writer's own check, the second same-publisher book
-- commits and breaks the one-to-one relation.
theorem missing_own_check_permits_a_second_link :
    secondOutcome missingOwnCheck true = .commit := by
  -- Neither read is validated, so the crossed publisher link is not refused.
  rfl

end RelationJoinRead
