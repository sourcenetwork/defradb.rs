---- MODULE CoordinatorShutdown ----
\* The story, in plain words.
\*
\* A P2P sync coordinator runs background tasks: a replication push, a DAG
\* fetch, a follow-up broadcast. Each task is spawned, and its handle is kept
\* in the coordinator's tracked set, so that shutdown can wait for it or stop
\* it. The task's own future holds the coordinator (an Arc), and the
\* coordinator holds the task's handle: a loop that nothing else breaks.
\* Under nuclei a parked task whose waker is held elsewhere is not dropped
\* when the runtime shuts down, so a task that nobody joins or aborts keeps
\* the coordinator alive for good, and with it the database and the lock on
\* the store.
\*
\* Shutting the coordinator down is what breaks the loop
\* (SyncShutdownHandle::shutdown):
\*   1. each caller takes a copy of the shared completion receiver;
\*   2. the first caller to set the shutdown flag wins. It takes the only
\*      completion sender and builds the drain, which owns that sender;
\*   3. it hands the drain to the network edge (try_spawn). The edge either
\*      runs it as a task of its own, or, when it no longer accepts work,
\*      hands it back unstarted, and then the winner runs it in place;
\*   4. the drain (drain_background_tasks) clears the DAG fetch reservations,
\*      waits until no pruner is at work, takes every handle out of the
\*      tracked set, and joins them one by one under one shared deadline.
\*      When the deadline passes it aborts every task still running and
\*      joins them under a short abort timeout. Then it sends the completion;
\*   5. every caller waits for the completion: Ok when the drain sent it, an
\*      error (logged as a warning) when the sender was dropped unsent,
\*      because the drain was cancelled or panicked.
\* Meanwhile a late spawn may race the shutdown (spawn_task, spawn_here): it
\* reads the flag, prunes the set (takes each handle out, drops the finished
\* ones, puts the live ones back), spawns its task, puts the new handle in
\* the set, then reads the flag again and aborts its own task when shutdown
\* has begun.
\*
\* A tiny example. The edge has closed when shutdown is called. The winner's
\* try_spawn hands the drain back, so the winner runs it itself: it takes the
\* two handles; one task ends, the other hangs; the deadline passes, the hung
\* task is aborted, and the drain sends the completion. Both callers return
\* Ok. Had the refused drain been dropped (the old code), its sender would
\* have dropped with it: both callers would have returned with an error, and
\* the two tasks, never joined nor aborted, would have kept the coordinator
\* alive forever.
\*
\* What must never happen:
\*   1. the drain is over (it returned, or the code dropped it) while a live
\*      task, never joined nor aborted, is still in the tracked set
\*      - INV_NoLeak;
\*   2. two drains are started - INV_OneDrain;
\*   3. a caller returns an error although nothing cancelled the drain
\*      - INV_ErrMeansCancelled;
\*   4. a caller of shutdown waits forever - LIVE_CallersReturn.
\*
\* Correspondence with the Rust code (crates/p2p/src/sync/coordinator/mod.rs
\* unless named):
\*
\*   | Model             | Rust                                                     |
\*   |-------------------|----------------------------------------------------------|
\*   | flag              | SyncShutdownState::is_shutting_down                      |
\*   | Call              | SyncShutdownHandle::shutdown: shutdown_complete.clone(), |
\*   |                   | then begin_shutdown's is_shutting_down.swap(true)        |
\*   | snd               | the one oneshot::Sender: "store" while it sits in        |
\*   |                   | shutdown_complete_tx, "drain" once the winner popped it  |
\*   |                   | and moved it into the drain future                       |
\*   | Spawn             | shutdown: `if let Err(drain) = self.network()            |
\*   |                   | .try_spawn(drain) { drain.await }`; Network::try_spawn   |
\*   |                   | (crates/p2p/src/network.rs) calls Edge::try_spawn        |
\*   |                   | (crates/edge/src/handle.rs), which hands the future back |
\*   |                   | unstarted when admit() finds the gate closed             |
\*   | edgeOpen          | the edge's gate admits work (Edge::is_closed); EdgeClose |
\*   |                   | is Scope::close, Scope::drain or the runtime's shutdown  |
\*   | dw                | the drain as an edge task ("edge"), or awaited in place  |
\*   |                   | by the winner ("here")                                   |
\*   | DrainCheck        | drain_background_tasks: pending_dag_fetch_tasks.store,   |
\*   |                   | fence, `while pruning.load() != 0 { yield_now().await }` |
\*   | DrainPop          | `while let Some(task) = background_tasks.pop()`          |
\*   | DrainJoin         | one handle joined under `timeout(deadline - now, ..)`    |
\*   | DrainDeadline     | `if result.is_err()`: abort.abort() and every            |
\*   |                   | pending.abort()                                          |
\*   | AbortTimeout      | timeout(BACKGROUND_TASK_ABORT_TIMEOUT, join) elapses:    |
\*   |                   | the handles left drop with `join`                        |
\*   | DrainReturn       | `return`, then the drain future's `sender.send(())`      |
\*   | Await             | `complete.await`: Ok, or Err and tracing::warn!          |
\*   | CancelCaller      | a caller's shutdown future dropped at `complete.await`   |
\*   | Teardown          | the edge runtime dropped before the drain task finished  |
\*   | CancelWinner      | the winner's shutdown future dropped at `drain.await`    |
\*   | life, loc         | a TrackedTask (crates/p2p/src/tracked_task.rs): whether  |
\*   |                   | its future still runs (MarkFinished sets `finished` when |
\*   |                   | it drops), and where its handle is                       |
\*   | RegCheck          | spawn_task: `if self.is_shutting_down() { return false }`|
\*   | PruneEnter ..     | prune_finished_tasks: pruning.fetch_add, the loop over   |
\*   | PruneExit         | background_tasks.len() that pops and re-pushes the live  |
\*   |                   | handles, pruning.fetch_sub                               |
\*   | RegSpawn          | spawn_here: TrackedTask::spawn                           |
\*   | RegPush,          | register_task: background_tasks.push, fence, re-read of  |
\*   | RegRecheck        | is_shutting_down, abort.abort()                          |
\*   | SwapFlag          | begin_shutdown's swap (RED: a load, then a store, so two |
\*   |                   | callers may both win)                                    |
\*   | AtomicSpawn       | one try_spawn that hands a refused future back (RED: the |
\*   |                   | winner reads is_closed(), then spawns)                   |
\*   | RunRefused        | `drain.await` on Err (RED: the old network().spawn(drain)|
\*   |                   | which dropped a refused drain)                           |
\*   | DrainOwnsSender   | the sender popped by the winner and moved into the drain |
\*   |                   | (RED: left in the shared state and sent at the drain's   |
\*   |                   | end)                                                     |
\*   | WaitForPruners    | the `while pruning != 0` wait (RED: none)                |
\*   | PrunerChecksFlag  | NOT IN THE CODE when this model was written:             |
\*   |                   | prune_finished_tasks reads is_shutting_down right after  |
\*   |                   | its fetch_add and leaves the set alone once it is set    |
\*   |                   | (RED: the code as it stands, which never reads it)       |
\*   | RecheckAfterPush  | register_task's re-read and abort (RED: none)            |
\*   | AbortAtDeadline   | the shared deadline and the abort path (RED: each handle |
\*   |                   | joined with no deadline)                                 |
\*
\* Gaps, explained:
\*   * One step of the model is one atomic step of the code. The flag, the
\*     pruning count and the set are sequentially consistent: the SeqCst
\*     swap, fetch_add and loads and the two fences make both handshakes
\*     (the drain against a pruner, the drain against a register_task) work
\*     like this.
\*   * The drain takes the whole set in one step; the code pops handles one
\*     by one. A pruner that pops while those pops run is one that started
\*     after the drain's check, and with PrunerChecksFlag it leaves the set
\*     alone, so the one step loses no interleaving that matters.
\*   * The drain joins the handles in any order; the code joins them in the
\*     order it popped them. Joining an ended task out of turn only brings
\*     the drain's end sooner; a task that hangs holds both until the
\*     deadline.
\*   * There is no clock. The deadline and the abort timeout may pass at any
\*     moment of their waits, and fairness says they do pass.
\*   * Clearing the DAG fetch reservations is not modeled: they hold abort
\*     handles, not the tasks' futures, and take no part in the loop.
\*   * A drain cancelled from outside (Teardown, CancelWinner) makes no leak
\*     promise. The handles it already took drop with it, and nuclei cancels
\*     a task whose handle drops; the handles still in the set (the drain was
\*     cancelled before its first poll, or while it waited for a pruner)
\*     stay there, with their loop. Callers still return, with an error. A
\*     panic in the drain is the same as its cancellation.
\*   * An aborted task stops at its next poll, which may never come; the
\*     abort timeout ends the drain all the same.
\*   * A late task is spawned by one late spawn; a pruner holds one handle
\*     at a time, as the code does.

