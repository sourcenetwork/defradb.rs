/-!
A document at the active version may have been created there or materialized
through a lens. Its version and stored value alone do not distinguish those
cases. Applying the inverse to both would change an ordinary new document.

Rust owner: crates/database/db/src/definition/migration/reindex/page.rs,
ReindexPage::walk. This proof models an increment lens, not arbitrary lenses.
-/
namespace RemovedLensOrigin

-- The stored value is shared by a native document and a materialized one.
def storedValue : Nat := 40

-- The native document's value should remain unchanged.
def nativeValue : Nat := 40

-- Reversing an increment changes the native document.
theorem inverse_changes_native : storedValue - 5 ≠ nativeValue := by decide

-- Keeping the stored value preserves a document created at the active version.
theorem preserving_keeps_native : storedValue = nativeValue := rfl

-- The same stored representation cannot by itself identify its origin.
theorem same_stored_value_different_origins :
    storedValue = 35 + 5 ∧ storedValue = nativeValue := by decide
end RemovedLensOrigin
