/-!
# Missing relation filters

When a child is missing, the join first requires an explicit `_docID`
condition. It then evaluates the entire filter against null fields. The
scalar evaluator supplies the two results below; this model does not prove
its operators or mapping implementation.

Rust: `TypeJoinOne::check_relation_filter` in
`crates/engine/query/src/plan/type_join/type_join_one.rs`.
-/

namespace MissingRelationFilter

-- The scalar evaluator has already evaluated the ID and all sibling fields
-- against null. `hasId` records whether the filter explicitly names `_docID`.
def accepts (hasId idResult siblingsResult : Bool) : Bool :=
  hasId && idResult && siblingsResult

theorem without_id_rejected (idResult siblingsResult : Bool) :
    accepts false idResult siblingsResult = false := by
  simp [accepts]

theorem failing_id_rejected (siblingsResult : Bool) :
    accepts true false siblingsResult = false := by
  simp [accepts]

theorem failing_sibling_rejected (idResult : Bool) :
    accepts true idResult false = false := by
  cases idResult <;> simp [accepts]

theorem accepted_iff_all_match (hasId idResult siblingsResult : Bool) :
    accepts hasId idResult siblingsResult = true ↔
      hasId = true ∧ idResult = true ∧ siblingsResult = true := by
  cases hasId <;> cases idResult <;> cases siblingsResult <;> simp [accepts]

end MissingRelationFilter
