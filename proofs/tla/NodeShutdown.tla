---- MODULE NodeShutdown ----
\* The story, in plain words.
\*
\* A node runs background tasks beside its requests. Three kinds matter here:
\*   * the ACP redrive (acp::redrive_owed), which settles registrations a
\*     previous run left owed. It runs on the worker pool and reads and
\*     writes storage;
\*   * a node service on the network edge (the libp2p host, its event
\*     handler, the replication loop), which also reaches storage;
\*   * a P2P coordinator background task (a DAG fetch, an ordered or gossip
\*     broadcast). It runs on the network edge and may hold a store
\*     transaction. Only the coordinator holds it: its own shutdown drains
\*     it, and that drain is network work it runs on the node's scope.
\* Requests (signed queries, HTTP requests, an accepted connection's included)
\* arrive while the node admits them. So do in-process writes: an unsigned
\* EmbeddedNode::execute_request and the commit of an explicit transaction
\* (EmbeddedNode::commit_transaction, EmbeddedTransaction::commit). A write
\* commits, then registers the durable head marker that owes its head to the
\* replicators; that registration needs P2P not yet shutting down.
\*
\* Shutting the node down runs one sequence, defra-node's
\* EmbeddedNode::shutdown and Teardown::run in crates/defra-node/src/lib.rs.
\* (The embedded node, crates/embedded/src/node.rs, serves no requests and
\* has no request step: it stops P2P, then its background tasks, then closes
\* its database. This model is defra-node's order.)
\*   1. abort every task in the node's shutdown set (BackgroundTasks,
\*      stop_services), then await each one until it let go of storage;
\*   2. stop admitting requests and drain those in flight, up to a deadline
\*      (NodeEdge::drain_requests, crates/defra-node/src/network.rs: the
\*      request scope, a child of the node's scope, closes);
\*   3. shut P2P down (P2PLifecycle::shutdown, IrohPeerShutdown::shutdown,
\*      SyncShutdownHandle::shutdown): from its start no write can register
\*      its marker any more. The coordinator spawns its drain on the node's
\*      scope, and the drain aborts and awaits its tasks. A scope that already
\*      stopped accepting refuses the drain, and the tasks run on with nobody
\*      to stop them;
\*   4. stop accepting the rest of the node's network work and drain it
\*      (NodeEdge::drain). This closes the node's scope, and with it the
\*      request scope if it was still open;
\*   5. end the explicit transactions callers left open (end_transactions:
\*      DbTransactionRegistry::cleanup_stale_transactions with no idle
\*      allowance). A transaction left open keeps its store transaction in
\*      the registry's table, past its owner's worker, and the store's close
\*      then waits its whole timeout for it: the 30 s stall of gents#3291;
\*   6. stop the workers: the pool finishes what it admitted and stops;
\*   7. close regolith (close_database);
\*   8. flush telemetry, handing the exporter join to the network runtime, or
\*      to a thread of its own when the runtime refuses it;
\*   9. drop the network runtime (NodeEdge::stop_owned).
\* An aborted task stops at its next poll: a storage call it is in the middle
\* of finishes first. Awaiting the task is what tells shutdown it is over.
\*
\* Several callers may call shutdown, and any may be cancelled. One leads: it
\* hands the teardown to a blocking thread of a runtime, as a task its own
\* cancellation does not stop, and every other caller waits for the end. A run
\* that ends early (the task refused or panicked, an in-place run cancelled)
\* hands the lead back through its Leading guard; the next caller reruns the
\* teardown in place, each step skipping what an earlier run took, a store
\* close that was interrupted included.
\*
\* What must never happen:
\*   1. a task reads or writes storage after regolith closed
\*      - INV_NoStorageAfterClose;
\*   2. a task outlives the step that should have stopped it, because nobody
\*      holds it to stop it - INV_NothingDetached;
\*   3. the caller of shutdown runs the exporter join itself
\*      - INV_CallerNeverJoinsExporter;
\*   4. a request the node admitted commits after P2P began shutting down, so
\*      its head marker is never registered - INV_AdmittedWritesMarked. The
\*      one exception is a request that outlived the request drain's deadline:
\*      shutdown stops waiting for it, and its commit is past the promise;
\*   5. the store's close waits for a transaction a caller left open
\*      - INV_CloseWaitsForNoTransaction;
\*   6. a caller returns before the shutdown is over
\*      - INV_ReturnsAfterShutdown;
\*   7. shutdown moves past regolith's close with regolith still open
\*      - INV_StoreClosedPastClose;
\*   8. shutdown never finishes, or the caller that stays never returns
\*      - LIVE_ShutdownTerminates.
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
\*   SlotKeepsHandle crates/defra-node/src/lib.rs Teardown::run, the
\*                   telemetry slot that keeps the handle when the runtime
\*                   refuses the flush;
\*   P2PFirst        crates/defra-node/src/lib.rs Teardown::run, which shuts
\*                   P2P down before NodeEdge::drain closes the node's scope.
\*                   Regression:
\*                   shutdown_drains_coordinator_tasks_before_the_network_scope_closes
\*                   in crates/defra-node/src/p2p_tests.rs;
\*   WaitersWait     crates/defra-node/src/lib.rs EmbeddedNode::shutdown: a
\*                   caller that does not lead awaits the teardown's `done`
\*                   until `finished`. Regression:
\*                   cancelled_and_concurrent_shutdown_callers_share_one_shutdown;
\*   HandsBackLead   crates/defra-node/src/lib.rs Leading, the guard that moves
\*                   with whatever runs the teardown and resets `started` when
\*                   it goes before `finished`;
\*   RerunCompletesClose crates/storage/src/backends/regolith/store.rs
\*                   RegolithStore::close, which no longer returns early on a
\*                   second call. Regression:
\*                   a_close_cancelled_while_waiting_completes_when_called_again;
\*   EndsOpenTransactions crates/defra-node/src/lib.rs end_transactions,
\*                   called by Teardown::run before close_database. Regression:
\*                   shutdown_ends_a_transaction_left_open_without_waiting_for_it
\*                   in crates/defra-node/src/p2p_tests.rs;
\*   CommitsAdmitted crates/defra-node/src/lib.rs EmbeddedNode::as_request,
\*                   through which begin_transaction (and with it
\*                   begin_transaction_guard), commit_transaction (and
\*                   EmbeddedTransaction::commit) and the unsigned
\*                   execute_prepared_request run as tasks of the request
\*                   scope, as signed queries do. The raw runner() is outside
\*                   the promise;
\*   RequestsFirst   crates/defra-node/src/lib.rs Teardown::run, which drains
\*                   the request scope (NodeEdge::drain_requests) before P2P,
\*                   with every HTTP request run as a task of it
\*                   (crates/http/src/server.rs Server::router), so a client
\*                   that goes away does not end the request's admission
\*                   before its handler ends. The CLI's start/run.rs also
\*                   stops its HTTP server before P2P. Regression:
\*                   shutdown_refuses_or_marks_every_write_admitted_around_it
\*                   in crates/defra-node/src/p2p_tests.rs.

EXTENDS Naturals  \* numbers, for comparing step positions

CONSTANTS
  RedriveInSet,     \* TRUE: the redrive is held in the shutdown set, on the pool
  AwaitServices,    \* TRUE: shutdown waits for each stopped service to end
  DrainDeadline,    \* TRUE: each drain gives up on requests at its deadline
  SlotKeepsHandle,  \* TRUE: a refused flush keeps its handle for its own thread
  P2PFirst,         \* TRUE: P2P shuts down before the drain closes the scope
  RequestsFirst,    \* TRUE: requests stop and drain before P2P shuts down
  CommitsAdmitted,  \* TRUE: in-process writes are admitted as requests are
  EndsOpenTransactions, \* TRUE: shutdown ends transactions left open
  WaitersWait,      \* TRUE: a caller that does not lead waits for the end
  HandsBackLead,    \* TRUE: a run that ends early hands the lead back
  RerunCompletesClose \* TRUE: a rerun completes an interrupted store close

Tasks == {"redrive", "service", "coordinator"}  \* the background tasks modeled

\* Two shutdown callers: "a" stays, "b" may be cancelled.
Callers == {"a", "b"}

\* Where each task runs, so what stopping kills it when nobody stopped it first:
\* the pool's workers, or the network runtime. The old redrive ran detached on
\* the network runtime.
Host(t) == IF t \in {"service", "coordinator"} THEN "edge"
           ELSE IF RedriveInSet THEN "pool" ELSE "edge"

\* Whether the node's shutdown set holds the task to abort and await it. The
\* coordinator task is not in it: only the coordinator's own drain holds it.
InSet(t) == t = "service" \/ (t = "redrive" /\ RedriveInSet)

\* Whether shutdown waits for the task to end after aborting it.
Awaited(t) == InSet(t) /\ (t = "redrive" \/ AwaitServices)

\* The shutdown steps, numbered in the order the code runs them. The request
\* drain always has its place; without RequestsFirst it closes nothing and
\* waits for nothing. P2P comes before the node's drain when P2PFirst holds,
\* after it otherwise.
Order == IF P2PFirst
         THEN [Running |-> 1, Abort |-> 2, Await |-> 3, Requests |-> 4,
               P2P |-> 5, Drain |-> 6, EndTxns |-> 7, StopWorkers |-> 8,
               CloseStore |-> 9, Flush |-> 10, DropRuntime |-> 11, Done |-> 12]
         ELSE [Running |-> 1, Abort |-> 2, Await |-> 3, Requests |-> 4,
               Drain |-> 5, P2P |-> 6, EndTxns |-> 7, StopWorkers |-> 8,
               CloseStore |-> 9, Flush |-> 10, DropRuntime |-> 11, Done |-> 12]

\* The position of a step in that order.
Pos(p) == Order[p]

\* The step that comes right after step p.
After(p) == CHOOSE q \in DOMAIN Order : Order[q] = Order[p] + 1

\* The step that should have ended task t: the wait for the node's own
\* tasks, or P2P's shutdown for the coordinator task.
StoppedBy(t) == IF t = "coordinator" THEN "P2P" ELSE "Await"

VARIABLES
  who,            \* per caller: "out", "waiting", "leading", "returned" or
                  \* "cancelled"
  driver,         \* who runs the teardown: "none", "task" or a caller
  started,        \* TRUE while a caller holds the lead
  everTask,       \* TRUE once the teardown was handed to a runtime's task
  closer,         \* the driver that began regolith's close, or "none"
  phase,          \* which shutdown step the node is at
  status,         \* per task: "running", "stopping" (aborted) or "done"
  inOp,           \* per task: TRUE while it is in the middle of a storage call
  inflight,       \* how many requests are in flight (0 or 1)
  commits,        \* how many in-process writes are in flight (0 or 1)
  admitting,      \* TRUE while the node admits requests
  overdue,        \* TRUE once the request drain gave up on a request
  markerLost,     \* TRUE once an admitted write committed with P2P going down
  deadlinePassed, \* TRUE once the current drain's deadline went by
  scopeOpen,      \* TRUE until the node's drain closes its network scope
  coordDrain,     \* the coordinator's drain: "none", "admitted" or "refused"
  storeOpen,      \* TRUE until regolith closes
  txnOpen,        \* TRUE while a transaction a caller left open is in the registry
  closeWaited,    \* TRUE once the store's close waited for such a transaction
  flush,          \* telemetry: "pending", "thread" (on its own thread) or "done"
  callerJoined    \* TRUE if the shutdown caller ran the exporter join itself

leadVars == <<who, driver, started, everTask, closer>>      \* the leadership
taskVars == <<status, inOp>>                                \* the tasks
reqVars == <<inflight, commits, admitting, overdue, markerLost>> \* requests
netVars == <<deadlinePassed, scopeOpen, coordDrain>>        \* drains and P2P
endVars == <<storeOpen, txnOpen, closeWaited, flush, callerJoined>> \* the end
vars == <<leadVars, phase, taskVars, reqVars, netVars, endVars>> \* all of it

\* Someone runs the teardown's steps.
Driving == driver # "none"

\* P2P has begun shutting down: no write registers its marker from here on.
P2PDown == coordDrain # "none"

TypeOK ==
  /\ who \in [Callers -> {"out", "waiting", "leading", "returned",
                          "cancelled"}]          \* a known caller state
  /\ driver \in Callers \cup {"none", "task"}   \* a known driver
  /\ started \in BOOLEAN                         \* a flag
  /\ everTask \in BOOLEAN                        \* a flag
  /\ closer \in Callers \cup {"none", "task"}   \* a known closer
  /\ phase \in DOMAIN Order                              \* a known step
  /\ status \in [Tasks -> {"running", "stopping", "done"}] \* a known task state
  /\ inOp \in [Tasks -> BOOLEAN]                         \* a flag per task
  /\ inflight \in 0..1                                   \* at most one request
  /\ commits \in 0..1                                    \* at most one write
  /\ admitting \in BOOLEAN                               \* a flag
  /\ overdue \in BOOLEAN                                 \* a flag
  /\ markerLost \in BOOLEAN                              \* a flag
  /\ deadlinePassed \in BOOLEAN                          \* a flag
  /\ scopeOpen \in BOOLEAN                               \* a flag
  /\ coordDrain \in {"none", "admitted", "refused"}      \* a known drain state
  /\ storeOpen \in BOOLEAN                               \* a flag
  /\ txnOpen \in BOOLEAN                                 \* a flag
  /\ closeWaited \in BOOLEAN                             \* a flag
  /\ flush \in {"pending", "thread", "done"}             \* a known flush state
  /\ callerJoined \in BOOLEAN                            \* a flag

Init ==
  /\ who = [c \in Callers |-> "out"]         \* nobody called shutdown
  /\ driver = "none"                         \* nobody runs it
  /\ started = FALSE                         \* nobody leads
  /\ everTask = FALSE                        \* no task was tried
  /\ closer = "none"                         \* the close has not begun
  /\ phase = "Running"                       \* the node is serving
  /\ status = [t \in Tasks |-> "running"]    \* every task runs
  /\ inOp = [t \in Tasks |-> FALSE]          \* none is in a storage call
  /\ inflight = 0                            \* no request yet
  /\ commits = 0                             \* no in-process write yet
  /\ admitting = TRUE                        \* requests are admitted
  /\ overdue = FALSE                         \* no request was given up on
  /\ markerLost = FALSE                      \* no write lost its marker
  /\ deadlinePassed = FALSE                  \* no drain has started
  /\ scopeOpen = TRUE                        \* the scope accepts work
  /\ coordDrain = "none"                     \* the coordinator has not drained
  /\ storeOpen = TRUE                        \* regolith is open
  /\ txnOpen = FALSE                         \* no transaction left open yet
  /\ closeWaited = FALSE                     \* the close waited for nothing
  /\ flush = "pending"                       \* telemetry not flushed yet
  /\ callerJoined = FALSE                    \* the caller joined nothing

\* ---- What tasks and requests do on their own ----

\* A running task starts a storage call. It does not look at whether regolith
\* is open: nothing in a task can know that, so the shutdown order must.
BeginOp(t) ==
  /\ status[t] = "running"                   \* only a task not yet aborted
  /\ ~inOp[t]                                \* one call at a time
  /\ inOp' = [inOp EXCEPT ![t] = TRUE]       \* it is now inside storage
  /\ UNCHANGED <<leadVars, phase, status, reqVars, netVars, endVars>>

\* The storage call returns.
EndOp(t) ==
  /\ inOp[t]                                 \* it was inside storage
  /\ inOp' = [inOp EXCEPT ![t] = FALSE]      \* now it is out
  /\ UNCHANGED <<leadVars, phase, status, reqVars, netVars, endVars>>

\* An aborted task reaches its next poll outside a storage call and ends.
Finish(t) ==
  /\ status[t] = "stopping"                  \* it was aborted
  /\ ~inOp[t]                                \* its current call is over
  /\ status' = [status EXCEPT ![t] = "done"] \* it has ended
  /\ UNCHANGED <<leadVars, phase, inOp, reqVars, netVars, endVars>>

\* A request arrives while the node still admits them.
Arrive ==
  /\ admitting                               \* admission is open
  /\ inflight = 0                            \* keep the instance small
  /\ inflight' = 1                           \* one request in flight
  /\ UNCHANGED <<leadVars, phase, taskVars, commits, admitting, overdue, markerLost,
                 netVars, endVars>>

\* An in-process write starts: admitted while the node admits requests, or,
\* unadmitted, any time the store is still open.
ArriveCommit ==
  /\ IF CommitsAdmitted THEN admitting       \* through the request scope
     ELSE storeOpen                          \* or straight to the runner
  /\ commits = 0                             \* keep the instance small
  /\ commits' = 1                            \* one write in flight
  /\ UNCHANGED <<leadVars, phase, taskVars, inflight, admitting, overdue, markerLost,
                 netVars, endVars>>

\* The in-process write commits; its marker is registered only if P2P is not
\* yet going down.
CompleteCommit ==
  /\ commits = 1                             \* there is one
  /\ commits' = 0                            \* and it committed
  /\ markerLost' = (markerLost \/ P2PDown)   \* committed with no marker
  /\ UNCHANGED <<leadVars, phase, taskVars, inflight, admitting, overdue, netVars,
                 endVars>>

\* A request completes, committing its write; its marker is registered only
\* if P2P is not yet going down. Nothing forces this: a request may hang.
Complete ==
  /\ inflight = 1                            \* there is one
  /\ inflight' = 0                           \* and it is done
  /\ markerLost' = (markerLost \/ P2PDown)   \* committed with no marker
  /\ UNCHANGED <<leadVars, phase, taskVars, commits, admitting, overdue, netVars,
                 endVars>>

\* A caller begins an explicit transaction and leaves it open: admitted while
\* the node admits requests, or, unadmitted, any time the store is open.
BeginTxn ==
  /\ IF CommitsAdmitted THEN admitting ELSE storeOpen \* where it may begin
  /\ ~txnOpen                                \* keep the instance small
  /\ txnOpen' = TRUE                         \* it holds a store transaction
  /\ UNCHANGED <<leadVars, phase, taskVars, reqVars, netVars, storeOpen, closeWaited,
                 flush, callerJoined>>

\* Shutdown ends the transactions left open, while their workers still run.
EndTxns ==
  /\ phase = "EndTxns"                       \* this step
  /\ Driving                                \* someone runs the teardown
  /\ IF EndsOpenTransactions
     THEN txnOpen' = FALSE                   \* each one ended
     ELSE UNCHANGED txnOpen                  \* left in the registry
  /\ phase' = "StopWorkers"                  \* next: the pool
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, storeOpen, closeWaited, flush,
                 callerJoined>>

\* The telemetry thread of its own finishes the exporter join.
ThreadFlush ==
  /\ flush = "thread"                        \* the join was handed to it
  /\ flush' = "done"                         \* and it ran there
  /\ UNCHANGED <<leadVars, phase, taskVars, reqVars, netVars, storeOpen, txnOpen,
                 closeWaited, callerJoined>>

\* Time passes during a drain until its deadline.
DeadlinePasses ==
  /\ phase \in {"Requests", "Drain"}         \* only a drain has a deadline
  /\ DrainDeadline                           \* and only if it sets one
  /\ ~deadlinePassed                         \* it passes once per drain
  /\ deadlinePassed' = TRUE                  \* now it has
  /\ UNCHANGED <<leadVars, phase, taskVars, reqVars, scopeOpen, coordDrain, endVars>>

\* ---- The shutdown sequence ----

\* Someone calls shutdown.
Begin ==
  /\ phase = "Running"                       \* the node was serving
  /\ Driving                                \* someone runs the teardown
  /\ phase' = "Abort"                        \* shutdown starts
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, endVars>>

\* Every task shutdown holds is aborted; a task it does not hold keeps running.
Abort ==
  /\ phase = "Abort"                         \* this step
  /\ Driving                                \* someone runs the teardown
  /\ status' = [t \in Tasks |->              \* for each task:
                  IF InSet(t) /\ status[t] = "running"
                  THEN "stopping"            \* held: told to stop
                  ELSE status[t]]            \* not held: nobody tells it
  /\ phase' = "Await"                        \* next: wait for them
  /\ UNCHANGED <<leadVars, inOp, reqVars, netVars, endVars>>

\* Shutdown waits until every task it awaits has ended.
Await ==
  /\ phase = "Await"                         \* this step
  /\ Driving                                \* someone runs the teardown
  /\ \A t \in Tasks : Awaited(t) => status[t] = "done" \* those ended
  /\ phase' = "Requests"                     \* next: the requests
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, endVars>>

\* Work the drains count: requests, and admitted in-process writes.
Pending == inflight + (IF CommitsAdmitted THEN commits ELSE 0)

\* The request drain stops admission and ends when nothing is in flight, or at
\* its deadline, giving up on the work still in flight. Without
\* RequestsFirst requests share the node's scope and nothing happens here.
Requests ==
  /\ phase = "Requests"                      \* this step
  /\ Driving                                \* someone runs the teardown
  /\ IF RequestsFirst
     THEN /\ Pending = 0 \/ deadlinePassed   \* drained, or out of time
          /\ admitting' = FALSE              \* no request admitted any more
          /\ overdue' = (overdue \/ Pending > 0) \* given up on, if any is left
     ELSE UNCHANGED <<leadVars, admitting, overdue>>   \* requests keep coming
  /\ deadlinePassed' = FALSE                 \* the next drain has its own
  /\ phase' = After("Requests")              \* next step in the order
  /\ UNCHANGED <<leadVars, taskVars, inflight, commits, markerLost, scopeOpen,
                 coordDrain, endVars>>

\* P2P's shutdown starts the coordinator's drain on the node's scope. An open
\* scope admits it, and the drain aborts the coordinator task; a closed scope
\* refuses it, and nobody tells the task to stop.
P2PStart ==
  /\ phase = "P2P"                           \* this step
  /\ Driving                                \* someone runs the teardown
  /\ coordDrain = "none"                     \* the drain has not started
  /\ IF scopeOpen
     THEN /\ coordDrain' = "admitted"        \* the scope runs the drain
          /\ status' = [status EXCEPT        \* which aborts the task
                          !["coordinator"] =
                            IF @ = "running" THEN "stopping" ELSE @]
     ELSE /\ coordDrain' = "refused"         \* the closed scope refuses it
          /\ UNCHANGED status                \* the task runs on
  /\ UNCHANGED <<leadVars, phase, inOp, reqVars, deadlinePassed, scopeOpen, endVars>>

