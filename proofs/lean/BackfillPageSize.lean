/-!
The story. An index backfill indexes existing documents one page at a time.
The operator sets a batch limit: the most documents one page may index. A
page that commits doubles the next page, up to the built-in page size, and
then the backfill clamps it back to the limit. A page that conflicts retries
at half its size, but never below one document.
What must never happen: a page bigger than the limit.

Rust owner: crates/database/db/src/index/backfill.rs DB::backfill_batches
(the clamp), crates/database/db/src/index/pages.rs DB::run_page (the
doubling) and PageAttempt::run (the halving).
-/
namespace BackfillPageSize

-- The size of the next page after a commit of size `s`: doubled, capped at
-- the built-in page size, then clamped to the configured limit.
def grow (s pageItems limit : Nat) : Nat := min (min (2 * s) pageItems) limit

-- The size of a retry after a conflict at size `s`: half, but at least one.
def shrink (s : Nat) : Nat := max (s / 2) 1

-- A page grown after a commit never exceeds the configured limit.
theorem grow_le_limit (s pageItems limit : Nat) : grow s pageItems limit ≤ limit := by
  unfold grow; omega

-- A grown page never exceeds the built-in page size either.
theorem grow_le_pageItems (s pageItems limit : Nat) :
    grow s pageItems limit ≤ pageItems := by
  unfold grow; omega

-- A page at or under a limit of at least one stays under it after a conflict.
theorem shrink_le_limit (s limit : Nat) (h : s ≤ limit) (one : 1 ≤ limit) :
    shrink s ≤ limit := by
  unfold shrink; omega

-- A retry always asks for at least one document, so the backfill progresses.
theorem shrink_pos (s : Nat) : 1 ≤ shrink s := by
  unfold shrink; omega

end BackfillPageSize
