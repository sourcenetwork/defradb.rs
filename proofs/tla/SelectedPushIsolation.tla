----------------------- MODULE SelectedPushIsolation -----------------------
\* Story. A caller asks the database to send a few chosen documents to one
\* peer. The call schedules one task per document, at most Cap at a time, and
\* each task sends its document's head blocks one PushLog at a time. A separate
\* subscription to the same peer has filters and a retry backoff; neither may
\* refuse the chosen documents, and a receiver hint for this call must not touch
\* that subscription's schedule.
\* Things go wrong: a send times out (the peer is silent), the connection drops,
\* or the receiver asks for a pause. A pause is retried within the current head's deadline.
\* A send deadline, disconnect or caller drop stops the call: it schedules no
\* more documents and no waiting task sends again. Sends already on the wire
\* end on their own deadline and are joined before the call returns. A head
\* whose pauses outlast its own deadline fails alone: the receiver is
\* answering, so the rest of the selection is still tried.
\* Never: scheduling the rest of the selection after a stop, more sends after a
\* stop than there are send slots, more timeouts than send slots, returning
\* before every started task is joined, success with a document unacknowledged,
\* or a receiver pause ending the selection.
\* Rust: crates/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist
\* (selected ownership); push_docs/stop.rs SelectedPushStop and stops_selected_transfer;
\* push_docs/selected.rs HeadDeadline and send; push_docs_replay.rs
\* ReplayPushGate::send_pushlog_with_admission; crates/p2p-adapter/src/
\* pooled_doc_pusher.rs push_existing_docs_by_id, which carries the caller's drop.
EXTENDS Naturals, FiniteSets
\* Bug names the one guard a RED configuration removes; "none" keeps them all.
\* N is how many documents were chosen; Cap is the slot count (8 in Rust).
CONSTANTS Bug, N, Cap
\* Two abstract ticks stand for the fixed configured send-timeout budget.
Budget == 2
\* The chosen documents, numbered 1 to N.
Docs == 1..N
\* Document 1 has two head blocks, so a running task meets a block boundary.
Blocks(d) == IF d = 1 THEN 2 ELSE 1
\* phase: "idle" before the call, "running" while it works, "done" once returned.
\* task[d]: "new" not scheduled; "queued" scheduled and waiting for a send slot;
\*   "admitted" passed the stop check and holds a send slot; "sending" its
\*   PushLog is on the wire; "acked" every block acknowledged; "failed" a send
\*   failed or its head's pauses outlasted its deadline; "abandoned" it saw
\*   the stop before sending again.
\* blocksDone[d]: how many of d's blocks the receiver acknowledged.
\* checked: the document the scheduler just checked and is about to start, or 0.
\* stopped: the per-call stop flag the code reads (SelectedPushStop).
\* stopSeen: ghost truth that a hard failure (timeout, disconnect) or the
\*   caller's cancellation already happened.
\* cancelled: the caller dropped the call. silent: the peer stopped answering.
\* lateSchedules / lateSends: documents started / PushLogs started after stopSeen.
\* timeouts: how many sends ended on their deadline.
\* result: the call's answer. refused: subscription state refused the call.
\* hint / replicatorHint: a receiver hint arrived / leaked into the subscription.
\* paused: the peer-wide receiver pause is in force; no task is admitted.
\* elapsed: ticks of one selection-wide pause clock, used only by the
\*   "shared-budget" bug.
\* headAge[d]: ticks the current head of d has spent since its first send.
\* startedHeads: documents whose current head has been sent and not yet
\*   acknowledged, so their deadline is running.
\* earlyExpiry: ghost truth that a head expired before spending its own budget.
\* hintedHeads: documents whose current head the receiver has paused at least
\*   once; only such a head is resent under its running deadline.
VARIABLES phase, task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
          lateSchedules, lateSends, timeouts, result, refused, hint, replicatorHint,
          paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads
\* Every variable, for steps that leave the rest unchanged.
vars == <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
          lateSchedules, lateSends, timeouts, result, refused, hint, replicatorHint,
          paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* Tasks holding a document slot: started and not yet ended.
