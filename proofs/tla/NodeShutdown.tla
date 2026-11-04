---- MODULE NodeShutdown ----
\* The story, in plain words.
\*
\* A node runs background tasks beside its requests. Two kinds matter here:
\*   * the ACP redrive (acp::redrive_owed), which settles registrations a
\*     previous run left owed. It runs on the worker pool and reads and
\*     writes storage;
\*   * a node service on the network edge (the libp2p host, its event
\*     handler, the replication loop), which also reaches storage.
\* Requests (signed queries, HTTP requests, an accepted connection's included)
\* arrive while the node admits them. So do in-process writes: an unsigned
\* Node::execute_request, Node::execute_as and the commit of an explicit
\* transaction (Node::commit_transaction, NodeTransaction::commit,
\* Node::txn_commit). A write commits, then registers the durable head marker
\* that owes its head to the replicators; that registration needs P2P not yet
\* shutting down. A caller may also leave an explicit transaction open: an
\* HTTP client gone mid-transaction, or a commit refused at shutdown, whose
\* discard over HTTP is refused too. It holds a store transaction in the
\* registry until something ends it.
\*
\* Shutting the node down runs one sequence (defra-node Node::shutdown
\* in crates/defra-node/src/lib.rs, and ShutdownHandle::stop in
\* crates/defra-node/src/managed_p2p.rs):
\*   1. abort every task in the node's shutdown set (BackgroundTasks), then
\*      await each one until it let go of storage;
\*   2. stop admitting requests and drain those in flight, up to a deadline
\*      (NodeEdge::drain_requests, crates/defra-node/src/network.rs: the
\*      request scope, a child of the node's scope, closes). The HTTP
\*      server is a task of the request scope, not a service: once the scope
\*      stops accepting, its accept loop stops and each connection finishes
\*      the requests in flight and closes (crates/net/src/server.rs serve,
\*      until_closing), so the drain waits for them;
\*   3. shut P2P down (ManagedP2PSystem::stop_transport): stop the services and
\*      await them. From its start no write can register its marker any more;
\*   4. stop accepting the rest of the node's network work and drain it, up
\*      to a deadline (NodeEdge::drain). This closes the node's scope, and
\*      with it the request scope if it was still open;
\*   5. end the explicit transactions callers left open
\*      (DbTransactionRegistry::cleanup_stale_transactions with no idle
\*      allowance). Regolith's close waits its whole timeout for a store
\*      transaction still open, then fails;
\*   6. stop the workers: the pool finishes what it admitted and stops;
\*   7. close regolith (close_database);
\*   8. flush telemetry, handing the exporter join to the network runtime, or
\*      to a thread of its own when the runtime refuses it;
\*   9. drop the network runtime (NodeEdge::stop_owned).
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
\*   4. a write the node admitted commits after P2P began shutting down, so
\*      its head marker is never registered - INV_AdmittedWritesMarked. The
\*      one exception is a write that outlived the request drain's deadline:
\*      shutdown stops waiting for it, and its commit is past the promise;
\*   5. regolith's close waits for a transaction a caller left open
\*      - INV_CloseWaitsForNoTransaction;
\*   6. shutdown never finishes - LIVE_ShutdownTerminates.
\*
\* The Rust code each guard models:
\*   RedriveInSet    crates/defra-node/src/node_acp.rs gate_external_acp: the
\*                   redrive is a workers::spawn_background task the node pushes
\*                   into BackgroundTasks, instead of drop(network.spawn(..));
\*   AwaitServices   crates/edge/src/task.rs Service::stopped, called by
\*                   crates/defra-node/src/managed_p2p.rs stop_services;
\*   DrainDeadline   crates/edge/src/handle.rs Scope::drain (a drain that
\*                   waits up to `within`);
\*   SlotKeepsHandle crates/defra-node/src/lib.rs Node::shutdown, the
\*                   telemetry slot that keeps the handle when the runtime
\*                   refuses the flush;
\*   RequestsFirst   crates/defra-node/src/lib.rs Node::shutdown, which drains
\*                   the request scope (NodeEdge::drain_requests) before P2P,
\*                   with every HTTP request run as a task of it
\*                   (crates/http/src/server.rs Server::router) with no
\*                   deadline of its own and inside the request timeout, so
\*                   neither a client that goes away nor a timeout ends the
\*                   request's admission before its handler ends. Regression:
\*                   a_transaction_committed_during_shutdown_is_refused_or_keeps_its_head_marker
\*                   in crates/defra-node/tests/commit_during_shutdown.rs, and
\*                   crates/http/tests/request_admission.rs;
\*   CommitsAdmitted crates/defra-node/src/lib.rs Node::as_request, through
\*                   which begin_transaction (and with it
\*                   begin_transaction_guard), commit_transaction (and
\*                   NodeTransaction::commit), execute_as and the unsigned
\*                   execute_prepared_request run as tasks of the request
\*                   scope, as signed queries do, and with them the ops API's
\*                   Node::txn_begin and Node::txn_commit
\*                   (crates/defra-node/src/ops/request.rs). The raw runner()
\*                   is outside the promise, as are the ops that write no
\*                   document head (truncate_collection_as purges, and the
\*                   schema and lens ops);
\*   EndsOpenTransactions crates/defra-node/src/lib.rs Node::shutdown, which
\*                   ends the registry's transactions after the node's drain
\*                   and before close_database. Regression:
\*                   a_transaction_left_open_by_a_refused_commit_does_not_stall_shutdown
\*                   in crates/defra-node/tests/commit_during_shutdown.rs.

EXTENDS Naturals  \* numbers, for comparing step positions

CONSTANTS
  RedriveInSet,     \* TRUE: the redrive is held in the shutdown set, on the pool
  AwaitServices,    \* TRUE: shutdown waits for each stopped service to end
  DrainDeadline,    \* TRUE: each drain gives up on requests at its deadline
  SlotKeepsHandle,  \* TRUE: a refused flush keeps its handle for its own thread
  RequestsFirst,    \* TRUE: requests stop and drain before P2P shuts down
  CommitsAdmitted,  \* TRUE: in-process begins and commits are admitted as requests are
  EndsOpenTransactions \* TRUE: shutdown ends transactions left open

Tasks == {"redrive", "service"}  \* the two background tasks modeled

\* Where each task runs, so what stopping kills it when nobody stopped it first:
\* the pool's workers, or the network runtime. The old redrive ran detached on
\* the network runtime.
Host(t) == IF t = "service" THEN "edge"
           ELSE IF RedriveInSet THEN "pool" ELSE "edge"

\* Whether the node's shutdown set holds the redrive to abort and await it.
\* The service is held by P2P's shutdown instead.
InSet(t) == t = "redrive" /\ RedriveInSet

\* The shutdown steps, numbered in the order the code runs them. The request
\* drain always has its place; without RequestsFirst it closes nothing and
\* waits for nothing, so requests are still admitted while P2P shuts down.
Order == [Running |-> 1, Abort |-> 2, Await |-> 3, Requests |-> 4, P2P |-> 5,
          Drain |-> 6, EndTxns |-> 7, StopWorkers |-> 8, CloseStore |-> 9,
          Flush |-> 10, DropRuntime |-> 11, Done |-> 12]

\* The position of a step in that order.
Pos(p) == Order[p]

\* The step that should have ended task t: the wait for the node's own
\* tasks, or P2P's shutdown for the service.
StoppedBy(t) == IF t = "service" THEN "P2P" ELSE "Await"

VARIABLES
  phase,          \* which shutdown step the node is at
  status,         \* per task: "running", "stopping" (aborted) or "done"
  inOp,           \* per task: TRUE while it is in the middle of a storage call
  inflight,       \* how many requests are in flight (0 or 1)
  commits,        \* how many in-process writes are in flight (0 or 1)
  admitting,      \* TRUE while the node admits requests
  overdue,        \* TRUE once the request drain gave up on a write
  markerLost,     \* TRUE once a write committed with P2P going down
  p2pDown,        \* TRUE once P2P began shutting down
  deadlinePassed, \* TRUE once the current drain's deadline went by
  storeOpen,      \* TRUE until regolith closes
  txnOpen,        \* TRUE while a transaction a caller left open is in the registry
  closeWaited,    \* TRUE once regolith's close waited out its timeout for one
  flush,          \* telemetry: "pending", "thread" (on its own thread) or "done"
  callerJoined    \* TRUE if the shutdown caller ran the exporter join itself

taskVars == <<status, inOp>>                                    \* the tasks
reqVars == <<inflight, commits, admitting, overdue, markerLost>> \* requests
endVars == <<storeOpen, txnOpen, closeWaited, flush, callerJoined>> \* the end
vars == <<phase, taskVars, reqVars, p2pDown, deadlinePassed, endVars>> \* all

TypeOK ==
  /\ phase \in DOMAIN Order                              \* a known step
  /\ status \in [Tasks -> {"running", "stopping", "done"}] \* a known task state
  /\ inOp \in [Tasks -> BOOLEAN]                         \* a flag per task
  /\ inflight \in 0..1                                   \* at most one request
  /\ commits \in 0..1                                    \* at most one write
  /\ admitting \in BOOLEAN                               \* a flag
  /\ overdue \in BOOLEAN                                 \* a flag
  /\ markerLost \in BOOLEAN                              \* a flag
  /\ p2pDown \in BOOLEAN                                 \* a flag
  /\ deadlinePassed \in BOOLEAN                          \* a flag
  /\ storeOpen \in BOOLEAN                               \* a flag
  /\ txnOpen \in BOOLEAN                                 \* a flag
  /\ closeWaited \in BOOLEAN                             \* a flag
  /\ flush \in {"pending", "thread", "done"}             \* a known flush state
  /\ callerJoined \in BOOLEAN                            \* a flag

Init ==
  /\ phase = "Running"                       \* the node is serving
  /\ status = [t \in Tasks |-> "running"]    \* both tasks run
  /\ inOp = [t \in Tasks |-> FALSE]          \* neither is in a storage call
  /\ inflight = 0                            \* no request yet
  /\ commits = 0                             \* no in-process write yet
  /\ admitting = TRUE                        \* requests are admitted
  /\ overdue = FALSE                         \* no write was given up on
  /\ markerLost = FALSE                      \* no write lost its marker
  /\ p2pDown = FALSE                         \* P2P runs
  /\ deadlinePassed = FALSE                  \* no drain has started
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
  /\ UNCHANGED <<phase, status, reqVars, p2pDown, deadlinePassed, endVars>>

\* The storage call returns.
EndOp(t) ==
  /\ inOp[t]                                 \* it was inside storage
  /\ inOp' = [inOp EXCEPT ![t] = FALSE]      \* now it is out
  /\ UNCHANGED <<phase, status, reqVars, p2pDown, deadlinePassed, endVars>>

\* An aborted task reaches its next poll outside a storage call and ends.
Finish(t) ==
  /\ status[t] = "stopping"                  \* it was aborted
  /\ ~inOp[t]                                \* its current call is over
  /\ status' = [status EXCEPT ![t] = "done"] \* it has ended
  /\ UNCHANGED <<phase, inOp, reqVars, p2pDown, deadlinePassed, endVars>>

\* A request arrives while the node still admits them.
Arrive ==
  /\ admitting                               \* admission is open
  /\ inflight = 0                            \* keep the instance small
  /\ inflight' = 1                           \* one request in flight
  /\ UNCHANGED <<phase, taskVars, commits, admitting, overdue, markerLost,
                 p2pDown, deadlinePassed, endVars>>

\* An in-process write starts: admitted while the node admits requests, or,
\* unadmitted, any time the store is still open.
ArriveCommit ==
  /\ IF CommitsAdmitted THEN admitting       \* through the request scope
     ELSE storeOpen                          \* or straight to the runner
  /\ commits = 0                             \* keep the instance small
  /\ commits' = 1                            \* one write in flight
  /\ UNCHANGED <<phase, taskVars, inflight, admitting, overdue, markerLost,
                 p2pDown, deadlinePassed, endVars>>

\* The in-process write commits; its marker is registered only if P2P is not
\* yet going down.
CompleteCommit ==
  /\ commits = 1                             \* there is one
  /\ commits' = 0                            \* and it committed
  /\ markerLost' = (markerLost \/ p2pDown)   \* committed with no marker
  /\ UNCHANGED <<phase, taskVars, inflight, admitting, overdue, p2pDown,
                 deadlinePassed, endVars>>

\* A request completes, committing its write; its marker is registered only
\* if P2P is not yet going down. Nothing forces this: a request may hang.
Complete ==
  /\ inflight = 1                            \* there is one
  /\ inflight' = 0                           \* and it is done
  /\ markerLost' = (markerLost \/ p2pDown)   \* committed with no marker
  /\ UNCHANGED <<phase, taskVars, commits, admitting, overdue, p2pDown,
                 deadlinePassed, endVars>>

\* A caller begins an explicit transaction and leaves it open: admitted while
\* the node admits requests, or, unadmitted, any time the store is open.
BeginTxn ==
  /\ IF CommitsAdmitted THEN admitting ELSE storeOpen \* where it may begin
  /\ ~txnOpen                                \* keep the instance small
  /\ txnOpen' = TRUE                         \* it holds a store transaction
  /\ UNCHANGED <<phase, taskVars, reqVars, p2pDown, deadlinePassed, storeOpen,
                 closeWaited, flush, callerJoined>>

\* The telemetry thread of its own finishes the exporter join.
ThreadFlush ==
  /\ flush = "thread"                        \* the join was handed to it
  /\ flush' = "done"                         \* and it ran there
  /\ UNCHANGED <<phase, taskVars, reqVars, p2pDown, deadlinePassed, storeOpen,
                 txnOpen, closeWaited, callerJoined>>

\* Time passes during a drain until its deadline.
DeadlinePasses ==
  /\ phase \in {"Requests", "Drain"}         \* only a drain has a deadline
  /\ DrainDeadline                           \* and only if it sets one
  /\ ~deadlinePassed                         \* it passes once per drain
  /\ deadlinePassed' = TRUE                  \* now it has
  /\ UNCHANGED <<phase, taskVars, reqVars, p2pDown, endVars>>

\* ---- The shutdown sequence ----

\* Someone calls shutdown.
Begin ==
  /\ phase = "Running"                       \* the node was serving
  /\ phase' = "Abort"                        \* shutdown starts
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, endVars>>

\* Every task the node's shutdown set holds is aborted; a task it does not
\* hold keeps running.
Abort ==
  /\ phase = "Abort"                         \* this step
  /\ status' = [t \in Tasks |->              \* for each task:
                  IF InSet(t) /\ status[t] = "running"
                  THEN "stopping"            \* held: told to stop
                  ELSE status[t]]            \* not held: nobody tells it
  /\ phase' = "Await"                        \* next: wait for them
  /\ UNCHANGED <<inOp, reqVars, p2pDown, deadlinePassed, endVars>>

\* Shutdown waits until every task it holds has ended.
Await ==
  /\ phase = "Await"                         \* this step
  /\ \A t \in Tasks : InSet(t) => status[t] = "done" \* those ended
  /\ phase' = "Requests"                     \* next: the requests
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, endVars>>

\* Work the drains count: requests, and admitted in-process writes.
Pending == inflight + (IF CommitsAdmitted THEN commits ELSE 0)

\* The request drain stops admission and ends when nothing is in flight, or at
\* its deadline, giving up on the work still in flight. Without
\* RequestsFirst requests share the node's scope and nothing happens here.
Requests ==
  /\ phase = "Requests"                      \* this step
  /\ IF RequestsFirst
     THEN /\ Pending = 0 \/ deadlinePassed   \* drained, or out of time
          /\ admitting' = FALSE              \* no request admitted any more
          /\ overdue' = (overdue \/ Pending > 0) \* given up on, if any is left
     ELSE UNCHANGED <<admitting, overdue>>   \* requests keep coming
  /\ deadlinePassed' = FALSE                 \* the next drain has its own
  /\ phase' = "P2P"                          \* next: P2P
  /\ UNCHANGED <<taskVars, inflight, commits, markerLost, p2pDown, endVars>>

\* P2P's shutdown starts: no write registers its marker from here on, and the
\* service is told to stop.
P2PStart ==
  /\ phase = "P2P"                           \* this step
  /\ ~p2pDown                                \* it has not started
  /\ p2pDown' = TRUE                         \* P2P is going down
  /\ status' = [status EXCEPT                \* the service is aborted
                  !["service"] = IF @ = "running" THEN "stopping" ELSE @]
  /\ UNCHANGED <<phase, inOp, reqVars, deadlinePassed, endVars>>