EXTENDS Naturals, FiniteSets
\* Naturals gives numbers; FiniteSets counts the handles in the set.

CONSTANTS Callers, Tasks, Late, CanKill,
          SwapFlag, AtomicSpawn, RunRefused, DrainOwnsSender,
          WaitForPruners, PrunerChecksFlag, RecheckAfterPush, AbortAtDeadline
\* Callers are the callers of shutdown. Tasks are the tracked background
\* tasks: those in Late come from a late spawn that races the shutdown, the
\* others are in the tracked set from the start. CanKill says whether
\* something outside the drain may cancel it (the runtime's teardown, or the
\* cancellation of the caller that runs it in place). Every other switch is a
\* guard: TRUE in the design, and each RED configuration turns one off.

ASSUME /\ Callers # {}
       \* someone calls shutdown;
       /\ Late \subseteq Tasks
       \* a late task is a task;
       /\ "none" \notin Tasks
       \* "none" is free to mean "no task";
       /\ \A s \in {CanKill, SwapFlag, AtomicSpawn, RunRefused, DrainOwnsSender,
                    WaitForPruners, PrunerChecksFlag, RecheckAfterPush,
                    AbortAtDeadline} : s \in BOOLEAN
       \* and every switch is on or off.

VARIABLES flag, edgeOpen, cpc, drains, dr, dw, snd,
          life, loc, rpc, held, budget, pruning