InFlight == {d \in Docs : task[d] \in {"queued", "admitted", "sending"}}
\* Tasks holding a send slot: past the stop check or on the wire.
SendSlots == {d \in Docs : task[d] \in {"admitted", "sending"}}
\* Nothing has happened yet; every document is waiting to be scheduled.
Init == /\ phase = "idle" /\ task = [d \in Docs |-> "new"]
        \* No block is acknowledged and the scheduler holds no document.
        /\ blocksDone = [d \in Docs |-> 0] /\ checked = 0
        \* No stop, no failure, no cancellation, and the peer is answering.
        /\ stopped = FALSE /\ stopSeen = FALSE /\ cancelled = FALSE /\ silent = FALSE
        \* No late work, no timeouts, and no answer yet.
        /\ lateSchedules = 0 /\ lateSends = 0 /\ timeouts = 0 /\ result = "pending"
        \* Nothing refused the call and no hint has arrived.
        /\ refused = FALSE /\ hint = FALSE /\ replicatorHint = FALSE
        \* No pause is in force and the shared clock has not moved.
        /\ paused = FALSE /\ elapsed = 0
        \* No head has started its deadline or expired early.
        /\ headAge = [d \in Docs |-> 0] /\ startedHeads = {} /\ earlyExpiry = FALSE
        \* No head has been paused.
        /\ hintedHeads = {}
\* The call starts. Selected sends never consult the subscription's filter or
\* backoff; the "filter" and "backoff" bugs let that state refuse the call.
Start == /\ phase = "idle"
         \* Only the planted bugs let subscription state refuse the call.
         /\ refused' = (Bug \in {"filter", "backoff"})
         \* A refused call returns at once with an error and sends nothing.
         /\ phase' = IF refused' THEN "done" ELSE "running"
         \* Its answer is an error if refused, otherwise still pending.
         /\ result' = IF refused' THEN "err" ELSE "pending"
         \* Nothing else changes.
         /\ UNCHANGED <<task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                        lateSchedules, lateSends, timeouts, hint, replicatorHint, paused,
                        elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* A failure that proves the transfer cannot progress: the task raises the stop
\* flag before it releases its slots. The "stall" bug forgets to raise it.
RaiseStop == /\ stopSeen' = TRUE
             \* The flag goes up, unless the stall bug forgets it.
             /\ stopped' = (stopped \/ Bug # "stall")
\* The scheduler holds a document slot and reads the stop flag before starting
\* document d. The "schedule" bug skips the read.
Check(d) == /\ phase = "running" /\ checked = 0 /\ task[d] = "new"
            \* A document slot is free (the checked document holds one).
            /\ Cardinality(InFlight) < Cap
            \* The flag is down, unless the bug skips the read.
            /\ (~stopped \/ Bug = "schedule")
            \* The scheduler is now about to start d.
            /\ checked' = d
            \* Nothing else changes.
            /\ UNCHANGED <<phase, task, blocksDone, stopped, stopSeen, cancelled, silent,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint, paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* The checked document's task starts. A stop can land between the read and the
\* start (another thread raised it); that one document is counted as late.
Spawn == /\ checked # 0
         \* The task waits for a send slot.
         /\ task' = [task EXCEPT ![checked] = "queued"]
         \* It is late if a stop already happened.
         /\ lateSchedules' = lateSchedules + (IF stopSeen THEN 1 ELSE 0)
         \* The scheduler is free to check the next document.
         /\ checked' = 0
         \* Nothing else changes.
         /\ UNCHANGED <<phase, blocksDone, stopped, stopSeen, cancelled, silent,
                        lateSends, timeouts, result, refused, hint, replicatorHint, paused,
                        elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* A waiting task takes a send slot and reads the stop flag before its next
\* PushLog; this is also every block boundary. The "admission" bug skips it.
Admit(d) == /\ task[d] = "queued" /\ Cardinality(SendSlots) < Cap
            \* No task is admitted while the receiver's pause is in force.
            /\ ~paused
            \* The flag is down, unless the bug skips the read.
            /\ (~stopped \/ Bug = "admission")
            \* The task holds a send slot.
            /\ task' = [task EXCEPT ![d] = "admitted"]
            \* Nothing else changes.
            /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint, paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* A waiting task reads a raised flag and ends without sending.
