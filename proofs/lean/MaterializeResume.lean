/-!
The story, in plain words. A collection holds documents with short IDs 1 to
n. A settlement walks them in order, a page at a time, fixing the index and
leaving every document at its older version; a crash can stop it after any
document, and its cursor says which. A materialization then walks the
documents and saves each one it walks at the active version. If it carried on
from the settlement's cursor, the documents before that cursor would never
be saved. So it starts over from the first document.

Rust owner: crates/database/db/src/definition/migration/reindex/mod.rs,
DB::reindex_in_pages (a rebuild that restamps owes a reindex of its own).
-/
namespace MaterializeResume

-- The documents a walk that starts after `cursor` visits: every short ID from
-- 1 to n that comes after the cursor.
def walked (cursor n : Nat) : List Nat :=
  ((List.range n).map (· + 1)).filter (fun d => cursor < d)

-- Starting over (cursor 0) visits, and so saves, every document 1 to n.
theorem start_over_saves_all (n d : Nat) (low : 1 ≤ d) (high : d ≤ n) :
    d ∈ walked 0 n := by
  -- d is in the list when d - 1 is below n and d is after cursor 0.
  refine List.mem_filter.mpr ⟨List.mem_map.mpr ⟨d - 1, List.mem_range.mpr (by omega), by omega⟩, ?_⟩
  -- 0 < d, because d is at least 1.
  simp; omega

-- Resuming a settlement's cursor never visits a document at or before it, so
-- that document is never saved at the active version.
theorem resume_misses (cursor n d : Nat) (walkedBySettlement : d ≤ cursor) :
    d ∉ walked cursor n := by
  -- A member would have to come after the cursor, and d does not.
  intro member
  -- Unpack the filter: membership demands cursor < d.
  have after := (List.mem_filter.mp member).2
  -- That contradicts d ≤ cursor.
  simp at after; omega

end MaterializeResume
