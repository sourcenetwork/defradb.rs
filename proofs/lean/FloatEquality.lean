/-
# FloatEquality — exact float filters

The story. A filter compares a stored float with its target. Exact equality
accepts only that target. A tolerance can accept a different number, so it
must not serve this filter.

Rust anchors: `crates/engine/query/src/mapper/filter/eval/operators.rs`
`values_equal`; `crates/engine/storage/src/index/matcher/mod.rs`
`values_equal`.
-/

namespace FloatEquality

/-- A natural stands in for a numeric value; the proof only needs distinctness. -/
abbrev Value := Nat

/-- The exact rule: the two values are the same. -/
def exactEq (a b : Value) : Prop := a = b

/-- The old tolerant rule: a fixed-width window can bridge distinct values. -/
def epsilonEq (a b : Value) (eps : Value) : Prop :=
  (if a <= b then b - a else a - b) < eps

/-- GREEN: when `a = b`, exact equality accepts it. -/
theorem exact_accepts_same {a : Value} : exactEq a a := rfl

/-- RED: a tolerance can accept distinct values. -/
theorem epsilon_accepts_distinct : ∃ a b eps : Value,
    a ≠ b ∧ epsilonEq a b eps :=
  ⟨0, 1, 2, by decide, by simp [epsilonEq]⟩

/-- RED: the tolerant result can contain a value exact equality rejects. -/
theorem tolerant_has_rejects : ∃ target other eps : Value,
    other ≠ target ∧ epsilonEq other target eps :=
  ⟨0, 1, 2, by decide, by simp [epsilonEq]⟩

end FloatEquality

#print axioms FloatEquality.exact_accepts_same
#print axioms FloatEquality.epsilon_accepts_distinct
#print axioms FloatEquality.tolerant_has_rejects
