----------------------- MODULE SelectedPushIsolation -----------------------
\* Story. A caller asks the database to send a few chosen documents to one
\* peer. The call schedules one task per document, at most Cap at a time, and
\* each task sends its document's head blocks one PushLog at a time. A separate
\* subscription to the same peer has filters and a retry backoff; neither may
\* refuse the chosen documents, and a receiver hint for this call must not touch
\* that subscription's schedule.
\* Things go wrong: a send times out (the peer is silent), the connection drops,
\* or the receiver asks for a pause. The caller may also give up and drop the
\* call. Any of these means the transfer cannot finish now, so the call stops:
\* it schedules no more documents and no waiting task sends again. Sends already
\* on the wire end on their own deadline and are joined before the call returns.
\* Never: scheduling the rest of the selection after a stop, more sends after a
\* stop than there are send slots, more timeouts than send slots, returning
\* before every started task is joined, or success with a document unacknowledged.
\* Rust: crates/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist
\* (selected ownership); push_docs/stop.rs SelectedPushStop; push_docs_replay.rs
\* ReplayPushGate::send_pushlog_with_admission; crates/p2p-adapter/src/
\* pooled_doc_pusher.rs push_existing_docs_by_id, which carries the caller's drop.
EXTENDS Naturals, FiniteSets
\* Bug names the one guard a RED configuration removes; "none" keeps them all.
\* N is how many documents were chosen; Cap is the slot count (8 in Rust).
CONSTANTS Bug, N, Cap
\* The chosen documents, numbered 1 to N.
Docs == 1..N
\* Document 1 has two head blocks, so a running task meets a block boundary.
Blocks(d) == IF d = 1 THEN 2 ELSE 1
\* phase: "idle" before the call, "running" while it works, "done" once returned.
\* task[d]: "new" not scheduled; "queued" scheduled and waiting for a send slot;
\*   "admitted" passed the stop check and holds a send slot; "sending" its
\*   PushLog is on the wire; "acked" every block acknowledged; "failed" a send
\*   failed; "abandoned" it saw the stop before sending again.
\* blocksDone[d]: how many of d's blocks the receiver acknowledged.
\* checked: the document the scheduler just checked and is about to start, or 0.
\* stopped: the per-call stop flag the code reads (SelectedPushStop).
\* stopSeen: ghost truth that a failure or cancellation already happened.
\* cancelled: the caller dropped the call. silent: the peer stopped answering.
\* lateSchedules / lateSends: documents started / PushLogs started after stopSeen.
\* timeouts: how many sends ended on their deadline.
\* result: the call's answer. refused: subscription state refused the call.
\* hint / replicatorHint: a receiver hint arrived / leaked into the subscription.
VARIABLES phase, task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
          lateSchedules, lateSends, timeouts, result, refused, hint, replicatorHint
\* Every variable, for steps that leave the rest unchanged.
vars == <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
          lateSchedules, lateSends, timeouts, result, refused, hint, replicatorHint>>
\* Tasks holding a document slot: started and not yet ended.
InFlight == {d \in Docs : task[d] \in {"queued", "admitted", "sending"}}
\* Tasks holding a send slot: past the stop check or on the wire.
SendSlots == {d \in Docs : task[d] \in {"admitted", "sending"}}
\* Nothing has happened yet; every document is waiting to be scheduled.
Init == /\ phase = "idle" /\ task = [d \in Docs |-> "new"]
        /\ blocksDone = [d \in Docs |-> 0] /\ checked = 0
        /\ stopped = FALSE /\ stopSeen = FALSE /\ cancelled = FALSE /\ silent = FALSE
        /\ lateSchedules = 0 /\ lateSends = 0 /\ timeouts = 0 /\ result = "pending"
        /\ refused = FALSE /\ hint = FALSE /\ replicatorHint = FALSE
\* The call starts. Selected sends never consult the subscription's filter or
\* backoff; the "filter" and "backoff" bugs let that state refuse the call.
Start == /\ phase = "idle"
         /\ refused' = (Bug \in {"filter", "backoff"})
         \* A refused call returns at once with an error and sends nothing.
         /\ phase' = IF refused' THEN "done" ELSE "running"
         /\ result' = IF refused' THEN "err" ELSE "pending"
         /\ UNCHANGED <<task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                        lateSchedules, lateSends, timeouts, hint, replicatorHint>>
