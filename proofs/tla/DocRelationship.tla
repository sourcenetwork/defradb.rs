---- MODULE DocRelationship ----
\* The story: a client asks a node to add or delete a document actor
\* relationship. It may arrive through the embedded node or through the HTTP
\* server `defradb start` runs, and either outside a transaction (autocommit)
\* or in the transaction its request names. Every route must run one ordered
\* set of checks and answer alike.
\*
\* The checks are Go's, stopping at the first refusal:
\*   1. nac        the caller may change relationships;
\*   2. acp        the node runs document ACP;
\*   3. get        the caller may read collections;
\*   4. name       a collection is named;
\*   5. collection the collection exists and has a policy;
\*   6. args       the identity, document, relation and target are given;
\*   7. owner      the relation is not the owner relation;
\*   8. target     the target parses;
\*   9. policy     the policy, its resource and its relation are found.
\* Only when every check passes is the change made. A grant that lands then
\* announces the document's update.
\*
\* Who acts: each way in and each transaction route moves through the same
\* checks independently. The "ok" answers are chosen up front, so the model
\* explores every combination.
\*
\* What must never happen: a relationship changes although a check should
\* refuse it; a refusal names anything other than the first failing check in
\* Go's order; an update is announced for a grant that did not land; or a
\* route answers the same request differently.
\*
\* Code it models:
\*   crates/database/db/src/access/dac/client/mod.rs DocumentRelationships::add_as,
\*     delete_as, change_as, apply
\*   crates/database/db/src/access/dac/client/checks.rs DocumentRelationships::requestor,
\*     requestor_in_txn, collection_policy, relationship_target
\*   crates/database/db/src/access/dac/client/in_txn.rs DocumentRelationships::autocommit,
\*     in_txn and AutocommitTxn
\*   crates/database/db/src/access/dac/client/settle.rs publish_document_update
\*   crates/entry-points/defra-node/src/acp_ops.rs DbAcpOps and
\*     crates/entry-points/cli/src/doc_acp_adapter.rs DocumentAcpAdapter
EXTENDS Naturals, Sequences

CONSTANTS
  Skipped,      \* checks every route leaves out (empty when all run)
  Swapped,      \* every route asks for arguments and owner before the collection
  ServerCopy,   \* the start server runs its own old copy instead of the core
  EventFirst,   \* an update is announced before its change is known to land
  TxnSwapped    \* only transaction routes use the swapped check order

\* Go's checks, in the order Go runs them.
GoOrder == <<"nac", "acp", "get", "name", "collection", "args", "owner", "target", "policy">>

\* The wrong order where arguments and the owner relation come before collection.
SwappedOrder == <<"nac", "acp", "get", "name", "args", "owner", "collection", "target", "policy">>

\* The start server's old private copy: no ACP or get-collection checks.
OldServerOrder == <<"nac", "args", "owner", "collection", "target", "policy">>

\* Every check can be selected as omitted; the route switches are booleans.
AllChecks == {GoOrder[i] : i \in 1..Len(GoOrder)}
ASSUME Skipped \subseteq AllChecks
ASSUME {Swapped, ServerCopy, EventFirst, TxnSwapped} \subseteq BOOLEAN

\* The two ways a request enters a node.
Edges == {"node", "server"}
\* The two transaction routes a request can take.
Routes == {"autocommit", "txn"}
\* Every request route the model follows.
Paths == Edges \X Routes

\* The checks the core runs on a route, with any configured omission or swap.
CoreOrder(route) ==
  SelectSeq(
    IF Swapped \/ (route = "txn" /\ TxnSwapped) THEN SwappedOrder ELSE GoOrder,
    LAMBDA check : check \notin Skipped)

\* The checks a way in runs; only the old server copy differs from the core.
Order(edge, route) ==
  IF edge = "server" /\ ServerCopy THEN OldServerOrder ELSE CoreOrder(route)

\* The step just past the checks, then the change and its announcement.
ApplyStep(path) == Len(Order(path[1], path[2])) + 1
PublishStep(path) == Len(Order(path[1], path[2])) + 2
DoneStep(path) == Len(Order(path[1], path[2])) + 3

