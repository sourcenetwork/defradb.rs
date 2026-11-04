---- MODULE DetachedBegin ----
\* A caller begins a transaction on the node's request edge. Once admitted,
\* the begin runs even when its caller stops waiting. If it finishes after
\* the caller left, its returned handle must abandon that transaction when
\* the unobserved result drops; otherwise the registry retains a transaction
\* whose handle nobody received. The handle's guard is disarmed when the
\* caller does receive it.
\*
\* State correspondence:
\*   phase    admitted edge task before it starts, running, or completed;
\*   waiter   caller's still-live Task completion;
\*   txn      DbTransactionRegistry state: absent, open, or abandoned;
\*   owner    the returned handle in the task completion, held by the caller,
\*            or absent when it was dropped.
\* GuardResult means an unobserved begin result abandons its handle on drop.
CONSTANT GuardResult
VARIABLES phase, waiter, txn, owner
vars == <<phase, waiter, txn, owner>>

\* No request or transaction exists yet.
Init == /\ phase = "idle" /\ waiter = FALSE /\ txn = "absent" /\ owner = "none"

\* The caller submits a begin and waits for it.
Start ==
  /\ phase = "idle"
  /\ phase' = "running" /\ waiter' = TRUE
  /\ UNCHANGED <<txn, owner>>

\* The caller drops the completion while the begin is still running.
Cancel ==
  /\ waiter
  /\ phase \in {"running", "ended"}
  /\ waiter' = FALSE
  /\ IF phase = "ended"
     THEN /\ owner' = "none"
          /\ txn' = IF GuardResult THEN "abandoned" ELSE "open"
     ELSE /\ UNCHANGED <<txn, owner>>
  /\ UNCHANGED phase

\* Begin finishes. Its result goes to a waiting caller; when nobody waits,
\* dropping the guarded result abandons the transaction it just opened.
Finish ==
  /\ phase = "running"
  /\ phase' = "ended"
  /\ IF waiter
     THEN /\ txn' = "open" /\ owner' = "completion"
     ELSE /\ txn' = IF GuardResult THEN "abandoned" ELSE "open"
          /\ owner' = "none"
  /\ UNCHANGED waiter

\* The caller receives and takes the handle from the completed task.
Receive ==
  /\ phase = "ended" /\ waiter /\ owner = "completion"
  /\ waiter' = FALSE /\ owner' = "caller"
  /\ UNCHANGED <<phase, txn>>

Next == Start \/ Cancel \/ Finish \/ Receive
Spec == Init /\ [][Next]_vars

\* An open transaction still has a handle in the completion or caller.
INV_OpenHasOwner == txn = "open" => owner # "none"
====
