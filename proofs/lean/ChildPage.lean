import Std

/-!
# ChildPage: stopping after an ordered child's page

A parent receives children in the requested order. Once the scan has found
`offset + limit` children belonging to that parent, later children cannot
change its page. The scan must count matches for this parent, not all index
entries. The proof assumes the input already has the requested order; it does
not prove that the planner selected an index with that order.

Rust anchor: `TypeJoinMany::next_per_parent` in
`crates/engine/query/src/plan/type_join/type_join_many/children.rs`.
-/

namespace ChildPage

def page (children : List Nat) (offset limit : Nat) : List Nat :=
  (children.drop offset).take limit

-- An ordered scan can stop after the offset and page have been collected.
theorem stop_after_page (children : List Nat) (offset limit : Nat) :
    (children.take (offset + limit)).drop offset = page children offset limit := by
  have count : offset + limit - offset = limit := by omega
  simp [page, List.drop_take, count]

-- Filtering the ordered scan for a parent first gives the same page, whether
-- the scan stops once that parent's page is full or reads to the end.
theorem stop_after_parent_page (children : List Nat) (belongs : Nat → Bool)
    (offset limit : Nat) :
    ((children.filter belongs).take (offset + limit)).drop offset =
      page (children.filter belongs) offset limit := by
  exact stop_after_page (children.filter belongs) offset limit

end ChildPage
