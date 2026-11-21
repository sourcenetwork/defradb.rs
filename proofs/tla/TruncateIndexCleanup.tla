---- MODULE TruncateIndexCleanup ----
\* The story: a client truncates a collection that has secondary indexes. The
\* truncate deletes the documents in chunks, then, in one last transaction,
\* deletes the collection's metadata, which includes every index entry. While
\* it runs, someone else can create a new index on the same collection: the
\* create commits the index into the collection's definition, then a backfill
\* writes an entry for each live document, a page at a time, and can fail
\* after some pages have committed, which leaves the index's build marked
\* failed with its entries still on disk.
\*
\* Who acts: the truncate (deleting the documents, then reading which indexes
\* to clean and committing the cleanup), the index creator, and the backfill of
\* each new index (indexing a document, or failing).
\*
\* What must never happen:
\*   - an index entry outlives the truncate for a document the truncate
\*     deleted, whatever index it belongs to: one created during the truncate,
\*     or one whose build failed;
\*   - the cleanup deletes another collection's documents or index entries.
\*
\* How the code keeps it: the cleanup transaction reads the collection's
\* definition inside itself, after every chunk has committed, and cleans every
\* index it declares, a failed build's too. An index it does not see was
\* defined after that read, so after the chunks: its backfill can only find
\* documents that are still alive, because every backfill page reads each
\* document it indexes as a validated read, and a page that read a document a
\* chunk deleted before the page committed runs again without it. So no lock
\* between index creation and truncate is needed, and none is taken. The
\* cleanup deletes each declared index's own key prefix, never one prefix for
\* the whole collection, which for some short IDs is also the start of another
\* family's keys (proofs/lean/IndexPrefix.lean proves which prefixes overlap).
\*
\* Code it models:
\*   crates/database/db/src/collection/ops/truncate/all.rs
\*     DB::truncate_all              the chunks, then the metadata cleanup
\*     DB::truncate_metadata_in      the cleanup: the definition read through
\*                                   DbTxn::load_collection_for_write_attempt,
\*                                   then IndexDataStoreKey::index_prefix of
\*                                   every declared ordered index
\*   crates/database/db/src/index/create.rs
\*     DB::stage_index_definition    the create's definition commit
\*   crates/database/db/src/index/backfill.rs
\*     BackfillPage::index           a page: validated reads of the definition
\*                                   and of each document it indexes
\*     DB::record_backfill_failure   a failed build, entries kept
EXTENDS FiniteSets   \* sets of documents and indexes

CONSTANTS
  Docs,          \* the documents the collection holds when the truncate starts
  OldIndexes,    \* the indexes it already has, built, when the truncate starts
  NewIndexes,    \* the indexes someone can create while the truncate runs
  Source,        \* where the cleanup learns the indexes (see ASSUME below)
  Which,         \* which of those indexes it cleans (see ASSUME below)
  BackfillRead,  \* how a backfill page reads a document (see ASSUME below)
  Boundary       \* what key prefix the cleanup deletes (see ASSUME below)

\* There are documents to truncate, an index to clean, and one to create.
ASSUME Docs # {} /\ OldIndexes # {} /\ NewIndexes # {}
\* An index is either old or new, never both.
ASSUME OldIndexes \cap NewIndexes = {}
\* "InTxn": the cleanup reads the definition in its own transaction, after
\*   the chunks (the fix);
\* "Captured": it uses the collection it was handed when the truncate began.
ASSUME Source \in {"InTxn", "Captured"}
\* "Declared": every index the definition names (the fix);
\* "Writable": only indexes whose build has not failed.
ASSUME Which \in {"Declared", "Writable"}
\* "Validated": a page reads each document it indexes, and a document deleted
\*   before the page commits makes the page run again;
\* "Snapshot": a page reads its documents and commits their entries even if
\*   they were deleted in between.
ASSUME BackfillRead \in {"Validated", "Snapshot"}
\* "Index": the cleanup deletes one prefix per index (the fix);
\* "Collection": it deletes the collection's whole prefix, which, for a short
\*   ID such as 100 (the byte `d`), is also the prefix of every collection's
\*   documents.
ASSUME Boundary \in {"Index", "Collection"}

