-------------------------- MODULE HttpIndexBuild --------------------------
\* A client asks for an ordered or vector index, synchronously or in the
\* background. The server commits the definition, runs its backfill and
\* responds. Background requests may respond after the definition commits;
\* synchronous requests wait for backfill. Build mode must not change kind.
\* Rust: handlers/index/go_api.rs go_create_index; index_adapter.rs
\* IndexAdapter::create_index; db/src/index/create.rs create_index/start_index.
EXTENDS Naturals
\* Background and Vector describe the request. DropVector plants the bug
\* that discards a vector configuration on a background request.
CONSTANTS Background, Vector, DropVector
\* defined, built, responded track the steps; storedVector is the chosen kind.
VARIABLES defined, built, responded, storedVector
\* All state, including the index kind.
vars == <<defined, built, responded, storedVector>>
\* No definition has been committed or answered.
Init == /\ defined = FALSE /\ built = FALSE /\ responded = FALSE
        /\ storedVector = FALSE
\* The definition records the supplied kind unless the bug is enabled.
Define == /\ ~defined /\ defined' = TRUE
          /\ storedVector' = (Vector /\ ~(Background /\ DropVector))
          /\ UNCHANGED <<built, responded>>
\* Backfill runs after definition publication.
Build == /\ defined /\ ~built /\ built' = TRUE
         /\ UNCHANGED <<defined, responded, storedVector>>
\* A background response follows publication; a synchronous one follows build.
Respond == /\ defined /\ ~responded /\ (Background \/ built)
           /\ responded' = TRUE /\ UNCHANGED <<defined, built, storedVector>>
\* Each operation may run in order allowed by its guard.
Next == Define \/ Build \/ Respond
\* The execution may also stutter.
Spec == Init /\ [][Next]_vars
\* An index definition has exactly the kind requested by the caller.
INV_KindPreserved == defined => storedVector = Vector
\* Synchronous creation never reports success before the backfill finishes.
INV_SyncWaits == (responded /\ ~Background) => built
=============================================================================