\* P2P's shutdown returns: an admitted drain once the task it aborted ended,
\* a refused one at once.
P2PEnd ==
  /\ phase = "P2P"                           \* this step
  /\ Driving                                \* someone runs the teardown
  /\ coordDrain # "none"                     \* the drain was started
  /\ coordDrain = "admitted" => status["coordinator"] = "done" \* and is over
  /\ phase' = After("P2P")                   \* next step in the order
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, endVars>>

\* The node's drain closes its scope, the request scope with it, and ends when
\* nothing is in flight, or when its deadline passed.
Drain ==
  /\ phase = "Drain"                         \* this step
  /\ Driving                                \* someone runs the teardown
  /\ Pending = 0 \/ deadlinePassed           \* drained, or out of time
  /\ scopeOpen' = FALSE                      \* the scope accepts no more work
  /\ admitting' = FALSE                      \* nor any request
  /\ deadlinePassed' = FALSE                 \* the next drain has its own
  /\ phase' = After("Drain")                 \* next step in the order
  /\ UNCHANGED <<leadVars, taskVars, inflight, commits, overdue, markerLost, coordDrain,
                 endVars>>

\* The pool stops. A background task still on it is dropped with its worker.
StopWorkers ==
  /\ phase = "StopWorkers"                   \* this step
  /\ Driving                                \* someone runs the teardown
  /\ status' = [t \in Tasks |->              \* each task the pool hosts ends
                  IF Host(t) = "pool" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "pool" THEN FALSE ELSE inOp[t]]
  /\ phase' = "CloseStore"                   \* next: regolith
  /\ UNCHANGED <<leadVars, reqVars, netVars, endVars>>

