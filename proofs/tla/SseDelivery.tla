------------------------------ MODULE SseDelivery ------------------------------
\* A publisher fills a bounded subscriber queue and may drop an update. The
\* HTTP stream reads updates and runs a scoped query. A loss ends the stream
\* for recovery; a query error is sent, even if it has no rows. This model
\* covers `graphql_sse` in `crates/http/src/handlers/graphql/query.rs`.
\* A red run removes each guard and demonstrates its specific failure.
EXTENDS Naturals

\* The red runs remove either the overflow or the error guard.
CONSTANT Bug
\* full says the one-slot queue is full; lost records a dropped update.
\* busy means the removed update is executing its scoped query; ended is the
\* stream state. unsafe records a query started after a detected loss;
\* swallowed records a query error the stream hid.
VARIABLES full, lost, busy, ended, unsafe, swallowed
\* Every state variable, for unchanged clauses.
vars == <<full, lost, busy, ended, unsafe, swallowed>>

\* The queue and stream start clean.
Init == /\ full = FALSE /\ lost = FALSE /\ busy = FALSE /\ ended = FALSE
        /\ unsafe = FALSE /\ swallowed = FALSE

\* A publish fills the queue, or records loss when it was already full.
Publish ==
  /\ ~ended
  /\ IF full THEN /\ lost' = TRUE /\ UNCHANGED full
             ELSE /\ full' = TRUE /\ UNCHANGED lost
  /\ UNCHANGED <<busy, ended, unsafe, swallowed>>

\* A read checks loss before starting a scoped query. If intact, that query
\* can overlap a publish; the next read then checks for any new loss.
Read ==
  /\ ~ended /\ ~busy /\ full
  /\ full' = FALSE
  /\ IF lost /\ Bug # "overflow"
     THEN /\ ended' = TRUE
          /\ UNCHANGED <<lost, busy, unsafe, swallowed>>
     ELSE /\ ended' = FALSE
          /\ busy' = TRUE
          /\ unsafe' = (unsafe \/ lost)
          /\ UNCHANGED <<lost, swallowed>>

\* A running query succeeds or fails. An error is sent and ends the stream.
\* A loss that happens while it runs is checked by the following read.
Finish ==
  /\ ~ended /\ busy
  /\ busy' = FALSE
  /\ \E failed \in BOOLEAN :
       /\ ended' = (failed /\ Bug # "error")
       /\ swallowed' = (swallowed \/ (failed /\ Bug = "error"))
  /\ UNCHANGED <<full, lost, unsafe>>

\* A publisher or reader takes the next step.
Next == Publish \/ Read \/ Finish

\* The reader never starts another query after it detects a missing update.
INV_DetectedLossEnds == ~unsafe
\* An error is never discarded as though its result were empty.
INV_ErrorForwarded == ~swallowed
=============================================================================