\* flag: shutdown has begun. edgeOpen: the edge still accepts work. cpc:
\* where each caller is. drains: how many drains were built. dr: how far the
\* drain got. dw: where it runs. snd: where the completion sender is. life:
\* each task's future (running, ended, aborted). loc: where each task's
\* handle is. rpc: where each late spawn is. held: the handle its pruner has
\* taken out. budget: how many more handles its pruner may take. pruning:
\* how many pruners are at work.

vars == <<flag, edgeOpen, cpc, drains, dr, dw, snd,
          life, loc, rpc, held, budget, pruning>>
\* Every variable, for the stuttering steps.

Ended == {"finished", "stopped"}
\* A task whose future is gone: it ended, or was dropped after an abort.

Busy(t) == \/ \E r \in Late : held[r] = t
           \* a pruner has t's handle out of the set,
           \/ t \in Late /\ rpc[t] \in {"push", "recheck"}
           \* or t's spawn has not yet pushed it, or not yet re-read the flag.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ flag \in BOOLEAN /\ edgeOpen \in BOOLEAN
  \* the flag and the edge are on or off;
  /\ cpc \in [Callers -> {"idle", "read", "won", "check", "here", "wait", "ok", "err", "gone"}]
  \* each caller has not called, read the flag (RED), won, checked the edge
  \* (RED), runs the drain, waits, returned Ok, returned an error, or left;
  /\ drains \in 0..Cardinality(Callers)
  \* at most one drain per caller was built;
  /\ dr \in {"none", "built", "start", "pop", "join", "abort", "done", "dropped", "killed"}
  \* the drain is not built, built, about to be polled, about to take the
  \* set, joining, joining what it aborted, returned, dropped unstarted (RED),
  \* or cancelled from outside;
  /\ dw \in {"none", "edge", "here"}
  \* it runs nowhere yet, on the edge, or in the winner;
  /\ snd \in {"store", "drain", "sent", "closed"}
  \* the sender is in the shared state, in the drain, sent, or dropped unsent;
  /\ life \in [Tasks -> {"unspawned", "live", "finished", "aborting", "stopped"}]
  \* each task is not spawned yet, running, ended, aborted but not yet
  \* dropped, or dropped after its abort;
  /\ loc \in [Tasks -> {"none", "hand", "queue", "pruner", "drain", "gone"}]
  \* each handle is nowhere yet, in its spawner's hand, in the set, out with
  \* a pruner, in the drain, or dropped;
  /\ rpc \in [Late -> {"idle", "enter", "look", "pop", "back", "pexit", "spawn", "push", "recheck", "over"}]
  \* each late spawn is at one of its steps;
  /\ held \in [Late -> Tasks \cup {"none"}]
  \* its pruner has one handle out, or none;
  /\ budget \in [Late -> 0..Cardinality(Tasks)]
  \* it may take at most as many handles as there are;
  /\ pruning \in 0..Cardinality(Late)
  \* and at most every late spawn prunes at once.