\* Regolith's close begins: the store stops handing out transactions and
\* waits for those still open. The driver that began it is remembered.
CloseBegin ==
  /\ phase = "CloseStore"                    \* this step
  /\ Driving                                \* someone runs the teardown
  /\ closer = "none"                         \* the close has not begun
  /\ closer' = driver                        \* this driver began it
  /\ UNCHANGED <<who, driver, started, everTask, phase, taskVars, reqVars,
                 netVars, endVars>>

\* Regolith closes: by the driver that began the close, or, when a rerun
\* completes an interrupted close, by the next one.
CloseEngine ==
  /\ phase = "CloseStore"                    \* this step
  /\ Driving                                \* someone runs the teardown
  /\ closer # "none"                         \* the close began
  /\ closer = driver \/ RerunCompletesClose  \* this run may finish it
  /\ storeOpen' = FALSE                      \* storage is gone
  /\ closeWaited' = (closeWaited \/ txnOpen) \* waited out its timeout for one
  /\ phase' = "Flush"                        \* next: telemetry
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, txnOpen, flush,
                 callerJoined>>

\* Without RerunCompletesClose a rerun finds the store already closing and
\* returns at once: regolith is never closed.
CloseSkipped ==
  /\ phase = "CloseStore"                    \* this step
  /\ Driving                                \* someone runs the teardown
  /\ closer # "none"                         \* an earlier run began the close
  /\ closer # driver                         \* and was interrupted
  /\ ~RerunCompletesClose                    \* the rerun returns early
  /\ phase' = "Flush"                        \* and shutdown moves on
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, endVars>>