\* A failure that proves the transfer cannot progress: the task raises the stop
\* flag before it releases its slots. The "stall" bug forgets to raise it.
RaiseStop == /\ stopSeen' = TRUE
             /\ stopped' = (stopped \/ Bug # "stall")
\* The scheduler holds a document slot and reads the stop flag before starting
\* document d. The "schedule" bug skips the read.
Check(d) == /\ phase = "running" /\ checked = 0 /\ task[d] = "new"
            \* A document slot is free (the checked document holds one).
            /\ Cardinality(InFlight) < Cap
            \* The flag is down, unless the bug skips the read.
            /\ (~stopped \/ Bug = "schedule")
            /\ checked' = d
            /\ UNCHANGED <<phase, task, blocksDone, stopped, stopSeen, cancelled, silent,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint>>
\* The checked document's task starts. A stop can land between the read and the
\* start (another thread raised it); that one document is counted as late.
Spawn == /\ checked # 0
         /\ task' = [task EXCEPT ![checked] = "queued"]
         /\ lateSchedules' = lateSchedules + (IF stopSeen THEN 1 ELSE 0)
         /\ checked' = 0
         /\ UNCHANGED <<phase, blocksDone, stopped, stopSeen, cancelled, silent,
                        lateSends, timeouts, result, refused, hint, replicatorHint>>
\* A waiting task takes a send slot and reads the stop flag before its next
\* PushLog; this is also every block boundary. The "admission" bug skips it.
Admit(d) == /\ task[d] = "queued" /\ Cardinality(SendSlots) < Cap
            /\ (~stopped \/ Bug = "admission")
            /\ task' = [task EXCEPT ![d] = "admitted"]
            /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint>>
\* A waiting task reads a raised flag and ends without sending.
Abandon(d) == /\ task[d] = "queued" /\ stopped /\ Bug # "admission"
              /\ task' = [task EXCEPT ![d] = "abandoned"]
              /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled,
                             silent, lateSchedules, lateSends, timeouts, result, refused,
                             hint, replicatorHint>>
\* An admitted task puts its PushLog on the wire; one starting after a stop is late.
Send(d) == /\ task[d] = "admitted"
           /\ task' = [task EXCEPT ![d] = "sending"]
           /\ lateSends' = lateSends + (IF stopSeen THEN 1 ELSE 0)
           /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                          lateSchedules, timeouts, result, refused, hint, replicatorHint>>
\* The receiver acknowledges the block; the task finishes or waits for its next one.
Ack(d) == /\ task[d] = "sending" /\ ~silent
          /\ blocksDone' = [blocksDone EXCEPT ![d] = @ + 1]
          /\ task' = [task EXCEPT ![d] = IF blocksDone'[d] = Blocks(d) THEN "acked" ELSE "queued"]
          /\ UNCHANGED <<phase, checked, stopped, stopSeen, cancelled, silent,
                         lateSchedules, lateSends, timeouts, result, refused, hint,
                         replicatorHint>>
\* The receiver refuses this head without a hint: only this document fails.
Refuse(d) == /\ task[d] = "sending" /\ ~silent
             /\ task' = [task EXCEPT ![d] = "failed"]
             /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled,
                            silent, lateSchedules, lateSends, timeouts, result, refused,
                            hint, replicatorHint>>
\* The send's own deadline passes: the peer is stalled, so the call stops.
Timeout(d) == /\ task[d] = "sending"
              /\ task' = [task EXCEPT ![d] = "failed"]
              /\ timeouts' = timeouts + 1
              /\ RaiseStop
              /\ UNCHANGED <<phase, blocksDone, checked, cancelled, silent, lateSchedules,
                             lateSends, result, refused, hint, replicatorHint>>
