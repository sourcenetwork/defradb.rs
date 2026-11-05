------------------------------ MODULE SseDelivery ------------------------------
\* A publisher fills a bounded subscriber queue and may drop an update. The
\* publisher can pause after finding a full queue, while the stream removes the
\* last event and waits. Recording the loss also closes the queue, which wakes
\* the stream even if there is no later publication. A query error is sent,
\* even if it has no rows. This models `ChannelBus::publish_raw` and
\* `graphql_sse` in `crates/http/src/handlers/graphql/query.rs`.
\* A red run removes each guard and demonstrates its specific failure.
EXTENDS Naturals

\* The red runs remove the overflow guard, close wake, or error guard.
CONSTANT Bug
\* full says the one-slot queue is full; lost records a dropped update.
\* pending means the publisher saw Full but has not recorded the loss yet.
\* closed means the publisher has closed the queue to wake the reader.
\* busy means the removed update is executing its scoped query; ended is the
\* stream state. unsafe records a query started after a detected loss;
\* swallowed records a query error the stream hid.
VARIABLES full, pending, lost, closed, busy, ended, unsafe, swallowed
\* Every state variable, for unchanged clauses.
vars == <<full, pending, lost, closed, busy, ended, unsafe, swallowed>>

\* The queue and stream start clean.
Init == /\ full = FALSE /\ pending = FALSE /\ lost = FALSE /\ closed = FALSE
        /\ busy = FALSE /\ ended = FALSE
        /\ unsafe = FALSE /\ swallowed = FALSE

\* A publish fills the queue, or finds it full and schedules loss recording.
Publish ==
  /\ ~ended /\ ~closed /\ ~pending
  /\ IF full THEN /\ pending' = TRUE /\ UNCHANGED full
             ELSE /\ full' = TRUE /\ UNCHANGED pending
  /\ UNCHANGED <<lost, closed, busy, ended, unsafe, swallowed>>

\* A publisher that saw Full records the loss and closes the queue. The red
\* wakeup run omits close and can strand a reader that already emptied it.
RecordLoss ==
  /\ pending /\ ~ended
  /\ pending' = FALSE /\ lost' = TRUE
  /\ closed' = (Bug # "wakeup")
  /\ UNCHANGED <<full, busy, ended, unsafe, swallowed>>

\* A read checks loss before starting a scoped query. If intact, that query
\* can overlap a publish; the next read then checks for any new loss.
Read ==
  /\ ~ended /\ ~busy /\ full
  /\ full' = FALSE
  /\ IF lost /\ Bug # "overflow"
     THEN /\ ended' = TRUE
          /\ UNCHANGED <<pending, lost, closed, busy, unsafe, swallowed>>
     ELSE /\ ended' = FALSE
          /\ busy' = TRUE
          /\ unsafe' = (unsafe \/ lost)
          /\ UNCHANGED <<pending, lost, closed, swallowed>>

\* A running query succeeds or fails. An error is sent and ends the stream.
\* A loss that happens while it runs is checked by the following read.
Finish ==
  /\ ~ended /\ busy
  /\ busy' = FALSE
  /\ \E failed \in BOOLEAN :
       /\ ended' = (failed /\ Bug # "error")
       /\ swallowed' = (swallowed \/ (failed /\ Bug = "error"))
  /\ UNCHANGED <<full, pending, lost, closed, unsafe>>

\* A reader waiting on an empty closed queue wakes and ends the stream.
EndClosed ==
  /\ ~ended /\ ~busy /\ ~full /\ closed
  /\ ended' = TRUE
  /\ UNCHANGED <<full, pending, lost, closed, busy, unsafe, swallowed>>

\* A publisher or reader takes the next step.
Next == Publish \/ RecordLoss \/ Read \/ Finish \/ EndClosed

\* The reader never starts another query after it detects a missing update.
INV_DetectedLossEnds == ~unsafe
\* An error is never discarded as though its result were empty.
INV_ErrorForwarded == ~swallowed

\* A recorded or pending loss eventually ends the stream, assuming the
\* publisher and reader continue to take their enabled steps.
LossEndsStream == []((pending \/ lost) => <>ended)

\* Fair processing lets a recorded close wake an idle reader.
Spec == Init /\ [][Next]_vars /\ WF_vars(RecordLoss) /\ WF_vars(Read)
        /\ WF_vars(Finish) /\ WF_vars(EndClosed)
=============================================================================
