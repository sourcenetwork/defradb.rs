/-
# MigratedCounterMerge — an old-version counter update is translated once

The story. A peer increments a counter at the source schema version. The
receiver migrates that counter with a lens. The peer's delta applies to the
source projection, and the resulting source document migrates once.

A lens with no inverse cannot give the source projection back. The receiver
then adds the delta to the migrated value as it is. That is the same answer
whenever the lens shifts the counter by a constant or leaves it alone.

Rust anchors: `crates/database/db/src/merge/merge_handler/field_ops.rs`
`counter_on_active_version`, `apply_field_op_once`;
`crates/database/db/src/merge/merge_handler/composite_persist.rs`
`persist_merged_document`.
-/

namespace MigratedCounterMerge

/-- A counter value small enough for the focused rule. -/
abbrev Value := Nat

/-- The focused lens adds one when it migrates to the active version. -/
def migrate (value : Value) : Value := value + 1

/-- GREEN: after the peer's one-unit increment, the active value is the migrated source value. -/
theorem translated_once (source : Value) :
    migrate (source + 1) = migrate source + 1 := rfl

/-- RED: applying the peer's delta to the migrated value and migrating again
can double-apply the lens. -/
theorem double_apply_differs (source : Value) :
    migrate (migrate source + 1) ≠ migrate (source + 1) := by
  simp [migrate]

-- A lens that shifts the counter by a fixed amount, whatever that amount is.
/-- A lens that adds `shift` to the counter when it migrates. -/
def shiftBy (shift value : Value) : Value := value + shift

/-- GREEN: for a lens that shifts the counter, adding the peer's delta to the
migrated value equals migrating the source value plus the delta. -/
theorem direct_add_matches_shift (shift source delta : Value) :
    -- left: add the delta to what is stored; right: go back, add, migrate once.
    shiftBy shift source + delta = shiftBy shift (source + delta) := by
  -- both sides are the same sum with its last two terms swapped.
  simp [shiftBy, Nat.add_right_comm]

/-- GREEN: a lens that leaves the counter alone is the shift by zero, so adding
the delta to the stored value is exactly the migrated sum. -/
theorem direct_add_matches_untouched (source delta : Value) :
    -- shifting by zero changes nothing on either side.
    shiftBy 0 source + delta = source + delta := by
  simp [shiftBy]

end MigratedCounterMerge

#print axioms MigratedCounterMerge.translated_once
#print axioms MigratedCounterMerge.double_apply_differs
#print axioms MigratedCounterMerge.direct_add_matches_shift
#print axioms MigratedCounterMerge.direct_add_matches_untouched
