---- MODULE NodeShutdown ----
\* The story, in plain words.
\*
\* A node runs background tasks beside its requests. Two kinds matter here:
\*   * the ACP redrive (acp::redrive_owed), which settles registrations a
\*     previous run left owed. It runs on the worker pool and reads and
\*     writes storage;
\*   * a node service on the network edge (the libp2p host, its event
\*     handler, the replication loop), which also reaches storage.
\* Requests arrive from the network while the node accepts them.
\*
\* Shutting the node down runs one sequence (defra-node EmbeddedNode::shutdown
\* in crates/defra-node/src/lib.rs, embedded EmbeddedNode::shutdown and
\* ShutdownHandle::shutdown in crates/embedded/src/node.rs):
\*   1. abort every task in the node's shutdown set (BackgroundTasks,
\*      stop_services), then await each one until it let go of storage;
\*   2. stop accepting requests and drain those in flight, up to a deadline
\*      (NodeEdge::drain, crates/defra-node/src/network.rs). The HTTP server
\*      is a task of the node's scope, not a service: once the scope stops
\*      accepting, its accept loop stops and each connection finishes the
\*      requests in flight and closes (crates/net/src/server.rs serve,
\*      until_closing), so the drain waits for them;
\*   3. stop the workers: the pool finishes what it admitted and stops;
\*   4. close regolith (close_database);
\*   5. flush telemetry, handing the exporter join to the network runtime, or
\*      to a thread of its own when the runtime refuses it;
\*   6. drop the network runtime (NodeEdge::stop_owned).
\* An aborted task stops at its next poll: a storage call it is in the middle
\* of finishes first. Awaiting the task is what tells shutdown it is over.
\*
\* What must never happen:
\*   1. a task reads or writes storage after regolith closed
\*      - INV_NoStorageAfterClose;
\*   2. a task outlives the step that should have stopped it, because nobody
\*      holds it to stop it - INV_NothingDetached;
\*   3. the caller of shutdown runs the exporter join itself
\*      - INV_CallerNeverJoinsExporter;
\*   4. shutdown never finishes - LIVE_ShutdownTerminates.
\*
\* The Rust code each guard models:
\*   RedriveInSet    crates/defra-node/src/node_acp.rs and
\*                   crates/embedded/src/node_acp.rs gate_external_acp: the
\*                   redrive is a workers::spawn_background task the node pushes
\*                   into BackgroundTasks, instead of drop(network.spawn(..));
\*   AwaitServices   crates/edge/src/task.rs Service::stopped, called by
\*                   crates/embedded/src/node.rs stop_services;
\*   DrainDeadline   crates/edge/src/handle.rs Scope::drain (a drain that
\*                   waits up to `within`);
\*   SlotKeepsHandle crates/defra-node/src/lib.rs EmbeddedNode::shutdown, the
\*                   telemetry slot that keeps the handle when the runtime
\*                   refuses the flush.

EXTENDS Naturals  \* numbers, for comparing step positions

CONSTANTS
  RedriveInSet,     \* TRUE: the redrive is held in the shutdown set, on the pool
  AwaitServices,    \* TRUE: shutdown waits for each stopped service to end
  DrainDeadline,    \* TRUE: the drain gives up on requests at its deadline
  SlotKeepsHandle   \* TRUE: a refused flush keeps its handle for its own thread

Tasks == {"redrive", "service"}  \* the two background tasks modeled

\* Where each task runs, so what stopping kills it when nobody stopped it first:
\* the pool's workers, or the network runtime. The old redrive ran detached on
\* the network runtime.
Host(t) == IF t = "service" THEN "edge"
           ELSE IF RedriveInSet THEN "pool" ELSE "edge"

\* Whether shutdown holds the task to abort and await it.
InSet(t) == t = "service" \/ RedriveInSet

\* Whether shutdown waits for the task to end after aborting it.
Awaited(t) == InSet(t) /\ (t = "redrive" \/ AwaitServices)

\* The shutdown steps, numbered in the order the code runs them.
Order == [Running |-> 1, Abort |-> 2, Await |-> 3, Drain |-> 4,
          StopWorkers |-> 5, CloseStore |-> 6, Flush |-> 7,
          DropRuntime |-> 8, Done |-> 9]

\* The position of a step in that order.
Pos(p) == Order[p]

VARIABLES
  phase,          \* which shutdown step the node is at
  status,         \* per task: "running", "stopping" (aborted) or "done"
  inOp,           \* per task: TRUE while it is in the middle of a storage call
  inflight,       \* how many requests are in flight (0 or 1)
  deadlinePassed, \* TRUE once the drain's deadline went by
  storeOpen,      \* TRUE until regolith closes
  flush,          \* telemetry: "pending", "thread" (on its own thread) or "done"
  callerJoined    \* TRUE if the shutdown caller ran the exporter join itself

