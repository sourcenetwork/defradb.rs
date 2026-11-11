---- MODULE GuardedCommit ----
\* An owning transaction guard can be committed through the node's request
\* edge. The caller may stop waiting after the commit task has started. The
\* task continues independently of its waiter. If the guard stays with the
\* caller, dropping that waiter abandons the transaction even while the task
\* is still trying to commit it. Moving the guard to the task instead leaves
\* it owned until the commit finishes while its edge keeps running. A node
\* without an edge executes the commit inline and follows its executor's
\* cancellation semantics.
\*
\* State correspondence:
\*   owner   the live TransactionGuard's handle, in NodeTransaction::commit
\*           or in its admitted commit task (crates/defra-node/src/transaction.rs);
\*   waiter  the caller's pending commit future (edge::Task completion);
\*   txn     the transaction's state in the registry: present, committed, or
\*           abandoned by QueryExecutor::abandon_txn.
\* TransferGuard is true when the admitted task owns the guard.
CONSTANT TransferGuard
VARIABLES owner, waiter, txn
vars == <<owner, waiter, txn>>

\* A caller owns an open transaction and has not started its commit.
Init == /\ owner = "caller" /\ waiter = FALSE /\ txn = "open"

\* Starting the commit either moves the guard into the task or leaves it with
\* the caller, according to the guard being checked.
Start ==
  /\ owner = "caller" /\ ~waiter /\ txn = "open"
  /\ owner' = IF TransferGuard THEN "task" ELSE "caller"
  /\ waiter' = TRUE
  /\ UNCHANGED txn

\* The caller drops its future. A guard it still holds abandons the open
\* transaction; a guard in the task remains there.
Cancel ==
  /\ waiter /\ txn = "open"
  /\ waiter' = FALSE
  /\ IF owner = "caller"
     THEN /\ owner' = "none" /\ txn' = "abandoned"
     ELSE /\ UNCHANGED <<owner, txn>>

\* The still-owned commit completes and its guard is disarmed.
Finish ==
  /\ txn = "open" /\ (waiter \/ owner = "task")
  /\ txn' = "committed" /\ owner' = "none" /\ waiter' = FALSE

Next == Start \/ Cancel \/ Finish
Spec == Init /\ [][Next]_vars

\* A started commit is not abandoned when its caller stops waiting.
INV_CommitNotAbandoned == txn # "abandoned"
====