Abandon(d) == /\ task[d] = "queued" /\ stopped /\ Bug # "admission"
              \* The task ends here.
              /\ task' = [task EXCEPT ![d] = "abandoned"]
              \* Nothing else changes.
              /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled,
                             silent, lateSchedules, lateSends, timeouts, result, refused,
                             hint, replicatorHint, paused, elapsed, headAge, startedHeads,
                             earlyExpiry, hintedHeads>>
\* An admitted task puts its PushLog on the wire; one starting after a stop is late.
Send(d) == /\ task[d] = "admitted"
           \* The PushLog is on the wire.
           /\ task' = [task EXCEPT ![d] = "sending"]
           \* It is late if a stop already happened.
           /\ lateSends' = lateSends + (IF stopSeen THEN 1 ELSE 0)
           \* The first send of a head starts its deadline; a resend keeps it running.
           /\ startedHeads' = startedHeads \cup {d}
           \* Nothing else changes.
           /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                          lateSchedules, timeouts, result, refused, hint, replicatorHint,
                          paused, elapsed, headAge, earlyExpiry, hintedHeads>>
\* The receiver acknowledges the block; the task finishes or waits for its next one.
Ack(d) == /\ task[d] = "sending" /\ ~silent
          \* One more of d's blocks is acknowledged.
          /\ blocksDone' = [blocksDone EXCEPT ![d] = @ + 1]
          \* The task finishes with its last block, otherwise queues for the next.
          /\ task' = [task EXCEPT ![d] = IF blocksDone'[d] = Blocks(d) THEN "acked" ELSE "queued"]
          \* The next head gets a fresh deadline.
          /\ headAge' = [headAge EXCEPT ![d] = 0]
          \* No deadline runs for d until its next head is sent.
          /\ startedHeads' = startedHeads \ {d}
          \* The next head has not been paused.
          /\ hintedHeads' = hintedHeads \ {d}
          \* Nothing else changes.
          /\ UNCHANGED <<phase, checked, stopped, stopSeen, cancelled, silent,
                         lateSchedules, lateSends, timeouts, result, refused, hint,
                         replicatorHint, paused, elapsed, earlyExpiry>>
\* The receiver refuses this head without a hint: only this document fails.
Refuse(d) == /\ task[d] = "sending" /\ ~silent
             \* The task fails alone.
             /\ task' = [task EXCEPT ![d] = "failed"]
             \* Nothing else changes.
             /\ UNCHANGED <<phase, blocksDone, checked, stopped, stopSeen, cancelled,
                            silent, lateSchedules, lateSends, timeouts, result, refused,
                            hint, replicatorHint, paused, elapsed, headAge, startedHeads,
                            earlyExpiry, hintedHeads>>
\* The send's own deadline passes: the peer is stalled, so the call stops.
Timeout(d) == /\ task[d] = "sending"
              \* The task fails.
              /\ task' = [task EXCEPT ![d] = "failed"]
              \* One more send ended on its deadline.
              /\ timeouts' = timeouts + 1
              \* The stop goes up before the task's slots are released.
              /\ RaiseStop
              \* Nothing else changes.
              /\ UNCHANGED <<phase, blocksDone, checked, cancelled, silent, lateSchedules,
                             lateSends, result, refused, hint, replicatorHint, paused,
                             elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* The connection drops under the send: the call stops.
Disconnect(d) == /\ task[d] = "sending"
                 \* The task fails.
                 /\ task' = [task EXCEPT ![d] = "failed"]
                 \* The stop goes up before the task's slots are released.
                 /\ RaiseStop
                 \* Nothing else changes.
                 /\ UNCHANGED <<phase, blocksDone, checked, cancelled, silent,
                                lateSchedules, lateSends, timeouts, result, refused, hint,
                                replicatorHint, paused, elapsed, headAge, startedHeads,
                                earlyExpiry, hintedHeads>>