\* The first check in Go's order whose answer is false, or "none".
GoRefusal(answers) ==
  IF \E i \in 1..Len(GoOrder) : ~answers[GoOrder[i]]
    THEN GoOrder[CHOOSE i \in 1..Len(GoOrder) :
                   /\ ~answers[GoOrder[i]]
                   /\ \A j \in 1..(i - 1) : answers[GoOrder[j]]]
    ELSE "none"

VARIABLES
  ok,       \* ok[check] says the check passes; ok["apply"] says the provider accepts
  step,     \* step[path] is how far that request has moved
  changed,  \* changed[path] says the relationship changed
  refusal,  \* refusal[path] names the refusing check, provider, or "none"
  event     \* event[path] says a document update was announced

vars == <<ok, step, changed, refusal, event>>

\* Every key an "ok" answer is stored under.
Keys == AllChecks \cup {"apply"}

\* Every variable has the shape its role requires.
TypeOK ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ \A path \in Paths : step[path] \in 1..DoneStep(path)
  /\ changed \in [Paths -> BOOLEAN]
  /\ refusal \in [Paths -> AllChecks \cup {"apply", "none"}]
  /\ event \in [Paths -> BOOLEAN]

\* Any mix of answers can arrive and every route starts before its first check.
Init ==
  /\ ok \in [Keys -> BOOLEAN]
  /\ step = [path \in Paths |-> 1]
  /\ changed = [path \in Paths |-> FALSE]
  /\ refusal = [path \in Paths |-> "none"]
  /\ event = [path \in Paths |-> FALSE]

\* A route runs its next check; a failing check ends that request.
RunCheck(path) ==
  /\ step[path] \in 1..Len(Order(path[1], path[2]))
  /\ LET check == Order(path[1], path[2])[step[path]] IN
     IF ~ok[check]
       THEN /\ refusal' = [refusal EXCEPT ![path] = check]
            /\ step' = [step EXCEPT ![path] = DoneStep(path)]
       ELSE /\ step' = [step EXCEPT ![path] = step[path] + 1]
            /\ UNCHANGED refusal
  /\ UNCHANGED <<ok, changed, event>>

\* Document ACP makes the change or refuses it; EventFirst announces too early.
Apply(path) ==
  /\ step[path] = ApplyStep(path)
  /\ IF ok["apply"]
       THEN /\ changed' = [changed EXCEPT ![path] = TRUE]
            /\ step' = [step EXCEPT ![path] = PublishStep(path)]
            /\ UNCHANGED refusal
       ELSE /\ refusal' = [refusal EXCEPT ![path] = "apply"]
            /\ step' = [step EXCEPT ![path] = DoneStep(path)]
            /\ UNCHANGED changed
  /\ event' = IF EventFirst THEN [event EXCEPT ![path] = TRUE] ELSE event
  /\ UNCHANGED ok

\* A landed change announces the document update; a refusal announces nothing.
Publish(path) ==
  /\ step[path] = PublishStep(path)
  /\ event' = [event EXCEPT ![path] = changed[path]]
  /\ step' = [step EXCEPT ![path] = DoneStep(path)]
  /\ UNCHANGED <<ok, changed, refusal>>

\* Any unfinished route takes its next step.
Next == \E path \in Paths : RunCheck(path) \/ Apply(path) \/ Publish(path)

\* Every finished route stays finished.
Done == (\A path \in Paths : step[path] = DoneStep(path)) /\ UNCHANGED vars

Spec == Init /\ [][Next \/ Done]_vars

\* No relationship changes unless every check would have passed.
INV_NoChangeUnlessAllChecksPass ==
  \A path \in Paths : changed[path] => \A check \in AllChecks : ok[check]

\* Every refusal on a route names the first failing check in Go's order.
INV_GoOrder ==
  \A path \in Paths : refusal[path] \in AllChecks => refusal[path] = GoRefusal(ok)

\* An update is announced only for a change that landed.
INV_EventOnlyIfChanged ==
  \A path \in Paths : event[path] => changed[path]

\* Once all routes finish, the embedded node, server, autocommit and transaction
\* routes gave the same answer.
INV_SameAnswer ==
  (\A path \in Paths : step[path] = DoneStep(path)) =>
    LET first == <<"node", "autocommit">> IN
      /\ \A path \in Paths : refusal[path] = refusal[first]
      /\ \A path \in Paths : changed[path] = changed[first]
====