\* P2P's shutdown returns, once the service ended if it waits for it.
P2PEnd ==
  /\ phase = "P2P"                           \* this step
  /\ p2pDown                                 \* it started
  /\ AwaitServices => status["service"] = "done" \* and the service is over
  /\ phase' = "Drain"                        \* next: the node's drain
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, endVars>>

\* The node's drain closes its scope, the request scope with it, and ends when
\* nothing is in flight, or when its deadline passed.
Drain ==
  /\ phase = "Drain"                         \* this step
  /\ Pending = 0 \/ deadlinePassed           \* drained, or out of time
  /\ admitting' = FALSE                      \* no request admitted any more
  /\ deadlinePassed' = FALSE                 \* the next drain has its own
  /\ phase' = "EndTxns"                      \* next: transactions left open
  /\ UNCHANGED <<taskVars, inflight, commits, overdue, markerLost, p2pDown,
                 endVars>>

\* Shutdown ends the transactions left open, while their workers still run.
EndTxns ==
  /\ phase = "EndTxns"                       \* this step
  /\ IF EndsOpenTransactions
     THEN txnOpen' = FALSE                   \* each one ended
     ELSE UNCHANGED txnOpen                  \* left in the registry
  /\ phase' = "StopWorkers"                  \* next: the pool
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, storeOpen,
                 closeWaited, flush, callerJoined>>