\* Every index the model knows.
Indexes == OldIndexes \cup NewIndexes

\* Another collection's document, the keys the cleanup must never touch.
Other == "other"

VARIABLES
  live,         \* the truncated collection's documents still on disk
  deleted,      \* the documents the truncate deleted
  defined,      \* the indexes in the collection's definition
  failed,       \* the defined indexes whose build failed
  entries,      \* index entries on disk: <<index, document>> pairs
  pending,      \* backfill pages read but not committed ("Snapshot" only)
  otherLive,    \* whether the other collection's document is still on disk
  tPc,          \* where the truncate is: "chunks", "read", "commit", "done"
  tSeen         \* the indexes the cleanup's open transaction will clean

vars == <<live, deleted, defined, failed, entries, pending, otherLive, tPc, tSeen>>

\* Every variable holds a value of the right shape.
TypeOK ==
  /\ live \subseteq Docs                                  \* live documents are the collection's
  /\ deleted \subseteq Docs                               \* so are the deleted ones
  /\ defined \subseteq Indexes                            \* only known indexes are defined
  /\ failed \subseteq defined                             \* only a defined index can fail
  /\ entries \subseteq Indexes \X Docs                    \* entries pair an index with a document
  /\ pending \subseteq Indexes \X Docs                    \* pending pages are pairs too
  /\ otherLive \in BOOLEAN                                \* the other document is there or not
  /\ tPc \in {"chunks", "read", "commit", "done"}         \* a known truncate step
  /\ tSeen \subseteq Indexes                              \* the cleanup cleans known indexes

\* The truncate is about to start: every document is live and has an entry in
\* every old index; no new index exists yet.
Init ==
  /\ live = Docs                         \* nothing deleted yet
  /\ deleted = {}                        \* so the truncate deleted nothing
  /\ defined = OldIndexes                \* only the old indexes
  /\ failed = {}                         \* their builds finished
  /\ entries = OldIndexes \X Docs        \* each old index holds every document
  /\ pending = {}                        \* no backfill page is open
  /\ otherLive = TRUE                    \* the other collection's document is there
  /\ tPc = "chunks"                      \* the truncate starts with the documents
  /\ tSeen = {}                          \* no cleanup transaction yet

\* ---------------------------------------------------------------- truncate

\* The chunks delete every document that existed when the truncate began.
\* Each chunk reads the documents it deletes, so a page indexing one of them
\* cannot commit across it: the chunks act as one step here.
Chunks ==
  /\ tPc = "chunks"                      \* the documents go first
  /\ deleted' = live                     \* the documents it found
  /\ live' = {}                          \* are gone
  /\ tPc' = "read"                       \* next the cleanup
  /\ UNCHANGED <<defined, failed, entries, pending, otherLive, tSeen>>

\* The indexes the cleanup would clean, out of a set of defined indexes and the
\* failed ones among them.
Cleaned(def, fail) == IF Which = "Declared" THEN def ELSE def \ fail

\* The cleanup opens its transaction and learns the indexes: from a read of the
\* definition, or from the collection the truncate was handed, which has the
\* old indexes, all built.
Read ==
  /\ tPc = "read"                                              \* the chunks are done
  /\ tSeen' = IF Source = "Captured"
                THEN Cleaned(OldIndexes, {})                   \* the handed collection
                ELSE Cleaned(defined, failed)                  \* the definition as it is now
  /\ tPc' = "commit"                                           \* next the commit
  /\ UNCHANGED <<live, deleted, defined, failed, entries, pending, otherLive>>

\* The keys of the indexes in `seen`, which an index-at-a-time cleanup deletes.
IndexKeys(seen) == {e \in entries : e[1] \in seen}