\* The connection drops under the send: the call stops.
Disconnect(d) == /\ task[d] = "sending"
                 /\ task' = [task EXCEPT ![d] = "failed"]
                 /\ RaiseStop
                 /\ UNCHANGED <<phase, blocksDone, checked, cancelled, silent,
                                lateSchedules, lateSends, timeouts, result, refused, hint,
                                replicatorHint>>
\* The receiver asks for a pause: the call stops, and the hint stays with this
\* call. The "hint" bug writes it into the subscription's retry schedule.
Hint(d) == /\ task[d] = "sending" /\ ~silent
           /\ task' = [task EXCEPT ![d] = "failed"]
           /\ hint' = TRUE /\ replicatorHint' = (replicatorHint \/ Bug = "hint")
           /\ RaiseStop
           /\ UNCHANGED <<phase, blocksDone, checked, cancelled, silent, lateSchedules,
                          lateSends, timeouts, result, refused>>
\* From now on the peer answers nothing; every send on the wire times out.
GoSilent == /\ phase = "running" /\ ~silent /\ silent' = TRUE
            /\ UNCHANGED <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint>>
\* The caller drops the call; its drop guard raises the stop flag. The "cancel"
\* bug drops it without raising the flag, so the work it left runs on.
Cancel == /\ phase = "running" /\ ~cancelled
          /\ cancelled' = TRUE /\ stopSeen' = TRUE
          /\ stopped' = (stopped \/ Bug # "cancel")
          /\ UNCHANGED <<phase, task, blocksDone, checked, silent, lateSchedules, lateSends,
                         timeouts, result, refused, hint, replicatorHint>>
\* Scheduling has ended: the flag stopped it, or every document was started.
SchedulingOver == checked = 0 /\ (stopped \/ \A d \in Docs : task[d] # "new")
\* The call returns once scheduling is over and every started task is joined.
\* The "drain" bug returns at the first failure with tasks still running.
\* Success needs every chosen document acknowledged; the "partial" bug only asks
\* that nothing failed, so unscheduled or abandoned documents pass as success.
Finish == /\ phase = "running" /\ SchedulingOver
          /\ (InFlight = {} \/ (Bug = "drain" /\ \E d \in Docs : task[d] = "failed"))
          /\ phase' = "done"
          /\ result' = IF IF Bug = "partial" THEN \A d \in Docs : task[d] # "failed"
                                             ELSE \A d \in Docs : task[d] = "acked"
                       THEN "ok" ELSE "err"
          /\ UNCHANGED <<task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                         lateSchedules, lateSends, timeouts, refused, hint, replicatorHint>>
\* Any of the steps above, in any order.
Next == \/ Start \/ Spawn \/ GoSilent \/ Cancel \/ Finish
        \/ \E d \in Docs : \/ Check(d) \/ Admit(d) \/ Abandon(d) \/ Send(d) \/ Ack(d)
                           \/ Refuse(d) \/ Timeout(d) \/ Disconnect(d) \/ Hint(d)
\* The whole behaviour: start from Init and take any Next step.
Spec == Init /\ [][Next]_vars
\* Every variable keeps a value of its intended kind.
TypeOK == /\ phase \in {"idle", "running", "done"}
          /\ task \in [Docs -> {"new", "queued", "admitted", "sending", "acked", "failed",
                                "abandoned"}]
          /\ blocksDone \in [Docs -> 0..2] /\ checked \in 0..N
          /\ result \in {"pending", "ok", "err"}
\* Subscription filters and backoff never refuse an explicit selection.
INV_Independent == ~refused
\* A receiver hint for this call never changes the subscription's schedule.
INV_HintOwner == ~replicatorHint
\* The call returns only after every task it started has been joined.
INV_Drained == phase = "done" => InFlight = {}
\* After a stop, at most the one document already past its check is started.
INV_ScheduleBound == lateSchedules <= 1
\* After a stop, only PushLogs already holding a send slot may start.
INV_SendBound == lateSends <= Cap
\* A stalled peer costs at most one deadline per send slot, not one per document.
INV_StallBound == timeouts <= Cap
\* Success means every chosen document was acknowledged.
INV_CompletionTruth == result = "ok" => \A d \in Docs : task[d] = "acked"
=============================================================================
