---- MODULE PatchInTxn ----
\* The story: a client opens an explicit transaction, may write in it first
\* (add a document, add a collection) or read a collection (which caches that
\* collection's definition in the transaction), then sends a collection patch
\* in that transaction, and finally commits or discards it. Meanwhile another
\* client may change the same collection definitions and commit first.
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
\* The node-access (NAC) check that lets the client patch is read in the same
\* transaction, so an administrator who revokes that permission and commits
\* before the client does makes the client's commit fail with a conflict.
\*
\* A patch that turns another collection's relation into a one-to-one owes that
\* collection a unique index. The index is staged in the same transaction as
\* the patch, so it lands with the patch or not at all; there is no second
\* commit after the patch that could leave it visible without its index.
\*
\* A patch can fail part way (a later collection may not be removed). It runs
\* as one statement behind a savepoint, so a failure puts the transaction back
\* as it was before the patch. A patch that succeeds drops every definition it
\* changed from the transaction's own cache, so the client's later requests in
\* the transaction see the patched definition.
\*
\* What must never happen: another client seeing the patch before the commit;
\* the post-commit work running for a patch that never landed; a conflict
\* hidden from the client by a silent retry; a patch deciding on a view that
\* misses the client's own earlier write in the same transaction; half of a
\* failed patch left in the transaction; a later request in the transaction
\* using the definition from before the patch; a patch landing after the
\* permission that allowed it was revoked; a landed patch without the
\* one-to-one unique index it owes.
\*
\* Code it models:
\*   crates/db/src/txn/registry/lifecycle.rs  patch_collection_as (routes a
\*                                             request that carries a txn)
\*   crates/db/src/txn/registry/schemas.rs    patch_collection_in_txn_as (the
\*                                             NAC check in the txn, the
\*                                             statement savepoint),
\*                                             stage_patch_in_txn (drops the
\*                                             touched names from the catalog)
\*   crates/db/src/definition/patch/stage.rs  DB::stage_patch (reads through
\*                                             the txn), DB::stage_groups (stages
\*                                             the owed one-to-one index after
\*                                             every prepared write),
\*                                             StagedPatch::touched_names,
\*                                             DB::finish_staged
\*   crates/db/src/definition/patch/store.rs  DB::stage_cross_collection_one_to_one_indexes
\*                                             (the index, on the patch's txn)
\*   crates/db/src/collection/ops/version.rs  DB::collection_has_data (a
\*                                             validated scan in the txn)
EXTENDS Naturals

CONSTANTS
  ReadThroughTxn,  \* the patch reads through the client's transaction
  WriteThroughTxn, \* the patch writes into the client's transaction
  FinishOnCommit,  \* post-commit work waits for the commit to be durable
  RetryHides,      \* a conflict is retried silently instead of reported
  Savepoint,       \* the patch runs behind a statement savepoint
  RefreshCatalog,  \* a staged patch drops what it changed from the txn's cache
  NacInTxn,        \* the NAC check reads the permission in the client's txn
  IndexInTxn       \* the owed one-to-one unique index is staged in the patch's txn

\* Each switch is on or off.
ASSUME WriteThroughTxn \in BOOLEAN
ASSUME ReadThroughTxn \in BOOLEAN /\ FinishOnCommit \in BOOLEAN /\ RetryHides \in BOOLEAN
ASSUME Savepoint \in BOOLEAN /\ RefreshCatalog \in BOOLEAN /\ NacInTxn \in BOOLEAN
ASSUME IndexInTxn \in BOOLEAN

VARIABLES
  earlier,    \* the client wrote something earlier in its transaction
  sawEarlier, \* the patch's decision saw that earlier write
  staged,     \* the patch is staged in the client's transaction
  rival,      \* another client committed a change to what the patch read
  outcome,    \* "open", "committed", "conflict" or "discarded"
  visible,    \* other clients can see the patch
  finished,   \* the post-commit work has run
  cached,     \* the transaction cached the patched collection's old definition
  stale,      \* that cached old definition is still what the txn resolves
  failed,     \* a patch attempt failed part way
  partial,    \* the transaction still holds writes of the failed attempt
  revoked,    \* an administrator revoked the patch permission and committed
  indexed     \* the one-to-one unique index the patch owes is durable

\* Every variable, so a step can say which ones it leaves alone.
vars == <<earlier, sawEarlier, staged, rival, outcome, visible, finished,
          cached, stale, failed, partial, revoked, indexed>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* the flags are true or false
  /\ earlier \in BOOLEAN /\ sawEarlier \in BOOLEAN /\ staged \in BOOLEAN
  /\ rival \in BOOLEAN /\ visible \in BOOLEAN /\ finished \in BOOLEAN
  /\ cached \in BOOLEAN /\ stale \in BOOLEAN
  /\ failed \in BOOLEAN /\ partial \in BOOLEAN /\ revoked \in BOOLEAN
  /\ indexed \in BOOLEAN
  \* the transaction ends one of three ways, or is still open
  /\ outcome \in {"open", "committed", "conflict", "discarded"}

\* A fresh transaction: nothing written, cached or staged, nobody else moved.
Init ==
  /\ earlier = FALSE /\ sawEarlier = FALSE /\ staged = FALSE
  /\ rival = FALSE /\ outcome = "open" /\ visible = FALSE /\ finished = FALSE
  /\ cached = FALSE /\ stale = FALSE /\ failed = FALSE /\ partial = FALSE
  /\ revoked = FALSE /\ indexed = FALSE

\* The client writes something in its transaction before the patch.
WriteEarlier ==
  \* only while the transaction is open and before the patch
  /\ outcome = "open" /\ ~staged /\ ~earlier
  /\ earlier' = TRUE
  /\ UNCHANGED <<sawEarlier, staged, rival, outcome, visible, finished,
                 cached, stale, failed, partial, revoked, indexed>>

\* The client reads the collection before the patch, so the transaction
\* caches its definition as it is now.
ReadEarlier ==
  \* only while the transaction is open and before the patch
  /\ outcome = "open" /\ ~staged /\ ~cached
  /\ cached' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, outcome, visible,
                 finished, stale, failed, partial, revoked, indexed>>

\* A patch attempt fails part way, after writing some of its collections.
\* Behind a savepoint those writes are undone; without one (the RED) they
\* stay in the transaction.
StageFails ==
  \* once, while the transaction is open and before any patch is staged
  /\ outcome = "open" /\ ~staged /\ ~failed
  /\ failed' = TRUE
  \* the bug a RED plants: the earlier collections' writes are kept
  /\ partial' = (~Savepoint)
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, outcome, visible,
                 finished, cached, stale, revoked, indexed>>

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
  \* the bug a RED plants: the old cached definition is left in place
  /\ stale' = (cached /\ ~RefreshCatalog)
  /\ UNCHANGED <<earlier, rival, outcome, cached, failed, partial, revoked,
                 indexed>>

\* Another client commits a change to the definitions the patch read.
RivalCommits ==
  \* only after the patch read, while the client's transaction is still open
  /\ outcome = "open" /\ staged /\ ~rival
  /\ rival' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, outcome, visible, finished,
                 cached, stale, failed, partial, revoked, indexed>>

\* An administrator revokes the client's patch permission and commits, after
\* the patch's NAC check passed but before the client commits.
RevokeCommits ==
  \* only after the patch was checked, while the client's transaction is open
  /\ outcome = "open" /\ staged /\ ~revoked
  /\ revoked' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, outcome, visible,
                 finished, cached, stale, failed, partial, indexed>>

\* A commit that the client's transaction read is overtaken by: a rival
\* change to the definitions, or a revocation when NAC was read in the
\* transaction. A check read outside it (the RED) never notices a revocation.
Overtaken == rival \/ (revoked /\ NacInTxn)

\* The client commits. A commit that overtook what the patch read fails it
\* with a conflict, unless a silent retry (the RED) lands it anyway.
Commit ==
  \* only an open transaction holding the patch
  /\ outcome = "open" /\ staged
  /\ IF Overtaken /\ ~RetryHides
       \* the conflict goes back to the client, and nothing lands
       THEN /\ outcome' = "conflict" /\ UNCHANGED <<visible, finished, indexed>>
       \* the patch lands, everyone can see it, and the follow-up work runs
       ELSE /\ outcome' = "committed" /\ visible' = TRUE /\ finished' = TRUE
            \* the owed index lands with the patch only if it was staged with
            \* it; the bug a RED plants leaves it to a second commit
            /\ indexed' = IndexInTxn
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, cached, stale, failed,
                 partial, revoked>>

\* The bug a RED plants: the owed index is built by a second transaction
\* after the patch committed. Until it commits (and forever, if it fails),
\* everyone sees the patch without the index.
IndexLater ==
  \* only after the patch landed without its index
  /\ outcome = "committed" /\ ~indexed /\ ~IndexInTxn
  /\ indexed' = TRUE
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, outcome, visible,
                 finished, cached, stale, failed, partial, revoked>>

\* The client discards its transaction: nothing lands.
Discard ==
  \* only an open transaction
  /\ outcome = "open"
  /\ outcome' = "discarded"
  /\ UNCHANGED <<earlier, sawEarlier, staged, rival, visible, finished,
                 cached, stale, failed, partial, revoked, indexed>>

\* Any of the steps can happen next.
Next == WriteEarlier \/ ReadEarlier \/ StageFails \/ Stage \/ RivalCommits
        \/ RevokeCommits \/ Commit \/ Discard \/ IndexLater

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

\* A patch that failed leaves none of its writes in the transaction.
INV_FailedPatchLeavesNothing == ~partial

\* Once the patch is staged, the transaction resolves the patched collection
\* to its new definition, not the one it cached before.
INV_SeesOwnPatch == staged => ~stale

\* A patch never lands once the permission that allowed it was revoked by a
\* commit before the client's.
INV_RevocationDeniesPatch == (revoked /\ staged) => outcome # "committed"

\* The one-to-one unique index a patch owes is durable exactly when the patch
\* is: no one ever sees the patch without it, nor the index without the patch.
INV_IndexLandsWithPatch == indexed <=> outcome = "committed"
====