\* The cleanup commits and deletes its keys: each index it saw, or, with the
\* collection-wide prefix, every entry of the collection and, for a short ID
\* that spells `d`, the other collection's document too. (Its reads are
\* validated, so a definition change between read and commit would run it
\* again; the invariants hold without that, so the model leaves it out.)
Commit ==
  /\ tPc = "commit"                                            \* the read is done
  /\ entries' = IF Boundary = "Index"
                   THEN entries \ IndexKeys(tSeen)             \* one prefix per index
                   ELSE {}                                     \* the whole collection prefix
  /\ otherLive' = (otherLive /\ Boundary = "Index")            \* `/d/` swept with it
  /\ tPc' = "done"                                             \* the truncate is finished
  /\ UNCHANGED <<live, deleted, defined, failed, pending, tSeen>>

\* ---------------------------------------------------------------- index creator

\* Someone creates a new index: its definition commits, at any point of the
\* truncate, nothing holding it back.
Create(i) ==
  /\ i \in NewIndexes \ defined          \* not created yet
  /\ defined' = defined \cup {i}         \* now in the definition
  /\ UNCHANGED <<live, deleted, failed, entries, pending, otherLive, tPc, tSeen>>

\* ---------------------------------------------------------------- backfill

\* A new index whose build is still running.
Building(i) == i \in (defined \cap NewIndexes) \ failed

\* A validated page indexes a live document in one commit: a document deleted
\* between its read and its commit makes it run again and not find it.
PageValidated(i, d) ==
  /\ BackfillRead = "Validated"          \* the page reads are validated
  /\ Building(i)                         \* the build runs
  /\ d \in live                          \* the document is still there
  /\ <<i, d>> \notin entries             \* and not indexed yet
  /\ entries' = entries \cup {<<i, d>>}  \* its entry commits
  /\ UNCHANGED <<live, deleted, defined, failed, pending, otherLive, tPc, tSeen>>

\* An unvalidated page reads a live document ...
PageRead(i, d) ==
  /\ BackfillRead = "Snapshot"           \* the page reads are not validated
  /\ Building(i)                         \* the build runs
  /\ d \in live                          \* the document is there when read
  /\ <<i, d>> \notin entries \cup pending \* not indexed or being indexed
  /\ pending' = pending \cup {<<i, d>>}  \* the page holds it
  /\ UNCHANGED <<live, deleted, defined, failed, entries, otherLive, tPc, tSeen>>

\* ... and commits its entry later, whether the document is still there or not.
PageCommit(p) ==
  /\ p \in pending                       \* an open page
  /\ Building(p[1])                      \* whose build still runs
  /\ entries' = entries \cup {p}         \* its entry commits
  /\ pending' = pending \ {p}            \* the page is done
  /\ UNCHANGED <<live, deleted, defined, failed, otherLive, tPc, tSeen>>

\* A build fails after some pages committed: the index stays defined, marked
\* failed, with its entries on disk.
Fail(i) ==
  /\ Building(i)                          \* the build runs
  /\ \E d \in Docs : <<i, d>> \in entries \* and committed a page
  /\ failed' = failed \cup {i}            \* it is marked failed
  /\ pending' = {p \in pending : p[1] # i} \* its open pages are dropped
  /\ UNCHANGED <<live, deleted, defined, entries, otherLive, tPc, tSeen>>

\* ---------------------------------------------------------------- spec

\* One of the truncate's own steps.
TruncateStep == Chunks \/ Read \/ Commit

\* Any actor's step.
Next ==
  \/ TruncateStep
  \/ \E i \in NewIndexes : Create(i) \/ Fail(i)
  \/ \E i \in NewIndexes, d \in Docs : PageValidated(i, d) \/ PageRead(i, d)
  \/ \E p \in pending : PageCommit(p)

\* The truncate keeps getting turns, so it finishes.
Spec == Init /\ [][Next]_vars /\ WF_vars(TruncateStep)

\* Once the truncate is done, no index entry names a document it deleted.
INV_NoEntryForTruncatedDoc ==
  tPc = "done" => \A e \in entries : e[2] \notin deleted

\* The cleanup never deletes the other collection's document.
INV_OtherCollectionIntact == otherLive

\* The truncate finishes, whatever the index creator and the backfills do.
LIVE_TruncateFinishes == <>(tPc = "done")
====
