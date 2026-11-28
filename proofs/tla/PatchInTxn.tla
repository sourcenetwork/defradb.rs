---- MODULE PatchInTxn ----
\* The story: a client opens an explicit transaction and patches a
\* collection in it (adds a field, say). It may read the collection's
\* definition in that same transaction, and then it commits or discards.
\* Meanwhile another writer may change any collection definition on its own
\* and commit first.
\*
\* The patch writes the new definition into the client's transaction, and
\* the transaction forgets the definitions it had cached, so the client's
\* later requests in the transaction see the patch. The shared collection
\* cache every other request reads is only told about the patch by the
\* commit's callbacks, and the follow-up work (registering a committed lens,
\* forbidding a collection left with no version, rebuilding owed indexes)
\* runs only once the commit is durable, awaited before the commit returns. A
\* failed follow-up is reported and remains owed, so the call returning means
\* the work ran, not that it succeeded.
\* A patch that fails part way runs as one statement opened with a
\* savepoint, so the failure rolls the transaction back to it. The patch
\* reads every collection definition, so any definition change that commits
\* first makes the client's commit fail as a conflict, which the client gets
\* back.
\*
\* What must never happen: anyone else seeing the patch before it commits;
\* any trace of the patch (stored, cached or followed up) after a discard or
\* a conflict; the client's own read in the transaction missing its patch; a
\* commit carrying the half-written writes or lens registration of a patch that
\* failed; the commit returning before its follow-up work has run; a commit
\* that silently overwrites a definition another writer committed first.
\*
\* Code it models:
\*   crates/database/db/src/txn/registry/lifecycle.rs    patch_collection_as (routes a txn)
\*   crates/database/db/src/txn/registry/patches.rs      patch_collection_in_txn_as
\*   crates/database/db/src/txn/registry/mod.rs          finish_statement
\*   crates/database/db/src/txn/registry/owned.rs        begin_operation_statement,
\*                                                 end_operation_statement
\*   crates/database/db/src/txn/lenses.rs                TxnLensStore::snapshot, restore
\*   crates/database/db/src/definition/patch/mod.rs      DB::patch_collection,
\*                                                 DB::patch_collection_with_txn
\*   crates/database/db/src/definition/patch/outcome.rs  DB::after_patch,
\*                                                 forget_cached_collections
\*   crates/database/db/src/definition/patch/store.rs    DB::store_new_version (cache on commit)
\*   crates/database/db/src/definition/patch/in_place.rs DB::update_version_in_place (cache on commit)
\*   crates/database/db/src/definition/patch/added.rs    DB::store_added_version (cache on commit)
\*   crates/database/db/src/txn/registry/lifecycle.rs    commit_error (a conflict to the caller)
EXTENDS Naturals

CONSTANTS
  WriteThroughTxn,     \* the patch is written into the client's transaction
  PublishOnCommit,     \* the shared collection cache takes the patch only at commit
  FollowUpOnCommit,    \* the follow-up work waits for the commit to be durable
  RefreshCatalog,      \* the transaction forgets its cached definitions after the patch
  RollBackFailed,      \* a patch that fails part way is rolled back to its savepoint
  RestoreLensStore,    \* the rollback also restores the statement's local lens registrations
  AwaitFollowUp,       \* the commit returns only once the follow-up work is done
  ValidateDefinitions  \* the definitions the patch read are checked at commit

\* Each switch is on or off.
ASSUME WriteThroughTxn \in BOOLEAN /\ PublishOnCommit \in BOOLEAN
\* (the remaining switches are on or off too)
ASSUME FollowUpOnCommit \in BOOLEAN /\ RefreshCatalog \in BOOLEAN
\* (the remaining switches)
ASSUME RollBackFailed \in BOOLEAN /\ RestoreLensStore \in BOOLEAN
\* (and the last two)
ASSUME AwaitFollowUp \in BOOLEAN /\ ValidateDefinitions \in BOOLEAN