Init ==
  \* Before shutdown:
  /\ flag = FALSE
  \* shutdown has not begun;
  /\ edgeOpen = TRUE
  \* the edge accepts work;
  /\ cpc = [c \in Callers |-> "idle"]
  \* nobody called shutdown;
  /\ drains = 0 /\ dr = "none" /\ dw = "none"
  \* there is no drain;
  /\ snd = "store"
  \* the sender sits in the shared state;
  /\ life = [t \in Tasks |-> IF t \in Late THEN "unspawned" ELSE "live"]
  \* the early tasks run, the late ones are not spawned;
  /\ loc = [t \in Tasks |-> IF t \in Late THEN "none" ELSE "queue"]
  \* the early tasks' handles are in the set;
  /\ rpc = [r \in Late |-> "idle"] /\ held = [r \in Late |-> "none"]
  \* no late spawn has begun, and no pruner has a handle out;
  /\ budget = [r \in Late |-> 0] /\ pruning = 0
  \* and no pruner is at work.

Build(c) ==
  \* Caller c won: the flag is set, and it builds the drain.
  /\ flag' = TRUE
  \* shutdown has begun;
  /\ drains' = drains + 1
  \* one more drain was built;
  /\ cpc' = [cpc EXCEPT ![c] = IF dr = "none" THEN "won" ELSE "wait"]
  \* the winner goes on to start it (a second winner, possible only in a
  \* RED, is counted and followed no further);
  /\ dr' = IF dr = "none" THEN "built" ELSE dr
  \* the drain exists, not started;
  /\ snd' = IF DrainOwnsSender /\ snd = "store" THEN "drain" ELSE snd
  \* and the winner pops the only sender into it (RED: leaves it in the
  \* shared state).

Call(c) ==
  \* Caller c calls shutdown: it copies the completion receiver, then tries
  \* to set the flag.
  /\ cpc[c] = "idle"
  \* it has not called yet;
  /\ IF ~SwapFlag
     \* RED: the flag is only read now, and set in a later step.
     THEN /\ cpc' = [cpc EXCEPT ![c] = IF flag THEN "wait" ELSE "read"]
          \* set already, it waits; clear, it will set it;
          /\ UNCHANGED <<flag, drains, dr, snd>>
          \* and nothing else changes yet.
     ELSE IF flag
     \* The swap finds the flag set: another caller won.
     THEN /\ cpc' = [cpc EXCEPT ![c] = "wait"]
          \* this one goes to wait for the completion;
          /\ UNCHANGED <<flag, drains, dr, snd>>
          \* and nothing else changes.
     \* The swap finds it clear: this caller wins.
     ELSE Build(c)
  /\ UNCHANGED <<edgeOpen, dw, life, loc, rpc, held, budget, pruning>>
  \* The edge, the tasks and the late spawns are untouched.

SetFlag(c) ==
  \* RED only: a caller that read the flag clear now sets it, whatever
  \* happened since.
  /\ cpc[c] = "read"
  \* it read the flag clear;
  /\ Build(c)
  \* it sets the flag and builds a drain;
  /\ UNCHANGED <<edgeOpen, dw, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

Start(c, w) ==
  \* The drain starts at w: on the edge, or here, in the winner c.
  /\ dr' = "start"
  \* it is about to be polled for the first time;
  /\ dw' = w
  \* it runs at w;
  /\ cpc' = [cpc EXCEPT ![c] = IF w = "here" THEN "here" ELSE "wait"]
  \* the winner runs it in place, or goes to wait for its completion;
  /\ UNCHANGED snd
  \* and the sender stays where it is.

