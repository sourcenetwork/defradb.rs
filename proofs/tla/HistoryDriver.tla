-------------------------- MODULE HistoryDriver --------------------------
\* Production construction and linked collection carriers must preserve driver control.
\* Rust: crates/db/src/merge/replication.rs create_merge_handler_with_max_merge_depth.
\* Rust: crates/db/src/merge/merge_handler/collection.rs process_collection_delta_body.
EXTENDS Naturals
\* One guard returns at the turn boundary; the other propagates a linked history rejection.
CONSTANT SingleTurn, Propagate, Immediate, Fallback
\* These states distinguish local looping from scheduler handoff and terminal rejection.
VARIABLES stage, turns, driver, outcome, ready, paced
\* The complete driver state includes its bounded work count.
vars == <<stage, turns, driver, outcome, ready, paced>>
\* A linked composite has more than one turn of history.
Init == /\ stage = "run" /\ turns = 0 /\ driver = "idle" /\ outcome = "pending" /\ ready = FALSE /\ paced = FALSE
\* A committed partial page yields through the production handler to its scheduler.
Page == /\ stage = "run" /\ turns = 0 /\ turns' = 1
        /\ stage' = IF SingleTurn THEN "scheduled" ELSE "local-loop"
        /\ ready' = Immediate
        /\ UNCHANGED <<driver, outcome, paced>>
\* A later delivery continues the same root without another network message.
Resume == /\ stage = "scheduled" /\ ready /\ stage' = "run"
          /\ UNCHANGED <<turns, driver, outcome, ready, paced>>
\* A linked history abort must reach the driver rather than become a generic retry.
Reject == /\ stage = "run" /\ turns = 1
          /\ driver' = IF Propagate THEN "reject" ELSE "idle"
          /\ outcome' = IF Propagate THEN "rejected" ELSE "retry"
          /\ stage' = "done" /\ UNCHANGED <<turns, ready, paced>>
\* Both rejection and continuation are explored after actual history work.
\* Claiming ready work reserves ordinary retry pacing if the page makes no progress.
Retry == /\ stage = "run" /\ turns = 1
         /\ stage' = "backoff" /\ outcome' = "retry" /\ paced' = Fallback
         /\ UNCHANGED <<turns, driver, ready>>
Next == Page \/ Resume \/ Reject \/ Retry
\* An active turn cannot consume the continuation inside the driver call.
INV_Handoff == stage # "local-loop"
\* A terminal history failure cannot get stuck in an ordinary retry cycle.
INV_Reject == stage = "done" => driver = "reject" /\ outcome = "rejected"
\* Locally ready work is eligible without waiting for a retry timer tick.
INV_Immediate == stage = "scheduled" => ready
\* Transient errors and abandoned claims cannot create an immediate retry loop.
INV_Backoff == stage = "backoff" => paced
=============================================================================
