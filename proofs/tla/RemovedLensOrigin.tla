------------------------- MODULE RemovedLensOrigin -------------------------
\* An operator removes a lens. A reindex walks a document that was created
\* directly at the active version, so the lens never applied to it. Reindex
\* must keep its value. The old inverse rule changed this document as well as
\* materialized documents, because both had the same stored version.
\* Rust: definition/migration/reindex/page.rs ReindexPage::walk.
EXTENDS Naturals
\* Reverse is the unsafe rule that inverts every active-version document.
CONSTANT Reverse
\* value is the document's stored scalar; done records the reindex.
VARIABLES value, done
\* All state variables, for stuttering.
vars == <<value, done>>
\* The native document was created with value 40.
Init == /\ value = 40 /\ done = FALSE
\* An unsafe inverse subtracts 5; a safe rebuild leaves the value alone.
Reindex == /\ ~done
           /\ value' = IF Reverse THEN value - 5 ELSE value
           /\ done' = TRUE
\* Reindex is the only modeled action.
Next == Reindex
\* The system may also stutter.
Spec == Init /\ [][Next]_vars
\* A native document's value survives removal and reindex.
INV_NativePreserved == value = 40
=============================================================================
