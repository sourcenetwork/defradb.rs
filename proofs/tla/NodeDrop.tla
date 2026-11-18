---- MODULE NodeDrop ----
\* The story, in plain words.
\*
\* A node runs background tasks on its own worker pool: the vector rebuild,
\* the downsample refresh, the parked-block replay, the Vera redrive and,
\* when it serves HTTP, the idle-transaction sweep. Each task holds the
\* database, and the database holds the pool, so while any task runs the
\* pool's threads and the store's files stay alive.
\*
\* A caller may end the node two ways:
\*   * Node::shutdown (crates/defra-node/src/lib.rs) aborts and awaits every
\*     task, then closes the store (see NodeShutdown.tla);
\*   * dropping the node, its last handle released, without a shutdown.
\* A task stops only when someone aborts it (workers::Background::abort);
\* dropping its handle leaves it running (crates/workers/src/pool/background.rs).
\* The node's shutdown set (BackgroundTasks, crates/defra-node/src/
\* background_tasks.rs) aborts every task it holds when it is dropped. The
\* abort only sets a flag: the task ends at its next poll on its own worker,
\* and the thread that dropped the node waits for nothing, so a drop on a
\* worker thread cannot wait on its own worker.
\*
\* What must never happen:
\*   1. a node dropped without shutdown keeps its pool and store alive
\*      forever, because a task it started still holds the database
\*      - LIVE_DroppedNodeReleasesPool;
\*   2. the thread that drops the node waits for a task to end
\*      - INV_DropNeverWaits;
\*   3. a shutdown returns while a task still runs
\*      - INV_ShutdownStopsEveryTask.
\*
\* The Rust code each guard models:
\*   SweepInSet   crates/defra-node/src/assemble.rs assemble: the transaction
\*                sweep (DbTransactionRegistry::start_stale_transaction_cleanup)
\*                is pushed into the node's BackgroundTasks, instead of a slot
\*                of its own whose drop left it running. Regression:
\*                a_dropped_http_node_releases_its_threads_and_descriptors in
\*                crates/defra-node/tests/node_drop.rs;
\*   AbortOnDrop  crates/defra-node/src/background_tasks.rs BackgroundTasks::drop,
\*                which aborts every task it holds;
\*   DropAwaits   FALSE in the code: BackgroundTasks::drop only aborts, it
\*                never awaits a task.

CONSTANTS
  SweepInSet,   \* TRUE: the sweep is held in the node's shutdown set
  AbortOnDrop,  \* TRUE: dropping the shutdown set aborts every task it holds
  DropAwaits    \* TRUE: the drop waits for each aborted task to end

Tasks == {"sweep", "rebuild"}  \* the sweep, and one task always in the set

\* Whether the node's shutdown set holds task t. The rebuild always is; the
\* sweep only with SweepInSet. Either way Node::shutdown stops it.
InSet(t) == t = "rebuild" \/ SweepInSet

VARIABLES
  node,       \* "live", "shutting" (shutdown running), "shut" or "dropped"
  status,     \* per task: "running", "stopping" (aborted) or "done"
  poolUp,     \* TRUE while the pool's threads run and the store is open
  dropWaited  \* TRUE once the dropping thread waited for a task

vars == <<node, status, poolUp, dropWaited>>  \* every variable

TypeOK ==
  /\ node \in {"live", "shutting", "shut", "dropped"}          \* a known stage
  /\ status \in [Tasks -> {"running", "stopping", "done"}]     \* a known task state
  /\ poolUp \in BOOLEAN                                        \* a flag
  /\ dropWaited \in BOOLEAN                                    \* a flag

Init ==
  /\ node = "live"                          \* the node serves
  /\ status = [t \in Tasks |-> "running"]   \* both tasks run
  /\ poolUp = TRUE                          \* the pool runs
  /\ dropWaited = FALSE                     \* nobody waited in a drop

\* The caller begins a shutdown: every task is aborted, the sweep whether it
\* sits in the set or in a slot of its own.
Shutdown ==
  /\ node = "live"                                         \* not ended yet
  /\ node' = "shutting"                                    \* shutdown runs
  /\ status' = [t \in Tasks |->                            \* each task:
                  IF status[t] = "running" THEN "stopping" \* told to stop
                  ELSE status[t]]                          \* or already over
  /\ UNCHANGED <<poolUp, dropWaited>>

\* Shutdown returns once it awaited every task.
ShutdownReturns ==
  /\ node = "shutting"                          \* shutdown is running
  /\ \A t \in Tasks : status[t] = "done"        \* every task ended
  /\ node' = "shut"                             \* it returns
  /\ UNCHANGED <<status, poolUp, dropWaited>>

\* The last handle to the node is released, after a shutdown or without one.
\* Dropping the shutdown set aborts what it holds when AbortOnDrop; a task it
\* does not hold is left running. With DropAwaits the dropping thread waits.
Drop ==
  /\ node \in {"live", "shut"}                             \* not mid-shutdown
  /\ node' = "dropped"                                     \* the handle is gone
  /\ status' = [t \in Tasks |->                            \* each task:
                  IF AbortOnDrop /\ InSet(t) /\ status[t] = "running"
                  THEN "stopping"                          \* told to stop
                  ELSE status[t]]                          \* or left as it is
  /\ dropWaited' = (dropWaited \/ DropAwaits)              \* waited, if it does
  /\ UNCHANGED poolUp

\* An aborted task reaches its next poll on its worker and ends, letting go
\* of the database.
Finish(t) ==
  /\ poolUp                                   \* its worker still runs
  /\ status[t] = "stopping"                   \* it was aborted
  /\ status' = [status EXCEPT ![t] = "done"]  \* it has ended
  /\ UNCHANGED <<node, poolUp, dropWaited>>

\* Once the node is gone and no task holds the database, its last handle is
\* released: the pool's threads exit and the store's files close.
Release ==
  /\ poolUp                                   \* still up
  /\ node = "dropped"                         \* the node let go
  /\ \A t \in Tasks : status[t] = "done"      \* and so did every task
  /\ poolUp' = FALSE                          \* everything is released
  /\ UNCHANGED <<node, status, dropWaited>>

\* Any one thing happens.
Next ==
  \/ Shutdown \/ ShutdownReturns \/ Drop \/ Release  \* the caller and the pool
  \/ \E t \in Tasks : Finish(t)                      \* a task ends

\* Fairness: an aborted task reaches its next poll; a pool nobody holds is
\* released; a shutdown that can return does. Whether and when the caller
\* shuts down or drops the node is the caller's choice.
Spec ==
  /\ Init                                           \* start serving
  /\ [][Next]_vars                                  \* then take steps
  /\ \A t \in Tasks : WF_vars(Finish(t))            \* tasks move
  /\ WF_vars(Release) /\ WF_vars(ShutdownReturns)   \* the pool and shutdown move

\* ---- What must hold ----

\* The thread that drops the node never waits for a task.
INV_DropNeverWaits == ~dropWaited

\* A shutdown that returned left no task running.
INV_ShutdownStopsEveryTask ==
  node = "shut" => \A t \in Tasks : status[t] = "done"

\* A node dropped, with or without a shutdown first, eventually releases its
\* pool's threads and its store.
LIVE_DroppedNodeReleasesPool == (node = "dropped") ~> ~poolUp

====