\* ---- Who runs the teardown (EmbeddedNode::shutdown, Leading) ----

\* A caller enters shutdown.
Call(c) ==
  /\ who[c] = "out"                          \* not inside yet
  /\ who' = [who EXCEPT ![c] = "waiting"]    \* inside, not leading
  /\ UNCHANGED <<driver, started, everTask, closer, phase, taskVars, reqVars,
                 netVars, endVars>>

\* A caller takes the lead when nobody holds it. The first lead runs the
\* teardown as a task of a runtime; a lead handed back reruns it in place,
\* where each step skips what an earlier run already took.
Elect(c) ==
  /\ who[c] \in {"waiting", "leading"}       \* inside shutdown
  /\ ~started                                \* nobody leads
  /\ phase # "Done"                          \* and it is not over
  /\ started' = TRUE                         \* this caller leads
  /\ who' = [who EXCEPT ![c] = "leading"]    \* as such
  /\ driver' = IF everTask THEN c ELSE "task" \* on a runtime, then in place
  /\ everTask' = TRUE                        \* the task was tried
  /\ UNCHANGED <<closer, phase, taskVars, reqVars, netVars, endVars>>

\* Caller "b" is cancelled. If it ran the teardown in place, the run stops,
\* and the Leading guard hands the lead back. A task it waited for runs on.
Cancel ==
  /\ who["b"] \in {"waiting", "leading"}     \* inside shutdown
  /\ who' = [who EXCEPT !["b"] = "cancelled"] \* gone
  /\ IF driver = "b"
     THEN /\ driver' = "none"                 \* its run stops
          /\ started' = (started /\ ~HandsBackLead) \* lead handed back
     ELSE UNCHANGED <<driver, started>>      \* nothing else changes
  /\ UNCHANGED <<everTask, closer, phase, taskVars, reqVars, netVars, endVars>>

