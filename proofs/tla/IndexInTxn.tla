---- MODULE IndexInTxn ----
\* The story: a client opens an explicit transaction and creates (or drops) a
\* secondary index in it, may list the collection's indexes in that same
\* transaction, and then commits or discards.
\*
\* The definition change and its action record are written into the
\* client's transaction, so the client's later reads in the transaction see
\* the new definition and nobody else sees it until the commit. The work that
\* follows a definition change (planning and running the backfill, or
\* removing a dropped index's entries) starts only once the commit is
\* durable, never for a discarded transaction, and the commit returns once
\* that work is done, as an autocommit create or drop does.
\*
\* A change can also fail part way: the definition is written but its name
\* key or action record is not. The change runs as one statement, opened
\* with a savepoint, so a failure rolls the transaction back to it.
\*
\* What must never happen: the backfill running for a definition that never
\* committed; another client seeing the definition before the commit; the
\* client's own listing in the transaction missing the change it just made;
\* the commit returning while the index it created is still being built; a
\* commit carrying the half-written definition of a change that failed.
\*
\* Who may make the change is NacOrder's model: the registry checks
\* get-collection, then the operation's own permission, in the transaction.
\*
\* Code it models:
\*   crates/defra-node/src/ops/index.rs           Node::create_index_as,
\*                                                 Node::delete_index_as,
\*                                                 Node::list_indexes_as (route a txn)
\*   crates/db/src/txn/registry/indexes.rs        create_index_in_txn,
\*                                                 drop_index_in_txn, indexes_in_txn
\*   crates/db/src/txn/registry/mod.rs            finish_statement
\*   crates/db/src/index/in_txn.rs                DB::create_index_in,
\*                                                 DB::drop_index_in
\*   crates/http/src/handlers/index/go_api.rs     the x-defradb-tx branches
\* Encrypted indexes follow the same steps with no backfill: the change is
\* staged in the caller's transaction and the shared cache takes it once the
\* commit is durable, before the commit call returns (the "work" here). A
\* failed add or delete is rolled back to its savepoint the same way. Listing
\* one collection's encrypted indexes, or every collection's, in the
\* transaction is List: both read the definitions from the transaction's
\* store, never a cached copy, so they see the change as RefreshCatalog does:
\*   crates/db/src/collection/encrypted_index.rs  DB::add_encrypted_index_in,
\*                                                 DB::delete_encrypted_index_in,
\*                                                 DB::encrypted_indexes_in,
\*                                                 DB::all_encrypted_indexes_in
\*   crates/db/src/txn/registry/indexes.rs        add_encrypted_index_in_txn,
\*                                                 delete_encrypted_index_in_txn,
\*                                                 encrypted_indexes_in_txn,
\*                                                 all_encrypted_indexes_in_txn
\*   crates/defra-node/src/ops/encrypted_index.rs NodeOps (routes a txn)
\*   crates/cli/src/encrypted_index_adapter.rs    EncryptedIndexAdapter (routes a txn)
\*   crates/http/src/handlers/encrypted_index.rs  the x-defradb-tx context
\* Views follow them too. AddView stages its view collections in the
\* caller's transaction, and building its materialized views (and
\* bootstrapping its downsamples) is the follow-up work, run at commit and
\* awaited. RefreshViews stages nothing; its rebuild is the follow-up work,
\* run at commit from what the transaction committed. An AddView whose
\* multi-type SDL fails part way is rolled back to its savepoint:
\*   crates/db/src/collection/ops/create.rs        DB::stage_new_collections
\*   crates/db/src/txn/registry/views.rs           add_view_in_txn,
\*                                                 refresh_views_in_txn
\*   crates/defra-node/src/db_impls.rs             DbSchemaOps (routes a txn)
\*   crates/http/src/handlers/views.rs             the x-defradb-tx context
EXTENDS Naturals

CONSTANTS
  WriteThroughTxn, \* the definition change is written into the client's transaction
  WorkOnCommit,   \* the backfill waits for the commit to be durable
  RefreshCatalog, \* the transaction drops its cached definition after the change
  AwaitWork,      \* the commit returns only once the backfill is done
  ReadCommitted,  \* the follow-up work reads what the transaction committed
  RollBackFailed  \* a change that fails part way is rolled back to its savepoint

\* Each switch is on or off.
ASSUME WriteThroughTxn \in BOOLEAN
ASSUME WorkOnCommit \in BOOLEAN /\ RefreshCatalog \in BOOLEAN /\ AwaitWork \in BOOLEAN
ASSUME ReadCommitted \in BOOLEAN /\ RollBackFailed \in BOOLEAN

VARIABLES
  staged,     \* the definition change is written into the client's transaction
  listedNew,  \* the client's listing in the transaction showed the change
  listed,     \* the client listed the indexes in the transaction
  outcome,    \* "open", "committed" or "discarded"
  visible,    \* other clients can see the change
  workRan,    \* the backfill (or entry removal, view build, view rebuild) has run
  returned,   \* the client's commit call has returned
  wrote,      \* the client wrote a document in its transaction before the change
  sawWrite,   \* the follow-up work saw that document
  leftover    \* writes of a failed change are still in the transaction

vars == <<staged, listedNew, listed, outcome, visible, workRan, returned, wrote, sawWrite,
          leftover>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  \* the flags are true or false
  /\ staged \in BOOLEAN /\ listedNew \in BOOLEAN /\ listed \in BOOLEAN
  /\ visible \in BOOLEAN /\ workRan \in BOOLEAN /\ returned \in BOOLEAN
  /\ wrote \in BOOLEAN /\ sawWrite \in BOOLEAN /\ leftover \in BOOLEAN
  \* the transaction is open or ended one of two ways
  /\ outcome \in {"open", "committed", "discarded"}