Drop(c) ==
  \* RED only: the refused drain is dropped unstarted.
  /\ dr' = "dropped"
  \* the drain is gone, never polled;
  /\ snd' = IF snd = "drain" THEN "closed" ELSE snd
  \* the sender it owned drops with it;
  /\ cpc' = [cpc EXCEPT ![c] = "wait"]
  \* the winner goes to wait for the completion;
  /\ UNCHANGED dw
  \* and the drain runs nowhere.

Spawn(c) ==
  \* The winner hands the drain to the edge.
  /\ cpc[c] = "won"
  \* it built the drain;
  /\ IF ~AtomicSpawn
     \* RED: it asks the edge whether it is closed, and spawns later.
     THEN IF edgeOpen
          \* The edge reads open: the spawn comes in a later step.
          THEN /\ cpc' = [cpc EXCEPT ![c] = "check"]
               \* the winner is between its check and its spawn;
               /\ UNCHANGED <<dr, dw, snd>>
               \* and the drain waits for it.
          \* The edge reads closed: the winner runs the drain itself.
          ELSE Start(c, "here")
     \* try_spawn: the edge admits the drain as a task of its own;
     ELSE IF edgeOpen THEN Start(c, "edge")
     \* or refuses it and hands it back: it runs in place (RED: dropped).
     ELSE IF RunRefused THEN Start(c, "here") ELSE Drop(c)
  /\ UNCHANGED <<flag, edgeOpen, drains, life, loc, rpc, held, budget, pruning>>
  \* The flag, the edge, the tasks and the late spawns are untouched.

SpawnAfterCheck(c) ==
  \* RED only: the winner that saw the edge open spawns the drain now.
  /\ cpc[c] = "check"
  \* it checked the edge;
  /\ IF edgeOpen THEN Start(c, "edge") ELSE Drop(c)
  \* the edge admits it, or, closed in between, refuses it, and the spawn
  \* drops it;
  /\ UNCHANGED <<flag, edgeOpen, drains, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

InPlaceDone(c) ==
  \* The drain the winner ran in place returned: the winner goes to wait.
  /\ cpc[c] = "here"
  \* the winner runs the drain;
  /\ dr = "done"
  \* the drain returned;
  /\ cpc' = [cpc EXCEPT ![c] = "wait"]
  \* the winner awaits the completion like everyone;
  /\ UNCHANGED <<flag, edgeOpen, drains, dr, dw, snd, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

Await(c) ==
  \* A waiting caller's completion resolves.
  /\ cpc[c] = "wait"
  \* the caller waits;
  /\ snd \in {"sent", "closed"}
  \* the sender sent, or dropped unsent;
  /\ cpc' = [cpc EXCEPT ![c] = IF snd = "sent" THEN "ok" ELSE "err"]
  \* the caller returns Ok, or logs the error and returns;
  /\ UNCHANGED <<flag, edgeOpen, drains, dr, dw, snd, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

CancelCaller(c) ==
  \* A waiting caller is cancelled: it leaves. A drain on the edge runs on.
  /\ cpc[c] = "wait"
  \* the caller waits;
  /\ cpc' = [cpc EXCEPT ![c] = "gone"]
  \* its shutdown future is dropped;
  /\ UNCHANGED <<flag, edgeOpen, drains, dr, dw, snd, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

EdgeClose ==
  \* The edge stops accepting work, at any moment.
  /\ edgeOpen
  \* it accepted work;
  /\ edgeOpen' = FALSE
  \* now it refuses it; what it admitted runs on;
  /\ UNCHANGED <<flag, cpc, drains, dr, dw, snd, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

DrainCheck ==
  \* The drain's first poll: it clears the fetch reservations, then waits
  \* until no pruner is at work (RED: does not wait).
  /\ dr = "start"
  \* the drain started;
  /\ pruning = 0 \/ ~WaitForPruners
  \* no pruner has handles out (RED: whatever pruners do);
  /\ dr' = "pop"
  \* it goes on to take the set;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dw, snd, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

DrainPop ==
  \* The drain takes every handle out of the set.
  /\ dr = "pop"
  \* it checked the pruners;
  /\ loc' = [t \in Tasks |-> IF loc[t] = "queue" THEN "drain" ELSE loc[t]]
  \* every handle in the set is now the drain's;
  /\ dr' = "join"
  \* and it starts joining them under the deadline;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dw, snd, life, rpc, held, budget, pruning>>
  \* and nothing else changes.