\* The runtime's task ends early (refused, or panicked); its Leading guard
\* hands the lead back.
TaskEnds ==
  /\ driver = "task"                         \* the task runs the teardown
  /\ phase # "Done"                          \* and has not finished
  /\ driver' = "none"                        \* it is gone
  /\ started' = (started /\ ~HandsBackLead)  \* lead handed back
  /\ UNCHANGED <<who, everTask, closer, phase, taskVars, reqVars, netVars,
                 endVars>>

\* A caller returns: once the shutdown is over, or, without WaitersWait, a
\* waiter at once.
Return(c) ==
  /\ who[c] \in {"waiting", "leading"}       \* inside shutdown
  /\ \/ phase = "Done"                       \* the shutdown ended
     \/ ~WaitersWait /\ who[c] = "waiting"    \* or it did not wait
  /\ who' = [who EXCEPT ![c] = "returned"]   \* it returned
  /\ UNCHANGED <<driver, started, everTask, closer, phase, taskVars, reqVars,
                 netVars, endVars>>

\* The runtime runs the flush on one of its blocking threads.
FlushAccepted ==
  /\ phase = "Flush"                         \* this step
  /\ Driving                                \* someone runs the teardown
  /\ flush' = "done"                         \* flushed off the caller
  /\ phase' = "DropRuntime"                  \* next: the runtime
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, storeOpen, txnOpen, closeWaited,
                 callerJoined>>