VARIABLES
  staged,    \* the patch is written into the client's transaction
  listed,    \* the client read the definition in its transaction
  listedNew, \* that read showed the patch
  outcome,   \* "open", "committed", "discarded" or "conflicted"
  visible,   \* the patched definition is in the store other requests read
  published, \* the shared collection cache holds the patched definition
  followUp,  \* the follow-up work for the patch has run
  returned,  \* the client's commit or discard call has returned
  leftover,  \* writes or a local lens registration left by a failed patch
  other,     \* another writer committed a definition change
  lost       \* the client's commit overwrote that other writer's definition

\* Every variable, for stuttering steps.
vars == <<staged, listed, listedNew, outcome, visible, published, followUp, returned,
          leftover, other, lost>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* the flags are true or false
  /\ staged \in BOOLEAN /\ listed \in BOOLEAN /\ listedNew \in BOOLEAN
  \* (more flags)
  /\ visible \in BOOLEAN /\ published \in BOOLEAN /\ followUp \in BOOLEAN
  \* (the last flags)
  /\ returned \in BOOLEAN /\ other \in BOOLEAN /\ lost \in BOOLEAN
  \* a failed patch can leave stored writes, a local lens, both or neither
  /\ leftover \in SUBSET {"writes", "lens"}
  \* the transaction is open or ended one of three ways
  /\ outcome \in {"open", "committed", "discarded", "conflicted"}

\* A fresh transaction: nothing patched, read, seen, cached, run or left over.
Init ==
  \* nothing in the transaction yet
  /\ staged = FALSE /\ listed = FALSE /\ listedNew = FALSE /\ outcome = "open"
  \* nothing outside it either
  /\ visible = FALSE /\ published = FALSE /\ followUp = FALSE /\ returned = FALSE
  \* no failed patch, no other writer, nothing lost
  /\ leftover = {} /\ other = FALSE /\ lost = FALSE

\* The client patches the collection in its transaction. Each RED switch
\* plants a bug that lets part of the patch escape the transaction here.
Patch ==
  \* only once, while the transaction is open
  /\ outcome = "open" /\ ~staged
  \* the patch is now in the client's transaction
  /\ staged' = TRUE
  \* the bug a RED plants: the patch commits in a transaction of its own
  /\ visible' = (~WriteThroughTxn)
  \* the bug a RED plants: the shared cache is updated at patch time
  /\ published' = (~PublishOnCommit)
  \* the bug a RED plants: the lens is registered (or the collection
  \* forbidden) before any commit
  /\ followUp' = (~FollowUpOnCommit)
  \* the client's read state, the outcome and the other writer stay
  /\ UNCHANGED <<listed, listedNew, outcome, returned, leftover, other, lost>>

\* The client's patch fails after writing part of it (its second operation
\* is refused, say), and the client gets the error. Rolled back to its
\* savepoint, nothing of it stays; without either rollback (the REDs) its first
\* writes or its transaction-local lens can stay in the transaction.
FailPatch ==
  \* while the transaction is open, before any patch succeeded
  /\ outcome = "open" /\ ~staged
  \* the bugs RED configurations plant: the storage savepoint or the local
  \* lens snapshot is not restored, so that part of the failed patch remains
  /\ leftover' = leftover
                 \cup (IF RollBackFailed THEN {} ELSE {"writes"})
                 \cup (IF RestoreLensStore THEN {} ELSE {"lens"})
  \* nothing else moves
  /\ UNCHANGED <<staged, listed, listedNew, outcome, visible, published, followUp, returned,
                 other, lost>>

\* The client reads the definition in its transaction (a query, or a list of
\* its collections). It sees the patch only if the transaction forgot the
\* definitions it had cached before the patch.
Read ==
  \* after the patch, once, while the transaction is open
  /\ outcome = "open" /\ staged /\ ~listed
  \* the read happened
  /\ listed' = TRUE
  \* the bug a RED plants: a stale cached definition answers the read
  /\ listedNew' = RefreshCatalog
  \* nothing else moves
  /\ UNCHANGED <<staged, outcome, visible, published, followUp, returned, leftover, other,
                 lost>>

