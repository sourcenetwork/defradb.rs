---- MODULE SignedRequestDrain ----
\* A signed query is admitted on the request edge and sends its write to a
\* worker. Dropping the worker completion after it starts does not stop that
\* worker. The request edge counts the query task until it ends. A task with a
\* deadline can be aborted while its worker is still writing, releasing that
\* count. Shutdown drains the count before stopping P2P, which the committed
\* write needs to register its head marker. This model explores that race
\* before the request-drain deadline; the bounded-drain and crash boundaries
\* remain in NodeShutdown and SyncOwnership.
\*
\* State correspondence:
\*   task       the request scope permit held by Edge::spawn in
\*              crates/defra-node/src/signed_query/mod.rs;
\*   worker     the task already started by workers::run_on in
\*              crates/query/src/pooled/mod.rs and its committed write;
\*   p2pUp      the coordinator before ManagedP2PSystem::stop_transport;
\*   marked     the durable retry marker stored for that committed head.
\* NoTaskDeadline models Task::within(Duration::MAX). TransformAdmitted models
\* the request layer around routes created by the router transform. The red
\* configurations break each guard separately.
EXTENDS Naturals

CONSTANTS
  NoTaskDeadline, \* TRUE when the signed query has no task deadline
  TransformAdmitted \* TRUE when routes the router transform adds are admitted
VARIABLES task, worker, p2pUp, marked
vars == <<task, worker, p2pUp, marked>>

\* Initially no request ran and the transport is available.
Init ==
  /\ task = "idle" /\ worker = "idle"
  /\ p2pUp = TRUE /\ marked = FALSE

\* A query is admitted and its worker starts while P2P is running.
Start ==
  /\ task = "idle" /\ p2pUp
  /\ task' = "running" /\ worker' = "running"
  /\ UNCHANGED <<p2pUp, marked>>

\* A route the router transform adds starts a write: its handler is counted
\* only if the admission layer wraps the result of the transform.
StartTransform ==
  /\ task = "idle" /\ p2pUp
  /\ task' = IF TransformAdmitted THEN "running" ELSE "ended"
  /\ worker' = "running"
  /\ UNCHANGED <<p2pUp, marked>>

\* The task deadline aborts its waiter, not work the pool already started.
Expire ==
  /\ ~NoTaskDeadline /\ task = "running" /\ worker = "running"
  /\ task' = "ended"
  /\ UNCHANGED <<worker, p2pUp, marked>>

\* Shutdown stops P2P once the request scope has no admitted query.
Stop ==
  /\ task # "running" /\ p2pUp
  /\ p2pUp' = FALSE
  /\ UNCHANGED <<task, worker, marked>>

\* The worker commits, registering the marker only while P2P is up.
Finish ==
  /\ worker = "running"
  /\ worker' = "committed" /\ marked' = p2pUp
  /\ task' = "ended"
  /\ UNCHANGED p2pUp

Next == Start \/ StartTransform \/ Expire \/ Stop \/ Finish
Spec == Init /\ [][Next]_vars

\* Every completed worker write has its durable head marker.
INV_CommittedMarked == worker = "committed" => marked
====