\* The runtime refuses the flush. With the slot, the handle goes to a thread of
\* its own; without it, the refused closure drops the handle on the caller,
\* whose drop runs the join right there.
FlushRefused ==
  /\ phase = "Flush"                         \* this step
  /\ Driving                                \* someone runs the teardown
  /\ IF SlotKeepsHandle
     THEN /\ flush' = "thread"               \* handed to its own thread
          /\ UNCHANGED callerJoined          \* the caller waits for nothing
     ELSE /\ flush' = "done"                 \* flushed, but by the caller
          /\ callerJoined' = TRUE            \* the caller ran the join
  /\ phase' = "DropRuntime"                  \* next: the runtime
  /\ UNCHANGED <<leadVars, taskVars, reqVars, netVars, storeOpen, txnOpen, closeWaited>>

\* The network runtime goes, and with it every task and request on it.
DropRuntime ==
  /\ phase = "DropRuntime"                   \* this step
  /\ Driving                                \* someone runs the teardown
  /\ status' = [t \in Tasks |->              \* each task the runtime hosts ends
                  IF Host(t) = "edge" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "edge" THEN FALSE ELSE inOp[t]]
  /\ inflight' = 0                           \* a hung request is cancelled
  /\ commits' = IF CommitsAdmitted THEN 0 ELSE commits \* and an admitted write
  /\ phase' = "Done"                         \* shutdown returns
  /\ UNCHANGED <<leadVars, admitting, overdue, markerLost, netVars, endVars>>