DrainJoin(t) ==
  \* The drain joins one of its handles, whose task has ended.
  /\ dr \in {"join", "abort"}
  \* it is joining, before or after the deadline;
  /\ loc[t] = "drain"
  \* it holds t's handle;
  /\ life[t] \in Ended
  \* t's future is gone;
  /\ loc' = [loc EXCEPT ![t] = "gone"]
  \* the join returns and the handle is used up;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, rpc, held, budget, pruning>>
  \* and nothing else changes.

DrainDeadline ==
  \* The shared deadline passes with handles left: every task still running
  \* is aborted (RED: there is no deadline).
  /\ dr = "join"
  \* the drain is joining;
  /\ AbortAtDeadline
  \* it joins under a deadline;
  /\ \E t \in Tasks : loc[t] = "drain"
  \* some handle is left;
  /\ life' = [t \in Tasks |-> IF loc[t] = "drain" /\ life[t] = "live" THEN "aborting" ELSE life[t]]
  \* each task it still holds that runs is aborted;
  /\ dr' = "abort"
  \* and it joins them under the abort timeout;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dw, snd, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

AbortTimeout ==
  \* The abort timeout passes with handles left: the drain lets them go.
  /\ dr = "abort"
  \* it joins what it aborted;
  /\ \E t \in Tasks : loc[t] = "drain"
  \* some handle is left;
  /\ loc' = [t \in Tasks |-> IF loc[t] = "drain" THEN "gone" ELSE loc[t]]
  \* every handle left drops (their tasks are aborted already);
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, rpc, held, budget, pruning>>
  \* and nothing else changes.

DrainReturn ==
  \* The drain holds no handle any more: it returns and sends the completion.
  /\ dr \in {"join", "abort"}
  \* it is joining;
  /\ \A t \in Tasks : loc[t] # "drain"
  \* every handle it took was joined or let go;
  /\ dr' = "done"
  \* it returned;
  /\ snd' = IF snd \in {"drain", "store"} THEN "sent" ELSE snd
  \* and the sender sends (RED: the drain pops it from the shared state now);
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dw, life, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

Cancelled ==
  \* What a drain cancelled from outside leaves behind.
  /\ dr' = "killed"
  \* the drain is gone;
  /\ loc' = [t \in Tasks |-> IF loc[t] = "drain" THEN "gone" ELSE loc[t]]
  \* the handles it took drop with it;
  /\ life' = [t \in Tasks |-> IF loc[t] = "drain" /\ life[t] = "live" THEN "aborting" ELSE life[t]]
  \* and nuclei cancels each task whose handle dropped;
  /\ snd' = IF snd = "drain" THEN "closed" ELSE snd
  \* the sender it owned drops unsent (RED: one it did not own stays).

Teardown ==
  \* The runtime is torn down while the drain runs on the edge, before its
  \* first poll or at one of its waits.
  /\ CanKill
  \* something may cancel the drain;
  /\ dw = "edge"
  \* the drain is an edge task;
  /\ dr \in {"start", "join", "abort"}
  \* it waits to be polled, for a pruner, or for a task;
  /\ Cancelled
  \* it is cancelled;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dw, rpc, held, budget, pruning>>
  \* and nothing else changes.

CancelWinner(c) ==
  \* The winner running the drain in place is cancelled at one of the
  \* drain's waits, and the drain with it.
  /\ CanKill
  \* something may cancel the drain;
  /\ cpc[c] = "here"
  \* c runs the drain;
  /\ \/ dr \in {"join", "abort"}
     \* the drain waits for a task,
     \/ dr = "start" /\ pruning > 0
     \* or yields for a pruner;
  /\ Cancelled
  \* the drain is cancelled;
  /\ cpc' = [cpc EXCEPT ![c] = "gone"]
  \* and the winner leaves;
  /\ UNCHANGED <<flag, edgeOpen, drains, dw, rpc, held, budget, pruning>>
  \* and nothing else changes.

Finish(t) ==
  \* A running task ends by itself. It may also never end.
  /\ life[t] = "live"
  \* t runs;
  /\ life' = [life EXCEPT ![t] = "finished"]
  \* its future returned and dropped;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

Stop(t) ==
  \* An aborted task is dropped by the runtime at its next poll.
  /\ life[t] = "aborting"
  \* t was aborted;
  /\ life' = [life EXCEPT ![t] = "stopped"]
  \* its future is dropped;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, loc, rpc, held, budget, pruning>>
  \* and nothing else changes.

