/-!
# Nested secondary relation ID filters

The story: a query filter is a list of top-level conditions that must all
hold (an implicit AND). Some conditions name a secondary relation ID, such as
`_publisherID` on a `Book`, which is known only after the join to the
publisher. The scan runs before that join, so the planner gives it a reduced
filter. It drops a whole `_or` or `_not` that names an ID, and keeps the other
conjuncts of an `_and`. The planner runs the full filter after the joins
whenever that reduction dropped anything.

What must never happen: a document that matches the full filter is thrown
away by the scan, or a document that does not match it is returned.

Dropping a whole top-level condition can only widen the scan, because the
scan checks a subset of an AND. Dropping one branch of an `_or` instead would
narrow it: the last theorem uses `scanFilter` to show a document that the full
`_or` accepts and a branch-pruned scan rejects, which is why the planner drops
the whole operator.

Rust: `Filter::without_secondary_relation_ids` in
`crates/engine/query/src/mapper/filter/secondary_id.rs`, and the
`nested_secondary_relation_id` complexity flag in `Planner::prepare_filter`
in `crates/engine/query/src/planner/builder/filter_prep.rs`.
-/

namespace NestedRelationIdFilter -- Keep these definitions together.

-- A condition is any yes-or-no test on a document of type `D`.
abbrev Cond (D : Type) := D → Bool -- A condition answers true or false.

-- A filter accepts a document when every one of its conditions holds.
def accepts {D : Type} (conds : List (Cond D)) (d : D) : Bool := -- The implicit AND.
  conds.all (fun c => c d) -- Every condition must say yes.

-- The scan keeps the conditions that `keep` allows and drops the rest.
def scanFilter {D : Type} (keep : Cond D → Bool) (conds : List (Cond D)) : List (Cond D) := -- The reduced filter.
  conds.filter keep -- Only kept conditions reach the scan.

-- The planner must run the full filter whenever the scan lost a condition.
def isComplex {D : Type} (keep : Cond D → Bool) (conds : List (Cond D)) : Bool := -- The full-filter gate.
  !conds.all keep -- True whenever at least one condition is dropped.

-- The answer: the scan filter, then the full filter when the gate is on.
def answer {D : Type} (keep : Cond D → Bool) (conds : List (Cond D)) (d : D) : Bool := -- Gated stages.
  accepts (scanFilter keep conds) d && (!isComplex keep conds || accepts conds d) -- Scan and optional full check.

-- Anything the full filter accepts also passes the reduced scan filter.
theorem scan_never_drops_a_match {D : Type} (keep : Cond D → Bool) (conds : List (Cond D)) (d : D) -- No lost rows.
    (h : accepts conds d = true) : accepts (scanFilter keep conds) d = true := by -- A full match survives the scan.
  simp only [accepts, scanFilter, List.all_eq_true, List.mem_filter] at * -- Unfold to "every member holds".
  exact fun c hc => h c hc.1 -- A kept condition is a member of the full list.

-- The two stages together answer exactly what the full filter answers.
theorem answer_is_full_filter {D : Type} (keep : Cond D → Bool) (conds : List (Cond D)) (d : D) : -- Exact result.
    answer keep conds d = accepts conds d := by -- The answer equals the full filter.
  unfold answer -- Expose the scan and the gated full check.
  by_cases hc : isComplex keep conds -- Split on whether the scan was reduced.
  · cases h : accepts conds d -- Split on the full filter's answer.
    · simp [hc, h] -- A rejected document stays rejected.
    · have hscan := scan_never_drops_a_match keep conds d h -- A match survives the scan.
      simp [hc, h, hscan] -- The full check then preserves the match.
  · have hs : scanFilter keep conds = conds := by -- The gate is off only when nothing was dropped.
      have hkeep : ∀ c ∈ conds, keep c = true := by -- Every condition was kept.
        exact List.all_eq_true.mp (by simpa [isComplex] using hc) -- Unfold the gate.
      exact List.filter_eq_self.mpr hkeep -- Filtering by an always-true check changes nothing.
    cases h : accepts conds d <;> simp [hc, hs, h] -- The scan itself is the full filter.

-- An `_or` accepts when any of its branches holds.
def acceptsAny {D : Type} (conds : List (Cond D)) (d : D) : Bool := -- Any branch succeeds.
  conds.any (fun c => c d) -- One true branch is enough.

-- The two branches of the example `_or`: the ID branch and a false branch.
def orBranches : List (Cond Bool) := -- The modeled `_or`.
  [fun d => d, fun _ => false] -- An ID test beside a condition that never holds.

-- A bad reduction that keeps only the false branch.
def keepOther (c : Cond Bool) : Bool := -- Choose the second branch.
  !c true -- The ID branch is true here, so negating it selects the false branch.

-- Pruning one `_or` branch would lose a match: `true` satisfies the full
-- `_or`, while the scan built from only the false branch rejects it.
theorem pruning_an_or_branch_loses_a_match : -- Why the whole operator is dropped.
    ∃ d : Bool, acceptsAny orBranches d = true ∧ -- The full `_or` accepts d,
      accepts (scanFilter keepOther orBranches) d = false := by -- but the pruned scan rejects d.
  refine ⟨true, ?_, ?_⟩ <;> -- The document `true` is the witness.
    simp [acceptsAny, orBranches, scanFilter, keepOther, accepts] -- Evaluate both sides.

end NestedRelationIdFilter -- End the model namespace.