\* Another writer changes a collection definition on its own and commits,
\* after the client's patch read every definition and before the client
\* commits.
OtherDefinitionChange ==
  \* once, while the client's patch is staged and its transaction open
  /\ outcome = "open" /\ staged /\ ~other
  \* the other writer's patch is committed
  /\ other' = TRUE
  \* nothing of the client's moves
  /\ UNCHANGED <<staged, listed, listedNew, outcome, visible, published, followUp, returned,
                 leftover, lost>>

\* A definition the client's patch read changed before its commit, and
\* the commit checks them: the commit fails as a conflict.
Conflicts == staged /\ other /\ ValidateDefinitions

\* The client commits. A conflict ends the transaction with nothing landed
\* and the conflict returned to the client. Otherwise the patch becomes
\* visible, the commit's callbacks put it in the shared cache, and, awaited,
\* the follow-up work runs before the call returns.
Commit ==
  \* only an open transaction
  /\ outcome = "open"
  \* the call returns either way, with the conflict or the success
  /\ returned' = TRUE
  \* the transaction ends conflicted, or committed
  /\ outcome' = IF Conflicts THEN "conflicted" ELSE "committed"
  \* only a landed commit makes the patch visible
  /\ visible' = (visible \/ (staged /\ ~Conflicts))
  \* only a landed commit's callbacks put it in the shared cache
  /\ published' = (published \/ (staged /\ ~Conflicts))
  \* awaited: the follow-up for a landed patch is done when the call returns
  /\ followUp' = (followUp \/ (staged /\ ~Conflicts /\ AwaitFollowUp))
  \* the bug a RED plants: unchecked, the commit lands over the other patch
  /\ lost' = (staged /\ other /\ ~Conflicts)
  \* the transaction's own state stays as it was
  /\ UNCHANGED <<staged, listed, listedNew, leftover, other>>

\* Follow-up work the commit did not wait for runs after the call returned.
Finish ==
  \* only for a committed patch whose follow-up has not run yet
  /\ outcome = "committed" /\ staged /\ ~followUp
  \* it runs now
  /\ followUp' = TRUE
  \* nothing else moves
  /\ UNCHANGED <<staged, listed, listedNew, outcome, visible, published, returned, leftover,
                 other, lost>>

\* The client discards its transaction: nothing lands, nothing follows.
Discard ==
  \* only an open transaction
  /\ outcome = "open"
  \* it ends discarded, and the call returns
  /\ outcome' = "discarded" /\ returned' = TRUE
  \* nothing else moves
  /\ UNCHANGED <<staged, listed, listedNew, visible, published, followUp, leftover, other,
                 lost>>

\* Any step can happen next.
Next == Patch \/ FailPatch \/ Read \/ OtherDefinitionChange \/ Commit \/ Finish \/ Discard

\* A finished transaction stays finished.
Done == outcome # "open" /\ UNCHANGED vars

\* The client's transaction runs its steps, then rests.
Spec == Init /\ [][Next \/ Done]_vars

\* No one else sees the patch, in the store or in the shared cache, unless
\* the commit landed.
INV_IsolatedUntilCommit == (visible \/ published) => outcome = "committed"

\* A discarded or conflicted transaction leaves no trace of its patch: not
\* stored, not cached, and no follow-up work run for it.
INV_NoTraceAfterRollback ==
  \* every way the transaction can end without landing
  outcome \in {"discarded", "conflicted"} => (~visible /\ ~published /\ ~followUp)

\* The client's own read in the transaction shows the patch it made.
INV_SeesOwnPatch == listed => listedNew

\* A transaction that commits after a patch failed carries none of that
\* failed patch's writes or local lens registration.
INV_FailedPatchLeavesNothing == outcome = "committed" => leftover = {}

\* When the commit call returns for a patch, its follow-up work has run. The
\* work may have failed and been reported; it then remains owed for a retry.
INV_FollowUpRanWhenCommitReturns ==
  \* a returned, landed patch has run its follow-up
  (returned /\ outcome = "committed" /\ staged) => followUp

\* A definition another writer committed first is never silently overwritten:
\* the client's commit fails as a conflict instead.
INV_NoLostPatch == ~lost
====