\* A pause keeps the head pending and holds all new sends until Resume.
\* The pause-stop bug mistakes temporary capacity for a terminal failure.
Hint(d) == /\ task[d] = "sending" /\ ~silent
           \* The same head waits to be sent again.
           /\ task' = [task EXCEPT ![d] = "queued"]
           \* A hint arrived; the hint bug also writes it into the subscription.
           /\ hint' = TRUE /\ replicatorHint' = (replicatorHint \/ Bug = "hint")
           \* The pause holds every task of the call.
           /\ paused' = TRUE
           \* Only the pause-stop bug raises the flag on a hint.
           /\ stopped' = (stopped \/ Bug = "pause-stop")
           \* This head has now been paused.
           /\ hintedHeads' = hintedHeads \cup {d}
           \* Nothing else changes; the head's deadline keeps running.
           /\ UNCHANGED <<phase, blocksDone, checked, stopSeen, cancelled, silent,
                          lateSchedules, lateSends, timeouts, result, refused, elapsed,
                          headAge, startedHeads, earlyExpiry>>
\* A shared peer pause resumes only while each paused head has budget left.
Resume == /\ phase = "running" /\ paused /\ ~stopped
          \* Every head still waiting has deadline left.
          /\ \A d \in startedHeads : headAge[d] < Budget
          \* The pause is over.
          /\ paused' = FALSE
          \* Nothing else changes.
          /\ UNCHANGED <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled,
                         silent, lateSchedules, lateSends, timeouts, result, refused,
                         hint, replicatorHint, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* Each head spends its allowance from its first actual send; ACK advances it. The old shared timer is
\* retained only to execute the negative control for a selection-wide budget.
Tick == /\ phase = "running"
        \* Time passes only while some clock still has room to move.
        /\ ((hint /\ elapsed < Budget) \/ \E d \in startedHeads : headAge[d] < Budget)
        \* The shared clock moves once a hint arrived, up to its budget.
        /\ elapsed' = IF hint /\ elapsed < Budget THEN elapsed + 1 ELSE elapsed
        \* Every started head that still has budget spends one more tick.
        /\ headAge' = [d \in Docs |-> IF d \in startedHeads /\ headAge[d] < Budget
                                       THEN headAge[d] + 1 ELSE headAge[d]]
        \* Nothing else changes.
        /\ UNCHANGED <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled,
                       silent, lateSchedules, lateSends, timeouts, result, refused,
                       hint, replicatorHint, paused, startedHeads, earlyExpiry, hintedHeads>>
\* A head the receiver has paused runs out of deadline, while it waits out a
\* pause or while it is resent: it is deferred and fails alone, leaving the
\* rest of the selection to be tried. Its deadline ends with it. (A head never
\* paused has no deadline of its own on the wire: its send deadline is Timeout.)
\* The "expiry-stop" bug also raises the call's stop flag, so one head's
\* pauses end the selection. The "shared-budget" bug expires a head on the
\* selection-wide clock.
PauseExpired(d) == /\ phase = "running" /\ d \in startedHeads /\ d \in hintedHeads
                   \* The head is still working: waiting, admitted or on the wire.
                   /\ task[d] \in {"queued", "admitted", "sending"}
                   \* The bug uses the shared clock; the fix uses the head's own.
                   /\ IF Bug = "shared-budget" THEN elapsed = Budget ELSE headAge[d] = Budget
                   \* An expiry before the head spent its own budget is early.
                   /\ earlyExpiry' = (earlyExpiry \/ headAge[d] < Budget)
                   \* The head fails alone.
                   /\ task' = [task EXCEPT ![d] = "failed"]
                   \* Its deadline no longer runs, and it is no longer paused.
                   /\ startedHeads' = startedHeads \ {d} /\ hintedHeads' = hintedHeads \ {d}
                   \* Only the expiry-stop bug raises the stop flag.
                   /\ stopped' = (stopped \/ Bug = "expiry-stop")
                   \* Nothing else changes; a deferred head is not a hard failure.
                   /\ UNCHANGED <<phase, blocksDone, checked, stopSeen, cancelled, silent,
                                  lateSchedules, lateSends, timeouts, result, refused,
                                  hint, replicatorHint, paused, elapsed, headAge>>
\* From now on the peer answers nothing; every send on the wire times out.
GoSilent == /\ phase = "running" /\ ~silent /\ silent' = TRUE
            \* Nothing else changes.
            /\ UNCHANGED <<phase, task, blocksDone, checked, stopped, stopSeen, cancelled,
                           lateSchedules, lateSends, timeouts, result, refused, hint,
                           replicatorHint, paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* The caller drops the call; its drop guard raises the stop flag. The "cancel"