vars == <<phase, status, inOp, inflight, deadlinePassed, storeOpen, flush,
          callerJoined>>  \* everything, for stuttering and fairness

TypeOK ==
  /\ phase \in DOMAIN Order                              \* a known step
  /\ status \in [Tasks -> {"running", "stopping", "done"}] \* a known task state
  /\ inOp \in [Tasks -> BOOLEAN]                         \* a flag per task
  /\ inflight \in 0..1                                   \* at most one request
  /\ deadlinePassed \in BOOLEAN                          \* a flag
  /\ storeOpen \in BOOLEAN                               \* a flag
  /\ flush \in {"pending", "thread", "done"}             \* a known flush state
  /\ callerJoined \in BOOLEAN                            \* a flag

Init ==
  /\ phase = "Running"                       \* the node is serving
  /\ status = [t \in Tasks |-> "running"]    \* both tasks run
  /\ inOp = [t \in Tasks |-> FALSE]          \* neither is in a storage call
  /\ inflight = 0                            \* no request yet
  /\ deadlinePassed = FALSE                  \* no drain has started
  /\ storeOpen = TRUE                        \* regolith is open
  /\ flush = "pending"                       \* telemetry not flushed yet
  /\ callerJoined = FALSE                    \* the caller joined nothing

\* ---- What tasks and requests do on their own ----

\* A running task starts a storage call. It does not look at whether regolith
\* is open: nothing in a task can know that, so the shutdown order must.
BeginOp(t) ==
  /\ status[t] = "running"                   \* only a task not yet aborted
  /\ ~inOp[t]                                \* one call at a time
  /\ inOp' = [inOp EXCEPT ![t] = TRUE]       \* it is now inside storage
  /\ UNCHANGED <<phase, status, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* The storage call returns.
EndOp(t) ==
  /\ inOp[t]                                 \* it was inside storage
  /\ inOp' = [inOp EXCEPT ![t] = FALSE]      \* now it is out
  /\ UNCHANGED <<phase, status, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* An aborted task reaches its next poll outside a storage call and ends.
Finish(t) ==
  /\ status[t] = "stopping"                  \* it was aborted
  /\ ~inOp[t]                                \* its current call is over
  /\ status' = [status EXCEPT ![t] = "done"] \* it has ended
  /\ UNCHANGED <<phase, inOp, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* A request arrives while the node still accepts them.
Arrive ==
  /\ Pos(phase) < Pos("Drain")               \* accepting has not stopped
  /\ inflight = 0                            \* keep the instance small
  /\ inflight' = 1                           \* one request in flight
  /\ UNCHANGED <<phase, status, inOp, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* A request completes. Nothing forces this: a request may hang forever.
Complete ==
  /\ inflight = 1                            \* there is one
  /\ inflight' = 0                           \* and it is done
  /\ UNCHANGED <<phase, status, inOp, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* The telemetry thread of its own finishes the exporter join.
ThreadFlush ==
  /\ flush = "thread"                        \* the join was handed to it
  /\ flush' = "done"                         \* and it ran there
  /\ UNCHANGED <<phase, status, inOp, inflight, deadlinePassed, storeOpen,
                 callerJoined>>

\* Time passes during the drain until its deadline.
DeadlinePasses ==
  /\ phase = "Drain"                         \* only a drain has a deadline
  /\ DrainDeadline                           \* and only if it sets one
  /\ ~deadlinePassed                         \* it passes once
  /\ deadlinePassed' = TRUE                  \* now it has
  /\ UNCHANGED <<phase, status, inOp, inflight, storeOpen, flush, callerJoined>>

\* ---- The shutdown sequence ----

\* Someone calls shutdown.
Begin ==
  /\ phase = "Running"                       \* the node was serving
  /\ phase' = "Abort"                        \* shutdown starts
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* Every task shutdown holds is aborted; a task it does not hold keeps running.
Abort ==
  /\ phase = "Abort"                         \* this step
  /\ status' = [t \in Tasks |->              \* for each task:
                  IF InSet(t) /\ status[t] = "running"
                  THEN "stopping"            \* held: told to stop
                  ELSE status[t]]            \* not held: nobody tells it
  /\ phase' = "Await"                        \* next: wait for them
  /\ UNCHANGED <<inOp, inflight, deadlinePassed, storeOpen, flush, callerJoined>>