\* The pool stops. A background task still on it is dropped with its worker.
StopWorkers ==
  /\ phase = "StopWorkers"                   \* this step
  /\ status' = [t \in Tasks |->              \* each task the pool hosts ends
                  IF Host(t) = "pool" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "pool" THEN FALSE ELSE inOp[t]]
  /\ phase' = "CloseStore"                   \* next: regolith
  /\ UNCHANGED <<reqVars, p2pDown, deadlinePassed, endVars>>

\* Regolith closes, after waiting out its timeout for a transaction still
\* open.
CloseStore ==
  /\ phase = "CloseStore"                    \* this step
  /\ storeOpen' = FALSE                      \* storage is gone
  /\ closeWaited' = (closeWaited \/ txnOpen) \* waited out its timeout for one
  /\ phase' = "Flush"                        \* next: telemetry
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, txnOpen, flush,
                 callerJoined>>

\* The runtime runs the flush on one of its blocking threads.
FlushAccepted ==
  /\ phase = "Flush"                         \* this step
  /\ flush' = "done"                         \* flushed off the caller
  /\ phase' = "DropRuntime"                  \* next: the runtime
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, storeOpen,
                 txnOpen, closeWaited, callerJoined>>

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
  /\ UNCHANGED <<taskVars, reqVars, p2pDown, deadlinePassed, storeOpen,
                 txnOpen, closeWaited>>

