------------------------- MODULE ContinuationSlot -------------------------
\* A queued continuation owns a slot even if reconciliation skips its merge.
\* Rust: process_one_event in sync/replication/handlers.rs; the regression is
\* already_merged_duplicate_continuation_releases_reservation_without_handler.
EXTENDS Naturals
\* The RED witness leaves ownership until after the already-merged check.
CONSTANT LateClaim
\* A slot is scheduled, owned by the handler, or released after its return.
VARIABLES stage, reserved, owned, outcome
\* Keep the handler lifecycle together for unchanged clauses.
vars == <<stage, reserved, owned, outcome>>
\* Dispatch has reserved the slot before queuing the event.
Init == /\ stage = "queued" /\ reserved = TRUE
        /\ owned = FALSE /\ outcome = "none"
\* The handler claims the reservation before any reconciliation await or exit.
Enter == /\ stage = "queued" /\ stage' = "running"
         /\ owned' = ~LateClaim /\ UNCHANGED <<reserved, outcome>>
\* Reconciliation may skip, fail, be cancelled, or continue into the merge.
Decide == /\ stage = "running" /\ stage' = "returning"
          /\ outcome' \in {"already merged", "failed", "cancelled", "merged"}
          /\ owned' = (owned \/ outcome' = "merged")
          /\ UNCHANGED reserved
\* Dropping the handler's owner releases its reservation on every exit path.
Return == /\ stage = "returning" /\ stage' = "done"
          /\ reserved' = (reserved /\ ~owned) /\ owned' = FALSE
          /\ UNCHANGED outcome
\* Every completed event relinquishes its slot, including early returns.
INV_Released == stage = "done" => ~reserved
\* Explore every outcome with and without the misplaced claim.
Next == Enter \/ Decide \/ Return
=============================================================================
