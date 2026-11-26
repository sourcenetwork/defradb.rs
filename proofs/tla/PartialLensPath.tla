------------------------- MODULE PartialLensPath -------------------------
\* A document starts before two version transitions. The first has a lens;
\* the second may acquire one later. A reindex may index a partially lensed
\* read, but must not stamp it at the destination until both lenses exist.
\* Otherwise a later second lens is skipped on reads of the stamped document.
\* Rust: definition/lens.rs migration_path_transforms, migration/helpers.rs
\* migrate_to_active, migration/reindex/page.rs ReindexPage::walk.
EXTENDS Naturals

\* Guard is false in the old behavior that materialized partial reads.
CONSTANT Guard
\* second says whether the second lens is registered.
\* stamped says whether a reindex stored the document at the destination.
\* stampedPartial records that a partial read was stamped.
VARIABLES second, stamped, stampedPartial
\* All state variables, for stuttering.
vars == <<second, stamped, stampedPartial>>

\* Initially the second lens is absent and the document is at its source.
Init == /\ second = FALSE /\ stamped = FALSE /\ stampedPartial = FALSE

\* Registering a lens on the missing hop leaves the stored document alone.
Register == /\ ~second /\ second' = TRUE /\ UNCHANGED <<stamped, stampedPartial>>

\* A rebuild can stamp only a complete path when the guard is enabled.
Reindex == /\ ~stamped
           /\ IF Guard /\ ~second
                 THEN /\ UNCHANGED <<second, stamped, stampedPartial>>
                 ELSE /\ stamped' = TRUE
                      /\ stampedPartial' = ~second
                      /\ UNCHANGED second

\* Either operation may occur, in either order.
Next == Register \/ Reindex
\* The system may also stutter.
Spec == Init /\ [][Next]_vars
\* No partial path is ever stamped, even if the missing lens is added later.
INV_NoPartialStamp == ~stampedPartial
=============================================================================