\* A fresh transaction: nothing staged, listed, seen, run, written or left over.
Init ==
  /\ staged = FALSE /\ listedNew = FALSE /\ listed = FALSE
  /\ outcome = "open" /\ visible = FALSE /\ workRan = FALSE /\ returned = FALSE
  /\ wrote = FALSE /\ sawWrite = FALSE /\ leftover = FALSE

\* The client writes a document in its transaction before the change, as a
\* RefreshViews scenario adds a document and then asks for the rebuild.
WriteDoc ==
  \* only before the change, once, while the transaction is open
  /\ outcome = "open" /\ ~staged /\ ~wrote
  /\ wrote' = TRUE
  /\ UNCHANGED <<staged, listedNew, listed, outcome, visible, workRan, returned, sawWrite,
                 leftover>>

\* The client creates or drops the index in its transaction. If the follow-up
\* work does not wait for the commit, it runs right here.
Change ==
  \* only once, while the transaction is open
  /\ outcome = "open" /\ ~staged
  /\ staged' = TRUE
  \* the bug a RED plants: the backfill starts before any commit
  /\ workRan' = (~WorkOnCommit)
  \* work run now runs outside the transaction, so it cannot see its write
  /\ sawWrite' = FALSE
  \* the bug a RED plants: the change commits in a transaction of its own
  /\ visible' = (~WriteThroughTxn)
  /\ UNCHANGED <<listedNew, listed, outcome, returned, wrote, leftover>>

\* The client's change fails after writing part of the definition, and the
\* client gets the error. Rolled back to its savepoint, nothing of it stays;
\* without the rollback (the RED) its first writes stay in the transaction.
FailChange ==
  \* while the transaction is open, before any change succeeded
  /\ outcome = "open" /\ ~staged
  \* the bug a RED plants: no savepoint, so the partial writes remain
  /\ leftover' = (leftover \/ ~RollBackFailed)
  /\ UNCHANGED <<staged, listedNew, listed, outcome, visible, workRan, returned, wrote,
                 sawWrite>>

\* The client lists the indexes in its transaction. It sees the change only
\* if the transaction's cached definition was dropped after the change.
List ==
  \* after the change, once, while the transaction is open
  /\ outcome = "open" /\ staged /\ ~listed
  /\ listed' = TRUE
  /\ listedNew' = RefreshCatalog
  /\ UNCHANGED <<staged, outcome, visible, workRan, returned, wrote, sawWrite, leftover>>

\* The client commits and the change becomes visible. Awaited, the commit
\* call runs the follow-up work before it returns; otherwise (the RED) it
\* returns first and the work runs later, in Finish.
Commit ==
  \* only an open transaction
  /\ outcome = "open"
  /\ outcome' = "committed" /\ visible' = (visible \/ staged) /\ returned' = TRUE
  \* awaited: the work for a staged change is done when the call returns
  /\ workRan' = (workRan \/ (staged /\ AwaitWork))
  \* work run at commit reads the committed state, write included, unless
  \* (the RED) it reads a snapshot taken before the transaction committed
  /\ sawWrite' = IF ~workRan /\ staged /\ AwaitWork THEN (wrote /\ ReadCommitted) ELSE sawWrite
  /\ UNCHANGED <<staged, listedNew, listed, wrote, leftover>>

\* Work the commit did not wait for runs after the call has returned.
Finish ==
  \* only for a committed change whose work has not run yet
  /\ outcome = "committed" /\ staged /\ ~workRan
  /\ workRan' = TRUE
  \* it reads the committed state, unless (the RED) a stale snapshot
  /\ sawWrite' = (wrote /\ ReadCommitted)
  /\ UNCHANGED <<staged, listedNew, listed, outcome, visible, returned, wrote, leftover>>

\* The client discards its transaction: nothing lands, nothing follows.
Discard ==
  \* only an open transaction
  /\ outcome = "open"
  /\ outcome' = "discarded" /\ returned' = TRUE
  /\ UNCHANGED <<staged, listedNew, listed, visible, workRan, wrote, sawWrite, leftover>>

\* Any step can happen next.
Next == WriteDoc \/ Change \/ FailChange \/ List \/ Commit \/ Finish \/ Discard

\* A finished transaction stays finished.
Done == outcome # "open" /\ UNCHANGED vars

\* The client's transaction runs its steps, then rests.
Spec == Init /\ [][Next \/ Done]_vars

\* The follow-up work runs only for a change whose commit landed.
INV_WorkOnlyAfterCommit == workRan => outcome = "committed"

\* No one else sees the change unless the commit landed.
INV_IsolatedUntilCommit == visible => outcome = "committed"

\* The client's own listing in the transaction shows the change it made.
INV_SeesOwnChange == listed => listedNew

\* When the commit call returns for a change, its follow-up work is done:
\* the commit is awaited, so the index is built before the call returns.
INV_BuiltWhenCommitReturns ==
  (returned /\ outcome = "committed" /\ staged) => workRan

\* Follow-up work that ran saw every document the transaction wrote before
\* the change: a RefreshViews in a transaction rebuilds from what it
\* committed, the documents it added included.
INV_WorkSeesTxnWrites == (workRan /\ wrote) => sawWrite

\* A commit never carries the half-written definition of a change that
\* failed: the client was told it failed, so none of it may land.
INV_FailedChangeLeavesNothing == outcome = "committed" => ~leftover
====
