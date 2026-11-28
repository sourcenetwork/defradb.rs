---- MODULE PostCommitFence ----
\* The story: a client's commit lands, but the client goes away before the
\* answer. Its post-commit work still runs on the owner and opens transactions.
\* The database has fenced the client's commit, and may have fenced other
\* commits too. Work from this commit must not wait on its own fence: that
\* would deadlock the owner. It must still wait for every other fenced commit
\* to land before taking a snapshot.
\*
\* Commit 1 is the commit whose post-commit work reads. Commit 2 is another
\* client's commit that may be fenced at the same time.
\*
\* What must never happen: post-commit work reads before its own commit lands,
\* or reads while another fenced commit is still landing.

EXTENDS Naturals

CONSTANT WaitOthers
\* True in the implementation: other fenced commits are awaited.

ASSUME WaitOthers \in BOOLEAN

VARIABLES
  landed,       \* whether each commit is durable
  fenced,       \* whether each commit still holds its cancellation fence
  wasFenced,    \* whether each commit held a fence at any point before the read
  read,         \* whether post-commit work has taken its transaction snapshot
  waited,       \* the other fences post-commit work waited for
  snapshot      \* the durable commits its snapshot contains

vars == <<landed, fenced, wasFenced, read, waited, snapshot>>
\* Every variable, for stuttering steps.

TypeOK ==
  \* each commit has a boolean landing and fence state
  /\ landed \in [1..2 -> BOOLEAN] /\ fenced \in [1..2 -> BOOLEAN]
  /\ wasFenced \in [1..2 -> BOOLEAN]
  \* the read, its waited-for fences and its snapshot have set shapes
  /\ read \in BOOLEAN /\ waited \in SUBSET {1, 2} /\ snapshot \in SUBSET {1, 2}

Init ==
  \* nothing has landed, no fence is held, and no snapshot exists
  /\ landed = [c \in 1..2 |-> FALSE]
  /\ fenced = [c \in 1..2 |-> FALSE]
  /\ wasFenced = [c \in 1..2 |-> FALSE]
  /\ read = FALSE /\ waited = {} /\ snapshot = {}

Fence(c) ==
  \* only a commit that has not landed can gain a fence
  /\ ~landed[c] /\ ~fenced[c]
  \* the caller stopped waiting, so this commit is now fenced
  /\ fenced' = [fenced EXCEPT ![c] = TRUE]
  /\ wasFenced' = [wasFenced EXCEPT ![c] = TRUE]
  \* nothing else moves
  /\ UNCHANGED <<landed, read, waited, snapshot>>

Land(c) ==
  \* only a fenced commit is modeled landing
  /\ fenced[c]
  \* it is now durable and no longer holds its fence
  /\ landed' = [landed EXCEPT ![c] = TRUE]
  /\ fenced' = [fenced EXCEPT ![c] = FALSE]
  \* nothing else moves
  /\ UNCHANGED <<wasFenced, read, waited, snapshot>>

PostCommitRead ==
  \* post-commit work runs only after commit 1 is durable and only once
  /\ landed[1] /\ ~read
  \* the bug a RED plants: it does not wait for another fenced commit
  /\ (WaitOthers => ~fenced[2])
  \* it records the other fences it waited for and snapshots durable commits
  /\ read' = TRUE
  /\ waited' = IF wasFenced[2] THEN {2} ELSE {}
  /\ snapshot' = {c \in 1..2 : landed[c]}
  \* the fences themselves do not move
  /\ UNCHANGED <<landed, fenced, wasFenced>>

Next ==
  \* any commit can gain a fence
  \/ \E c \in 1..2 : Fence(c)
  \* any fenced commit can land
  \/ \E c \in 1..2 : Land(c)
  \* post-commit work can read when its guards allow it
  \/ PostCommitRead

Spec == Init /\ [][Next]_vars
\* The model runs those steps forever.

INV_ReadAfterOwnCommit == read => landed[1]
\* Post-commit work never reads before its own commit is durable.

INV_WaitsForEveryOtherFence == read => waited \subseteq snapshot
\* Every other fence that was outstanding at the read is in its snapshot.
====