\* Any one thing happens.
Next ==
  \/ \E t \in Tasks : BeginOp(t) \/ EndOp(t) \/ Finish(t)  \* a task acts
  \/ Arrive \/ Complete \/ ArriveCommit \/ CompleteCommit  \* requests, writes
  \/ ThreadFlush \/ DeadlinePasses                         \* time and threads
  \/ Begin \/ Abort \/ Await \/ Requests                   \* shutdown steps
  \/ P2PStart \/ P2PEnd \/ Drain \/ EndTxns \/ StopWorkers \/ BeginTxn
  \/ CloseBegin \/ CloseEngine \/ CloseSkipped
  \/ FlushAccepted \/ FlushRefused \/ DropRuntime
  \/ \E c \in Callers : Call(c) \/ Elect(c) \/ Return(c)  \* callers act
  \/ Cancel \/ TaskEnds                                   \* runs end early

\* Fairness: shutdown is called and each of its steps is taken once possible;
\* a storage call returns; an aborted task reaches its next poll; time passes;
\* a thread runs. A request is not promised to complete.
Spec ==
  /\ Init                                             \* start serving
  /\ [][Next]_vars                                    \* then take steps
  /\ WF_vars(Begin)                                   \* shutdown is called
  /\ WF_vars(Abort) /\ WF_vars(Await) /\ WF_vars(Requests) \* steps proceed
  /\ WF_vars(P2PStart) /\ WF_vars(P2PEnd) /\ WF_vars(Drain)
  /\ WF_vars(EndTxns) /\ WF_vars(StopWorkers)
  /\ WF_vars(CloseBegin) /\ WF_vars(CloseEngine \/ CloseSkipped)
  /\ \A c \in Callers : WF_vars(Call(c)) /\ WF_vars(Elect(c)) /\ WF_vars(Return(c))
  /\ WF_vars(FlushAccepted \/ FlushRefused) /\ WF_vars(DropRuntime)
  /\ \A t \in Tasks : WF_vars(EndOp(t)) /\ WF_vars(Finish(t)) \* tasks move
  /\ WF_vars(DeadlinePasses) /\ WF_vars(ThreadFlush)  \* time and threads

\* ---- What must hold ----

\* No task is inside storage once regolith closed.
INV_NoStorageAfterClose == \A t \in Tasks : inOp[t] => storeOpen

\* Once shutdown is past the step that should have ended a task, that task is
\* not left running or stopping: it was held, stopped and waited for.
INV_NothingDetached ==
  \A t \in Tasks : Pos(phase) > Pos(StoppedBy(t)) => status[t] = "done"

\* The shutdown caller never runs the exporter join itself.
INV_CallerNeverJoinsExporter == ~callerJoined

\* Every write registered its head marker, unless shutdown gave up on it at
\* the request drain's deadline.
INV_AdmittedWritesMarked == markerLost => overdue

\* The store's close never waits for a transaction a caller left open.
INV_CloseWaitsForNoTransaction == ~closeWaited

\* A caller returns only once the shutdown is over.
INV_ReturnsAfterShutdown == \A c \in Callers : who[c] = "returned" => phase = "Done"

\* Past regolith's close step, regolith is closed.
INV_StoreClosedPastClose == Pos(phase) > Pos("CloseStore") => ~storeOpen

\* Shutdown always ends, and the caller that stays returns.
LIVE_ShutdownTerminates == <>(phase = "Done" /\ who["a"] = "returned")

====
