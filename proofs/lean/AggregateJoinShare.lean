/-!
# When two aggregates may read through one join

The story. A query asks several aggregates over one relation of each parent,
such as `SUM(enrollments: {field: grade, filter: {grade: {_gt: 75}}})` beside
`MAX(enrollments: {field: grade, filter: {grade: {_lt: 85}}})`. The planner
builds a join that reads a parent's related documents through a scan. The
scan already applies a filter (the "scan filter"), and afterwards each
aggregate applies its own filter to what the join handed it.

What can happen. A second aggregate may reuse a join that an earlier aggregate
built, or get a join of its own.

What must never happen. An aggregate must never see fewer documents than its
own filter keeps. Reusing a join whose scan filter differs drops documents the
second aggregate wanted: Alice's 70 fails `> 75`, so `MAX(< 85)` saw nothing.

Rust owner: crates/engine/query/src/planner/joins/aggregate_joins.rs,
Planner::apply_aggregate_joins and aggregate_scan_filter. A join is shared only
when the relation and the scan filter are the same; otherwise the aggregate
gets a join of its own.
-/
namespace AggregateJoinShare

-- A filter is a yes/no test on one related document (a grade here).
abbrev Filter := Nat → Bool

-- What an aggregate sees through a join whose scan kept `scan`: the scan's
-- documents, then the aggregate's own filter `own` on top.
def seen (scan own : Filter) (docs : List Nat) : List Nat :=
  (docs.filter scan).filter own

-- What the aggregate should see: every document its own filter keeps.
def wanted (own : Filter) (docs : List Nat) : List Nat :=
  docs.filter own

-- The planner's choice: share the earlier join when its scan filter is the
-- aggregate's own (`same = true`), otherwise read through a join whose scan
-- filter is the aggregate's own.
def planned (same : Bool) (earlier own : Filter) (docs : List Nat) : List Nat :=
  if same then seen earlier own docs else seen own own docs

-- Applying the same filter twice keeps the same documents as applying it once.
theorem filter_twice (f : Filter) (docs : List Nat) :
    (docs.filter f).filter f = docs.filter f := by
  -- Walk the list one document at a time.
  induction docs with
  -- No documents: both sides are empty.
  | nil => rfl
  -- One more document: it is kept by both sides or by neither.
  | cons d rest ih =>
    -- Split on whether the filter keeps this document.
    cases h : f d <;> simp [List.filter, h, ih]

-- Sharing a join whose scan filter equals the aggregate's own filter loses nothing.
theorem shared_same_filter_exact (own : Filter) (docs : List Nat) :
    seen own own docs = wanted own docs :=
  -- That is exactly applying the same filter twice.
  filter_twice own docs

-- The planner's choice always gives the aggregate what its filter keeps,
-- provided it shares only when the two filters are the same.
theorem planned_exact (same : Bool) (earlier own : Filter) (docs : List Nat)
    (sharesOnlyEqual : same = true → earlier = own) :
    planned same earlier own docs = wanted own docs := by
  -- Look at both choices.
  cases same with
  -- Its own join: the scan filter is the aggregate's own.
  | false => exact shared_same_filter_exact own docs
  -- A shared join: its scan filter is the aggregate's own by the rule.
  | true =>
    -- Replace the earlier join's filter with the aggregate's own.
    have h := sharesOnlyEqual rfl
    -- Then it is the same-filter case.
    simp only [planned, if_pos, h]
    exact shared_same_filter_exact own docs

-- Alice's grades: 90 in Math, 70 in Science.
def alice : List Nat := [90, 70]

-- The SUM's filter: grades above 75.
def above75 : Filter := fun g => decide (g > 75)

-- The MAX's filter: grades below 85.
def below85 : Filter := fun g => decide (g < 85)

-- The bug this rule removes: sharing the SUM's join with the MAX leaves the
-- MAX nothing, though Alice's 70 passes its filter.
theorem shared_other_filter_drops :
    seen above75 below85 alice = [] ∧ wanted below85 alice = [70] := by
  -- Both sides are concrete lists: evaluate them.
  decide

end AggregateJoinShare
