---- MODULE HeaderRetry ----
\* The fetched head is tried first, and its proof read fails transiently.
\* The same head was queued after subscription and authenticates on retry.
\* Suppressing the queued copy based on an unverified head loses that retry.
\* Rust: crates/engine/vera-embedded/src/acp.rs prepend_head.
EXTENDS Naturals, Sequences
CONSTANT KeepQueued
VARIABLES queue, attempts, authenticated
\* State of the stream and the light client's verified head.
vars == <<queue, attempts, authenticated>>
\* Both the fetched head and its queued copy name the latest block.
Init == /\ queue = <<1, 1>>
        /\ attempts = 0
        /\ authenticated = FALSE
\* The first proof read fails; the red mode drops the duplicate before retry.
Consume == /\ queue # <<>>
           /\ attempts' = attempts + 1
           /\ authenticated' = (authenticated \/ attempts > 0)
           /\ queue' = IF attempts = 0 /\ ~KeepQueued THEN <<>> ELSE Tail(queue)
\* Consume at most the two offered headers.
Next == Consume
Spec == Init /\ [][Next]_vars
\* A drained queue leaves the client authenticated after the transient error.
INV_Recovers == queue = <<>> => authenticated
====