\* bug drops it without raising the flag, so the work it left runs on.
Cancel == /\ phase = "running" /\ ~cancelled
          \* The caller is gone, which counts as a stop.
          /\ cancelled' = TRUE /\ stopSeen' = TRUE
          \* The guard raises the flag, unless the cancel bug is planted.
          /\ stopped' = (stopped \/ Bug # "cancel")
          \* Nothing else changes.
          /\ UNCHANGED <<phase, task, blocksDone, checked, silent, lateSchedules, lateSends,
                         timeouts, result, refused, hint, replicatorHint, paused, elapsed,
                         headAge, startedHeads, earlyExpiry, hintedHeads>>
\* Scheduling has ended: the flag stopped it, or every document was started.
SchedulingOver == checked = 0 /\ (stopped \/ \A d \in Docs : task[d] # "new")
\* The call returns once scheduling is over and every started task is joined.
\* The "drain" bug returns at the first failure with tasks still running.
\* Success needs every chosen document acknowledged; the "partial" bug only asks
\* that nothing failed, so unscheduled or abandoned documents pass as success.
Finish == /\ phase = "running" /\ SchedulingOver
          \* Every started task is joined, unless the drain bug returns early.
          /\ (InFlight = {} \/ (Bug = "drain" /\ \E d \in Docs : task[d] = "failed"))
          \* The call has returned.
          /\ phase' = "done"
          \* Its answer: success only if every document is acknowledged (the partial
          \* bug asks only that none failed).
          /\ result' = IF IF Bug = "partial" THEN \A d \in Docs : task[d] # "failed"
                                             ELSE \A d \in Docs : task[d] = "acked"
                       THEN "ok" ELSE "err"
          \* Nothing else changes.
          /\ UNCHANGED <<task, blocksDone, checked, stopped, stopSeen, cancelled, silent,
                         lateSchedules, lateSends, timeouts, refused, hint, replicatorHint,
                         paused, elapsed, headAge, startedHeads, earlyExpiry, hintedHeads>>
\* Any of the steps above, in any order.
Next == \/ Start \/ Spawn \/ GoSilent \/ Cancel \/ Finish \/ Resume \/ Tick
        \* Or a step by one document's task.
        \/ \E d \in Docs : \/ Check(d) \/ Admit(d) \/ Abandon(d) \/ Send(d) \/ Ack(d)
                           \/ Refuse(d) \/ Timeout(d) \/ Disconnect(d) \/ Hint(d)
                           \/ PauseExpired(d)
\* The whole behaviour: start from Init and take any Next step.
Spec == Init /\ [][Next]_vars
\* Every variable keeps a value of its intended kind.
TypeOK == /\ phase \in {"idle", "running", "done"}
          \* Each task is in one of its seven states.
          /\ task \in [Docs -> {"new", "queued", "admitted", "sending", "acked", "failed",
                                "abandoned"}]
          \* Block counts and the checked document stay in range.
          /\ blocksDone \in [Docs -> 0..2] /\ checked \in 0..N
          \* The answer is pending, success or error.
          /\ result \in {"pending", "ok", "err"}
          \* The pause flag and shared clock stay in range.
          /\ paused \in BOOLEAN /\ elapsed \in 0..Budget
          \* Head ages stay within the budget; started heads are documents.
          /\ headAge \in [Docs -> 0..Budget] /\ startedHeads \subseteq Docs /\ earlyExpiry \in BOOLEAN
          \* Paused heads are documents.
          /\ hintedHeads \subseteq Docs
\* Subscription filters and backoff never refuse an explicit selection.
INV_Independent == ~refused
\* A receiver hint for this call never changes the subscription's schedule.
INV_HintOwner == ~replicatorHint
\* No head expires before it has spent its own full budget.
INV_FreshHeadBudget == ~earlyExpiry
\* A hint alone cannot manufacture a terminal stop.
INV_PauseNotTerminal == (hint /\ ~stopSeen) => ~stopped
\* Unless the peer stalled, the connection dropped or the caller cancelled,
\* the call tries every chosen document before it returns.
INV_EveryDocTried == (phase = "done" /\ ~refused /\ ~stopSeen) => \A d \in Docs : task[d] # "new"
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
