----------------------- MODULE SelectedPushIsolation -----------------------
\* Two explicitly selected documents exist and have valid authority. A separate
\* subscription filters them out and is in backoff; neither controls selection.
\* An acknowledgement-storage failure must not leave the other send running.
\* Rust: push_docs.rs; transport_doc_pusher.rs push_existing_docs_by_id.
EXTENDS Naturals
\* Each negative control restores one unwanted coupling or premature return.
CONSTANT Bug
\* Pending sends and error reporting are separate from replicator retry state.
VARIABLES started, remaining, returned, refused, hint, replicatorHint
\* The complete state for unchanged clauses.
vars == <<started, remaining, returned, refused, hint, replicatorHint>>
\* The independent replicator's filters/backoff are assumed to be restrictive.
Init == /\ started = FALSE /\ remaining = 0 /\ returned = FALSE
        /\ refused = FALSE /\ hint = FALSE /\ replicatorHint = FALSE
\* Selected sends do not consult those subscription-owned restrictions.
Start == /\ ~started /\ started' = TRUE
         /\ refused' = (Bug \in {"filter", "backoff"})
         /\ remaining' = IF refused' THEN 0 ELSE 2
         /\ UNCHANGED <<returned, hint, replicatorHint>>
\* A receiver hint belongs to this explicit call, not durable replication.
RetryHint == /\ started /\ ~returned /\ ~hint
             /\ hint' = TRUE /\ replicatorHint' = (Bug = "hint")
             /\ UNCHANGED <<started, remaining, returned, refused>>
\* Storage fails for the first acknowledgement, but all tasks must still join.
Join == /\ started /\ remaining > 0 /\ ~returned
        /\ remaining' = remaining - 1
        /\ returned' = (remaining' = 0 \/ Bug = "drain")
        /\ UNCHANGED <<started, refused, hint, replicatorHint>>
\* Existing selected documents cannot be refused by subscription state.
INV_Independent == ~refused
\* A selected reply cannot modify the replicator's persisted schedule.
INV_HintOwner == ~replicatorHint
\* Returning an error still joins every started task.
INV_Drained == returned => remaining = 0
\* Explore receiver hints interleaved with completion and error reporting.
Next == Start \/ RetryHint \/ Join
=============================================================================
