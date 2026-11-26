/-!
# AggregateJoinReuse: an aggregate reads every child its own filter matches

The story, in plain words. A query can ask for a relation's children and, next
to them, for aggregates over the same relation:

    authoredBooks(filter: {pages: {_gt: 100}}, limit: 1) { title }
    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})

The planner fetches children through joins. To save work, an aggregate may read
the rows of a join that is already there instead of building its own:

- the rendered selection's join, and
- the join an earlier aggregate built.

Who acts: the planner, choosing which join each aggregate reads; then the
runner, which applies the aggregate's own filter, order, limit and offset to
the rows it reads.

What can happen: a join's rows are the children that pass its scan filter,
cut to its limit when it has one, and only the fields its builder asked for are
rendered in them.

What must never happen: an aggregate reads fewer rows, other rows, or rows
missing a field it reads, than its own scan would give. Issue #3276 was both
mistakes at once: SUM read COUNT's join, which it found by relation name alone,
and so it read the selection's `limit: 1` rows.

The rules this file proves enough:
- reuse an earlier aggregate's join only when its scan filter is the
  aggregate's own and it rendered every field the aggregate reads
  (`reuse_reads_own_rows`, `reuse_keeps_read_fields`);
- share a selection's join only when it has the aggregate's filter and no limit
  (`selection_share_reads_own_rows`). "The aggregate's filter" is its scan
  filter, an average's not-null condition included, and no filter matches only
  no filter, as in Go's `Filter.equal`.
And why less is not enough: a limited selection drops rows
(`limited_selection_loses_rows`), a filtered selection hides children from an
unfiltered aggregate (`filtered_selection_hides_rows_from_unfiltered`, the
planner's old "no filter, has a field: share" branch), a join found by relation alone has another
filter's rows (`relation_only_reuse_reads_other_rows`), and a join that did not
render a field hides it (`unrendered_field_is_missing`).

Rust anchors:
- `AggregateJoin::serves`, `scan_filter`
  (`crates/engine/query/src/planner/joins/aggregate_join_reuse.rs`);
- `Planner::apply_aggregate_joins`
  (`crates/engine/query/src/planner/joins/aggregate_joins.rs`);
- `build_selection_join_info`, `SelectionJoinInfo::has_limit`
  (`crates/engine/query/src/planner/joins/selection.rs`, `shared.rs`);
- the selection scan's aggregate filter in `Planner::build_relation_child_plan`
  (`crates/engine/query/src/planner/joins/child_plan.rs`): only a target with
  the selection's own filter narrows the selection's scan, so the selection's
  rows stay its own (`selection_share_reads_own_rows` read the other way).
-/

namespace AggregateJoinReuse

-- A scan filter decides, for one child row, whether the join keeps it.
abbrev Filter (R : Type) := R → Bool

-- The rows an aggregate's own join would read: every child its scan filter keeps.
def ownRows (f : Filter R) (rows : List R) : List R :=
  -- the scan keeps exactly the rows passing the filter
  rows.filter f

-- An earlier aggregate's join: its scan filter (the rows it holds follow from it).
structure AggJoin (R : Type) where
  -- the filter its child scan applied
  scan : Filter R

-- The rows a join holds.
def AggJoin.rows (j : AggJoin R) (children : List R) : List R :=
  -- an aggregate join is never paged: it holds its whole scan
  children.filter j.scan

-- A rendered selection's join: its filter and its limit. Its offset is not
-- here: the runner applies it after the aggregates have read the rows, but a
-- limit lets the join stop fetching early.
structure SelJoin (R : Type) where
  -- the selection's filter
  filter : Filter R
  -- the selection's limit, if any
  limit : Option Nat

-- The rows a selection's join holds: its filtered children, cut to its limit.
def SelJoin.rows (s : SelJoin R) (children : List R) : List R :=
  -- filter first, then keep at most the limit
  match s.limit with
  | some n => (children.filter s.filter).take n
  | none => children.filter s.filter

-- Reusing an earlier aggregate's join whose scan filter is the aggregate's own
-- gives exactly the rows the aggregate's own join would read.
theorem reuse_reads_own_rows (j : AggJoin R) (f : Filter R) (children : List R)
    (same : j.scan = f) : j.rows children = ownRows f children := by
  -- both are the children filtered by the same filter
  simp [AggJoin.rows, ownRows, same]

-- Sharing a selection's join with an aggregate of the same filter, when the
-- selection has no limit, gives exactly the aggregate's own rows.
theorem selection_share_reads_own_rows (s : SelJoin R) (f : Filter R) (children : List R)
    (same : s.filter = f) (unlimited : s.limit = none) :
    s.rows children = ownRows f children := by
  -- no limit drops nothing, and the filters agree
  simp [SelJoin.rows, ownRows, unlimited, same]

-- A rendered row holds only the fields its join asked for: the others read as missing.
def render [DecidableEq S] (fields : List S) (row : S → Option V) : S → Option V :=
  -- keep a field's value when it was asked for, otherwise nothing
  fun x => if x ∈ fields then row x else none

-- Reusing a join that rendered every field the aggregate reads keeps those values.
theorem reuse_keeps_read_fields [DecidableEq S] (fields reads : List S)
    (row : S → Option V) (covered : ∀ x ∈ reads, x ∈ fields) :
    ∀ x ∈ reads, render fields row x = row x := by
  -- each field read was rendered, so its value is the row's own
  intro x hx
  simp [render, covered x hx]

-- The children of the issue: page counts 80, 120, 150 and 330.
def books : List Nat := [80, 120, 150, 330]

-- The issue's filter: more than 100 pages.
def over100 : Filter Nat := fun pages => decide (100 < pages)

-- Another aggregate's filter: more than 140 pages.
def over140 : Filter Nat := fun pages => decide (140 < pages)

-- Why the limit matters: the issue's `limit: 1` selection holds one row where
-- the aggregate must read three, so a limited selection must not be shared.
theorem limited_selection_loses_rows :
    (SelJoin.rows ⟨over100, some 1⟩ books) ≠ ownRows over100 books := by
  -- one row against three
  decide

-- An aggregate with no filter: its scan keeps every child.
def noFilter : Filter Nat := fun _ => true

-- Why an absent filter matches only an absent one: `authoredBooks(filter:
-- {pages: {_gt: 100}})` beside `SUM(authoredBooks: {field: pages})` must sum all
-- four books (680), but the selection's join holds three (600), and MIN would
-- see 120 instead of 80.
theorem filtered_selection_hides_rows_from_unfiltered :
    (SelJoin.rows ⟨over100, none⟩ books) ≠ ownRows noFilter books := by
  -- three rows against four
  decide

-- Why the filter matters: a join found by relation name alone (the old rule)
-- holds another filter's rows, here COUNT's `_gt: 100` rows for a `_gt: 140` SUM.
theorem relation_only_reuse_reads_other_rows :
    (AggJoin.rows ⟨over100⟩ books) ≠ ownRows over140 books := by
  -- 120 is in the first list and not in the second
  decide

-- Why the fields matter: a join that rendered only `pages` hides `rating`.
theorem unrendered_field_is_missing :
    render ["pages"] (fun _ => some (4 : Nat)) "rating" = none := by
  -- "rating" was not asked for
  decide

end AggregateJoinReuse