RegCheck(r) ==
  \* Late spawn r reads the flag before anything else.
  /\ rpc[r] = "idle"
  \* it has not begun;
  /\ rpc' = [rpc EXCEPT ![r] = IF flag THEN "over" ELSE "enter"]
  \* shutdown has begun: it spawns nothing; otherwise it goes on to prune;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, loc, held, budget, pruning>>
  \* and nothing else changes.

PruneEnter(r) ==
  \* r's pruner says it is at work.
  /\ rpc[r] = "enter"
  \* r passed its check;
  /\ pruning' = pruning + 1
  \* one more pruner at work;
  /\ rpc' = [rpc EXCEPT ![r] = "look"]
  \* it goes on to look at the set;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, loc, held, budget>>
  \* and nothing else changes.

PruneLook(r) ==
  \* r's pruner reads the flag (RED: it does not) and the size of the set.
  /\ rpc[r] = "look"
  \* it said it is at work;
  /\ IF PrunerChecksFlag /\ flag
     \* Shutdown has begun: the drain may already have taken the set.
     THEN /\ rpc' = [rpc EXCEPT ![r] = "pexit"]
          \* it leaves the set alone;
          /\ UNCHANGED budget
          \* and takes nothing.
     \* Otherwise it will take as many handles as the set holds now.
     ELSE /\ budget' = [budget EXCEPT ![r] = Cardinality({t \in Tasks : loc[t] = "queue"})]
          \* the set's length;
          /\ rpc' = [rpc EXCEPT ![r] = "pop"]
          \* and it starts taking handles.
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, loc, held, pruning>>
  \* Nothing else changes.

PrunePop(r) ==
  \* r's pruner takes the next handle out of the set, or stops.
  /\ rpc[r] = "pop"
  \* it is pruning;
  /\ IF budget[r] > 0 /\ \E t \in Tasks : loc[t] = "queue"
     \* There are handles left to take and the set is not empty.
     THEN \E t \in Tasks :
            /\ loc[t] = "queue"
            \* a handle in the set;
            /\ loc' = [loc EXCEPT ![t] = "pruner"]
            \* is out with the pruner;
            /\ held' = [held EXCEPT ![r] = t]
            \* which holds it;
            /\ budget' = [budget EXCEPT ![r] = @ - 1]
            \* one fewer to take;
            /\ rpc' = [rpc EXCEPT ![r] = "back"]
            \* and it goes on to look at it.
     \* None left: the pruner stops.
     ELSE /\ rpc' = [rpc EXCEPT ![r] = "pexit"]
          \* it goes on to say it is done;
          /\ UNCHANGED <<loc, held, budget>>
          \* and takes nothing.
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, pruning>>
  \* Nothing else changes.

PruneBack(r) ==
  \* r's pruner drops the handle it holds if its task's future is gone, and
  \* otherwise puts it back in the set.
  /\ rpc[r] = "back"
  \* it holds a handle;
  /\ loc' = [loc EXCEPT ![held[r]] = IF life[held[r]] \in Ended THEN "gone" ELSE "queue"]
  \* dropped when finished, back in the set when not;
  /\ held' = [held EXCEPT ![r] = "none"]
  \* the pruner holds nothing;
  /\ rpc' = [rpc EXCEPT ![r] = "pop"]
  \* and it goes on to the next;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, budget, pruning>>
  \* and nothing else changes.

PruneExit(r) ==
  \* r's pruner says it is done.
  /\ rpc[r] = "pexit"
  \* it stopped taking handles;
  /\ pruning' = pruning - 1
  \* one fewer pruner at work;
  /\ rpc' = [rpc EXCEPT ![r] = "spawn"]
  \* and r goes on to spawn its task;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, loc, held, budget>>
  \* and nothing else changes.

RegSpawn(r) ==
  \* r spawns its task: the task runs, its handle in r's hand.
  /\ rpc[r] = "spawn"
  \* r pruned;
  /\ life' = [life EXCEPT ![r] = "live"]
  \* the task runs;
  /\ loc' = [loc EXCEPT ![r] = "hand"]
  \* r holds its handle;
  /\ rpc' = [rpc EXCEPT ![r] = "push"]
  \* and goes on to put it in the set;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, held, budget, pruning>>
  \* and nothing else changes.

