/-!
A document can cross several collection-version transitions. Some transitions
have a lens and others currently have none. Reindexing may index the current
read result, but stamping that result at the destination would hide a lens
registered on a previously empty transition later.

This proves the path classification used by `migration_path_transforms` in
`crates/database/db/src/definition/lens.rs` and the materialization guard in
`crates/database/db/src/definition/migration/reindex/page.rs`. It models lens
presence, not the contents or effects of the lens programs.
-/
namespace PartialLensPath

-- A path is a list of transitions; true means that transition has a lens.
abbrev Path := List Bool

-- A nonempty path is complete only when every transition has a lens.
def complete (path : Path) : Bool := !path.isEmpty && path.all id

-- Materialization is permitted only for a complete path.
def mayMaterialize (path : Path) : Bool := complete path

-- A partial path cannot be materialized at the destination.
theorem partial_not_materialized (path : Path) (h : path.all id = false) :
    mayMaterialize path = false := by
  simp [mayMaterialize, complete, h]

-- The empty path is not a completed transformed path.
theorem empty_not_materialized : mayMaterialize [] = false := by
  rfl

-- A missing hop prevents a document with another transformed hop being stamped.
theorem missing_hop_not_materialized : mayMaterialize [true, false] = false := by
  rfl

-- Registering the missing lens permits materialization on the now complete path.
theorem filled_hop_materialized : mayMaterialize [true, true] = true := by
  rfl
end PartialLensPath
