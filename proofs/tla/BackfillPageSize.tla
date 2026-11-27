--------------------------- MODULE BackfillPageSize ---------------------------
\* The story. An index backfill walks a collection's existing documents in
\* pages, one engine transaction per page. The operator configures how many
\* documents one transaction may index at most (the batch limit). A page that
\* commits doubles the size of the next one, up to the built-in page size; a
\* page that conflicts is retried at half its size, down to one document.
\* What must never happen: a page indexes more documents than the configured
\* batch limit, because the doubling forgot about the limit.
\* Rust: crates/database/db/src/index/backfill.rs DB::backfill_batches (the
\* clamp to the limit after each page); crates/database/db/src/index/pages.rs
\* DB::run_page (the doubling) and PageAttempt::run (the halving).
EXTENDS Naturals
\* Docs: documents to index. Limit: the configured batch limit.
\* PageItems: the built-in page size. Capped: TRUE when the clamp is in place.
CONSTANTS Docs, Limit, PageItems, Capped
\* size: the size the next attempt gets. indexed: documents done so far.
\* biggest: the most documents any committed page indexed.
VARIABLES size, indexed, biggest
\* Every variable, for stuttering steps.
vars == <<size, indexed, biggest>>
\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b
\* The larger of two numbers.
Max(a, b) == IF a > b THEN a ELSE b
\* The first page gets the configured limit, nothing is indexed yet.
Init == /\ size = Limit
        /\ indexed = 0
        /\ biggest = 0
\* After a commit of an attempt of size s, the next page doubles, never past
\* the built-in page size, and, with the clamp, never past the limit.
Grow(s) == IF Capped THEN Min(Min(2 * s, PageItems), Limit)
                     ELSE Min(2 * s, PageItems)
\* A page commits: it indexes up to `size` of the documents left, and the
\* next page grows from the size this attempt ran at.
Commit == /\ indexed < Docs
          /\ LET n == Min(size, Docs - indexed)
             IN /\ indexed' = indexed + n
                /\ biggest' = Max(biggest, n)
          /\ size' = Grow(size)
\* A page conflicts: its retry runs at half the size, but at least one.
Conflict == /\ indexed < Docs
            /\ size' = Max(size \div 2, 1)
            /\ UNCHANGED <<indexed, biggest>>
\* Any page either commits or conflicts; a finished backfill stutters.
Next == Commit \/ Conflict
\* The whole behaviour.
Spec == Init /\ [][Next]_vars
\* No committed page ever indexed more documents than the configured limit.
INV_BatchBound == biggest <= Limit
\* The next attempt never asks for more documents than the configured limit.
INV_SizeBound == size <= Limit
\* The backfill never counts more documents than exist.
INV_Indexed == indexed <= Docs
=============================================================================