RegPush(r) ==
  \* r puts its task's handle in the set.
  /\ rpc[r] = "push"
  \* r holds the handle;
  /\ loc' = [loc EXCEPT ![r] = "queue"]
  \* it is in the set;
  /\ rpc' = [rpc EXCEPT ![r] = "recheck"]
  \* and r goes on to read the flag again;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, life, held, budget, pruning>>
  \* and nothing else changes.

RegRecheck(r) ==
  \* r reads the flag again: shutdown has begun, so it aborts its own task
  \* (RED: it never looks again).
  /\ rpc[r] = "recheck"
  \* r pushed its handle;
  /\ life' = [life EXCEPT ![r] = IF RecheckAfterPush /\ flag /\ @ = "live" THEN "aborting" ELSE @]
  \* the task is aborted when shutdown has begun and it still runs;
  /\ rpc' = [rpc EXCEPT ![r] = "over"]
  \* and r is done;
  /\ UNCHANGED <<flag, edgeOpen, cpc, drains, dr, dw, snd, loc, held, budget, pruning>>
  \* and nothing else changes.

CallerStep(c) == SetFlag(c) \/ Spawn(c) \/ SpawnAfterCheck(c) \/ InPlaceDone(c) \/ Await(c)
\* What caller c does once it called; it does it whenever it can.

DrainStep == DrainCheck \/ DrainPop \/ (\E t \in Tasks : DrainJoin(t))
             \/ DrainDeadline \/ AbortTimeout \/ DrainReturn
\* What the drain does, on the edge or in place; it does it whenever it can,
\* and its deadline and abort timeout do pass.

SpawnStep(r) == RegCheck(r) \/ PruneEnter(r) \/ PruneLook(r) \/ PrunePop(r)
                \/ PruneBack(r) \/ PruneExit(r) \/ RegSpawn(r) \/ RegPush(r)
                \/ RegRecheck(r)
\* What late spawn r does; it does it whenever it can.

World == \/ EdgeClose \/ Teardown
         \* the edge closes, or the runtime is torn down;
         \/ \E c \in Callers : Call(c) \/ CancelCaller(c) \/ CancelWinner(c)
         \* a caller calls, or is cancelled;
         \/ \E t \in Tasks : Finish(t) \/ Stop(t)
         \* a task ends, or an aborted one is dropped.
\* What happens to the coordinator from outside, owed nothing.

Next == World \/ DrainStep \/ (\E c \in Callers : CallerStep(c)) \/ (\E r \in Late : SpawnStep(r))
\* Each step is the world's, the drain's, a caller's or a late spawn's.

Spec == /\ Init /\ [][Next]_vars
        \* start before shutdown, then take any steps;
        /\ \A c \in Callers : WF_vars(CallerStep(c))
        \* a caller that called goes on whenever it can;
        /\ WF_vars(DrainStep)
        \* so does the drain, and its deadline and abort timeout pass;
        /\ \A r \in Late : WF_vars(SpawnStep(r))
        \* and so does every late spawn.
\* Nothing is owed by the world: a task may hang forever, an aborted task may
\* never be dropped, nobody need call, and nothing need cancel anything.

INV_NoLeak ==
  dr \in {"done", "dropped"} =>
  \* once the drain is over,
    \A t \in Tasks : (loc[t] = "queue" /\ ~Busy(t)) => life[t] # "live"
    \* every handle left in the set, with no late spawn at work on it, is
    \* that of a task that ended or was aborted.
\* Promise: once the drain returned (or, in a RED, was dropped unstarted), no
\* task is left in the tracked set still running, never joined nor aborted,
\* unless a late spawn is still at work on it.

INV_OneDrain == drains <= 1
\* Promise: at most one drain is ever started.

INV_ErrMeansCancelled == \A c \in Callers : cpc[c] = "err" => dr = "killed"
\* Promise: a caller returns an error only when something cancelled the
\* drain, so the warning it logs tells the truth.

LIVE_CallersReturn == \A c \in Callers : (cpc[c] # "idle") ~> (cpc[c] \in {"ok", "err", "gone"})
\* Promise: every caller of shutdown returns (or was cancelled); none waits
\* forever.

====