\* The network runtime goes, and with it every task and request on it.
DropRuntime ==
  /\ phase = "DropRuntime"                   \* this step
  /\ status' = [t \in Tasks |->              \* each task the runtime hosts ends
                  IF Host(t) = "edge" THEN "done" ELSE status[t]]
  /\ inOp' = [t \in Tasks |->                \* and is out of storage
                IF Host(t) = "edge" THEN FALSE ELSE inOp[t]]
  /\ inflight' = 0                           \* a hung request is cancelled
  /\ commits' = IF CommitsAdmitted THEN 0 ELSE commits \* and an admitted write
  /\ phase' = "Done"                         \* shutdown returns
  /\ UNCHANGED <<admitting, overdue, markerLost, p2pDown, deadlinePassed,
                 endVars>>

\* Any one thing happens.
Next ==
  \/ \E t \in Tasks : BeginOp(t) \/ EndOp(t) \/ Finish(t)  \* a task acts
  \/ Arrive \/ Complete \/ ArriveCommit \/ CompleteCommit  \* requests, writes
  \/ BeginTxn                                             \* a transaction left open
  \/ ThreadFlush \/ DeadlinePasses                         \* time and threads
  \/ Begin \/ Abort \/ Await \/ Requests \/ P2PStart       \* shutdown steps
  \/ P2PEnd \/ Drain \/ EndTxns \/ StopWorkers \/ CloseStore
  \/ FlushAccepted \/ FlushRefused \/ DropRuntime

\* Fairness: shutdown is called and each of its steps is taken once possible;
\* a storage call returns; an aborted task reaches its next poll; time passes;
\* a thread runs. A request is not promised to complete.
Spec ==
  /\ Init                                             \* start serving
  /\ [][Next]_vars                                    \* then take steps
  /\ WF_vars(Begin)                                   \* shutdown is called
  /\ WF_vars(Abort) /\ WF_vars(Await) /\ WF_vars(Requests) \* steps proceed
  /\ WF_vars(P2PStart) /\ WF_vars(P2PEnd) /\ WF_vars(Drain)
  /\ WF_vars(EndTxns) /\ WF_vars(StopWorkers) /\ WF_vars(CloseStore)
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

\* Regolith's close never waits for a transaction a caller left open.
INV_CloseWaitsForNoTransaction == ~closeWaited

\* Shutdown always returns.
LIVE_ShutdownTerminates == <>(phase = "Done")

====
