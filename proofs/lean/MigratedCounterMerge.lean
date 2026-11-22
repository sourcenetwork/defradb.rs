/-
# MigratedCounterMerge — an old-version counter update is translated once

The story. A peer increments a counter at the source schema version. The
receiver migrates that counter with a lens. The peer's delta applies to the
source projection, and the resulting source document migrates once.

Rust anchors: `crates/database/db/src/merge/merge_handler/field_ops.rs`
`counter_document_on_active_version`, `counter_result_on_active_version`;
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

end MigratedCounterMerge

#print axioms MigratedCounterMerge.translated_once
#print axioms MigratedCounterMerge.double_apply_differs