\* Shutdown waits until every task it awaits has ended.
Await ==
  /\ phase = "Await"                         \* this step
  /\ \A t \in Tasks : Awaited(t) => status[t] = "done" \* those ended
  /\ phase' = "Drain"                        \* next: stop accepting, drain
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* The drain ends when nothing is in flight, or when its deadline passed.
Drain ==
  /\ phase = "Drain"                         \* this step
  /\ inflight = 0 \/ deadlinePassed          \* drained, or out of time
  /\ phase' = "StopWorkers"                  \* next: the pool
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, storeOpen, flush,
                 callerJoined>>

\* The pool stops. A background task still on it is dropped with its worker.
StopWorkers ==
  /\ phase = "StopWorkers"                   \* this step
  /\ status' = [t \in Tasks |->              \* each task the pool hosts ends
                  IF Host(t) = "pool" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "pool" THEN FALSE ELSE inOp[t]]
  /\ phase' = "CloseStore"                   \* next: regolith
  /\ UNCHANGED <<inflight, deadlinePassed, storeOpen, flush, callerJoined>>

\* Regolith closes.
CloseStore ==
  /\ phase = "CloseStore"                    \* this step
  /\ storeOpen' = FALSE                      \* storage is gone
  /\ phase' = "Flush"                        \* next: telemetry
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, flush, callerJoined>>

\* The runtime runs the flush on one of its blocking threads.
FlushAccepted ==
  /\ phase = "Flush"                         \* this step
  /\ flush' = "done"                         \* flushed off the caller
  /\ phase' = "DropRuntime"                  \* next: the runtime
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, storeOpen,
                 callerJoined>>

\* The runtime refuses the flush. With the slot, the handle goes to a thread of
\* its own; without it, the refused closure drops the handle on the caller,
\* whose drop runs the join right there.
FlushRefused ==
  /\ phase = "Flush"                         \* this step
  /\ IF SlotKeepsHandle
     THEN /\ flush' = "thread"               \* handed to its own thread
          /\ UNCHANGED callerJoined          \* the caller waits for nothing
     ELSE /\ flush' = "done"                 \* flushed, but by the caller
          /\ callerJoined' = TRUE            \* the caller ran the join
  /\ phase' = "DropRuntime"                  \* next: the runtime
  /\ UNCHANGED <<status, inOp, inflight, deadlinePassed, storeOpen>>

\* The network runtime goes, and with it every task and request on it.
DropRuntime ==
  /\ phase = "DropRuntime"                   \* this step
  /\ status' = [t \in Tasks |->              \* each task the runtime hosts ends
                  IF Host(t) = "edge" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "edge" THEN FALSE ELSE inOp[t]]
  /\ inflight' = 0                           \* a hung request is cancelled
  /\ phase' = "Done"                         \* shutdown returns
  /\ UNCHANGED <<deadlinePassed, storeOpen, flush, callerJoined>>

\* Any one thing happens.
Next ==
  \/ \E t \in Tasks : BeginOp(t) \/ EndOp(t) \/ Finish(t)  \* a task acts
  \/ Arrive \/ Complete                                    \* a request acts
  \/ ThreadFlush \/ DeadlinePasses                         \* time and threads
  \/ Begin \/ Abort \/ Await \/ Drain \/ StopWorkers       \* shutdown steps
  \/ CloseStore \/ FlushAccepted \/ FlushRefused \/ DropRuntime

\* Fairness: shutdown is called and each of its steps is taken once possible;
\* a storage call returns; an aborted task reaches its next poll; time passes;
\* a thread runs. A request is not promised to complete.
Spec ==
  /\ Init                                             \* start serving
  /\ [][Next]_vars                                    \* then take steps
  /\ WF_vars(Begin)                                   \* shutdown is called
  /\ WF_vars(Abort) /\ WF_vars(Await) /\ WF_vars(Drain) \* steps proceed
  /\ WF_vars(StopWorkers) /\ WF_vars(CloseStore)
  /\ WF_vars(FlushAccepted \/ FlushRefused) /\ WF_vars(DropRuntime)
  /\ \A t \in Tasks : WF_vars(EndOp(t)) /\ WF_vars(Finish(t)) \* tasks move
  /\ WF_vars(DeadlinePasses) /\ WF_vars(ThreadFlush)  \* time and threads

\* ---- What must hold ----

\* No task is inside storage once regolith closed.
INV_NoStorageAfterClose == \A t \in Tasks : inOp[t] => storeOpen

\* Once shutdown is past waiting for its tasks, none is left running or
\* stopping: every task was held, stopped and waited for.
INV_NothingDetached ==
  Pos(phase) > Pos("Await") => \A t \in Tasks : status[t] = "done"

\* The shutdown caller never runs the exporter join itself.
INV_CallerNeverJoinsExporter == ~callerJoined

\* Shutdown always returns.
LIVE_ShutdownTerminates == <>(phase = "Done")

====
