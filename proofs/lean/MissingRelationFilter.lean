/-!
# Missing relation filters

When a child is missing, the join first requires an explicit `_docID`
condition. It then evaluates the entire filter against null fields. The
scalar evaluator supplies the two results below; this model does not prove
its operators or mapping implementation.

Rust: `TypeJoinOne::check_relation_filter` in
`crates/engine/query/src/plan/type_join/type_join_one.rs`.
-/

namespace MissingRelationFilter -- Keep these definitions together.

-- The scalar evaluator has already evaluated the ID and all sibling fields
-- against null. `hasId` records whether the filter explicitly names `_docID`.
def accepts (hasId idResult siblingsResult : Bool) : Bool := -- Decide from three inputs.
  hasId && idResult && siblingsResult -- All conditions must hold.

-- A filter without an explicit document ID never accepts a missing child.
theorem without_id_rejected (idResult siblingsResult : Bool) : -- No ID means rejection.
    accepts false idResult siblingsResult = false := by -- State the missing-ID case.
  simp [accepts] -- Reduce the conjunction.

-- A failing document ID condition rejects the missing child.
theorem failing_id_rejected (siblingsResult : Bool) : -- A failing ID rejects.
    accepts true false siblingsResult = false := by -- State the failed-ID case.
  simp [accepts] -- Reduce the conjunction.

-- Any failing sibling condition rejects the missing child.
theorem failing_sibling_rejected (idResult : Bool) : -- A failing sibling rejects.
    accepts true idResult false = false := by -- State the failed-sibling case.
  cases idResult <;> simp [accepts] -- Check both ID results.

-- Acceptance requires the ID condition and both scalar results.
theorem accepted_iff_all_match (hasId idResult siblingsResult : Bool) : -- Acceptance is exact.
    accepts hasId idResult siblingsResult = true ↔ -- Acceptance holds exactly when
      hasId = true ∧ idResult = true ∧ siblingsResult = true := by -- each required input holds.
  cases hasId <;> cases idResult <;> cases siblingsResult <;> simp [accepts] -- Check every input combination.

end MissingRelationFilter -- End the model namespace.
