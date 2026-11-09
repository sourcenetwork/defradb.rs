---- MODULE PatchInTxn ----
\* The story: a client opens an explicit transaction, may write in it first
\* (add a document, add a collection), then sends a collection patch in that
\* transaction, and finally commits or discards it. Meanwhile another client
\* may change the same collection definitions and commit first.
\*
\* The patch is staged inside the client's transaction: it reads the
\* definitions and documents through that transaction, so it sees the
\* client's own earlier writes, and it writes into that transaction, so no one
\* else sees it until the commit. If another commit changed what the patch
\* read, the client's commit fails with a conflict, which goes back to the
\* client; nothing retries it behind the client's back. The work a patch owes
\* after it lands (registering migrations, building owed indexes) runs only
\* once the commit is durable, and never for a discarded transaction.
\*
\* What must never happen: another client seeing the patch before the commit;
\* the post-commit work running for a patch that never landed; a conflict
\* hidden from the client by a silent retry; a patch deciding on a view that
\* misses the client's own earlier write in the same transaction.
\*
\* Code it models:
\*   crates/db/src/txn/registry/lifecycle.rs  patch_collection_as (routes a
\*                                             request that carries a txn)
\*   crates/db/src/txn/registry/schemas.rs    patch_collection_in_txn_as
\*   crates/db/src/definition/patch/stage.rs  DB::stage_patch (reads through
\*                                             the txn), DB::finish_staged
\*   crates/db/src/collection/ops/version.rs  DB::collection_has_data (a
\*                                             validated scan in the txn)
EXTENDS Naturals

CONSTANTS
  ReadThroughTxn,  \* the patch reads through the client's transaction
  WriteThroughTxn, \* the patch writes into the client's transaction
  FinishOnCommit,  \* post-commit work waits for the commit to be durable
  RetryHides       \* a conflict is retried silently instead of reported

\* Each switch is on or off.
ASSUME WriteThroughTxn \in BOOLEAN
ASSUME ReadThroughTxn \in BOOLEAN /\ FinishOnCommit \in BOOLEAN /\ RetryHides \in BOOLEAN

VARIABLES
  earlier,    \* the client wrote something earlier in its transaction
  sawEarlier, \* the patch's decision saw that earlier write
  staged,     \* the patch is staged in the client's transaction
  rival,      \* another client committed a change to what the patch read
  outcome,    \* "open", "committed", "conflict" or "discarded"
  visible,    \* other clients can see the patch
  finished    \* the post-commit work has run

vars == <<earlier, sawEarlier, staged, rival, outcome, visible, finished>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* the flags are true or false
  /\ earlier \in BOOLEAN /\ sawEarlier \in BOOLEAN /\ staged \in BOOLEAN
  /\ rival \in BOOLEAN /\ visible \in BOOLEAN /\ finished \in BOOLEAN
  \* the transaction ends one of three ways, or is still open
  /\ outcome \in {"open", "committed", "conflict", "discarded"}

\* A fresh transaction: nothing written, nothing staged, nobody else moved.
Init ==
  /\ earlier = FALSE /\ sawEarlier = FALSE /\ staged = FALSE
  /\ rival = FALSE /\ outcome = "open" /\ visible = FALSE /\ finished = FALSE

\* The client writes something in its transaction before the patch.
WriteEarlier ==
  \* only while the transaction is open and before the patch
  /\ outcome = "open" /\ ~staged /\ ~earlier
  /\ earlier' = TRUE
  /\ UNCHANGED <<sawEarlier, staged, rival, outcome, visible, finished>>

\* The patch is staged. Reading through the transaction it sees the earlier
\* write; reading outside it, it does not. If post-commit work does not wait
\* for the commit, it runs right here.
Stage ==
  \* only once, while the transaction is open
  /\ outcome = "open" /\ ~staged
  /\ staged' = TRUE
  \* what the patch's decision saw of the client's own earlier write
  /\ sawEarlier' = (earlier /\ ReadThroughTxn)
  \* the bug a RED plants: the follow-up work runs before any commit
  /\ finished' = (~FinishOnCommit)
  \* the bug a RED plants: the definition write lands outside the transaction
  /\ visible' = (~WriteThroughTxn)
  /\ UNCHANGED <<earlier, rival, outcome>>

\* Another client commits a change to the definitions the patch read.
RivalCommits ==
  \* only after the patch read, while the client's transaction is still open
  /\ outcome = "open" /\ staged /\ ~rival
  /\ rival' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, outcome, visible, finished>>

\* The client commits. A rival commit on what the patch read fails it with a
\* conflict, unless a silent retry (the RED) lands it anyway.
Commit ==
  \* only an open transaction holding the patch
  /\ outcome = "open" /\ staged
  /\ IF rival /\ ~RetryHides
       \* the conflict goes back to the client, and nothing lands
       THEN /\ outcome' = "conflict" /\ UNCHANGED <<visible, finished>>
       \* the patch lands, everyone can see it, and the follow-up work runs
       ELSE /\ outcome' = "committed" /\ visible' = TRUE /\ finished' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival>>

\* The client discards its transaction: nothing lands.
Discard ==
  \* only an open transaction
  /\ outcome = "open"
  /\ outcome' = "discarded"
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, visible, finished>>

\* Any of the steps can happen next.
Next == WriteEarlier \/ Stage \/ RivalCommits \/ Commit \/ Discard

\* A finished transaction stays finished.
Done == outcome # "open" /\ UNCHANGED vars

\* The client's transaction runs its steps, then rests.
Spec == Init /\ [][Next \/ Done]_vars

\* No one else sees the patch unless the client's commit landed.
INV_IsolatedUntilCommit == visible => outcome = "committed"

\* The post-commit work runs only for a patch whose commit landed.
INV_FinishOnlyAfterCommit == finished => outcome = "committed"

\* A rival commit on what the patch read always reaches the client as a
\* conflict (or the client discarded first): it never lands silently.
INV_ConflictReachesCaller == (rival /\ staged) => outcome # "committed"

\* The patch decides on a view that holds the client's own earlier write.
INV_SeesOwnWrites == (staged /\ earlier) => sawEarlier
====
