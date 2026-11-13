---- MODULE NacEdgeRefreshOnWrite ----
\* The story, in plain words.
\*
\* One database transaction writes documents. For some documents, a document
\* projector (or a source grant) also writes node access control (NAC) tuples
\* into the same transaction. The edge keeps a snapshot of the NAC grants, so
\* a transaction whose NAC writes commit owes a rebuild of that snapshot when
\* it commits. A transaction that writes no NAC tuple owes nothing: most
\* projected writes touch no NAC tuple, and must not pay for a rebuild.
\*
\* Each NAC tuple write registers one rebuild as commit work. The transaction
\* may set a savepoint and roll back to it: every write, and every
\* registration, made after the savepoint is undone. When the transaction
\* commits, every surviving registration rebuilds the snapshot. Several NAC
\* writes in one transaction rebuild it several times; that is accepted, as
\* such transactions are rare.
\*
\* What must never happen:
\*   1. The transaction commits with a NAC write that survived, and the
\*      snapshot is not rebuilt. (INV_RebuiltAfterNacWrite)
\*   2. The snapshot is rebuilt for a transaction whose surviving writes
\*      touched no NAC tuple. (INV_NoRebuildWithoutNacWrite)
\*
\* The Rust code this models:
\*   - a NAC tuple write marks it       crates/acp/src/zanzibar/store/transaction.rs
\*                                       TransactionZanzibarStore::written;
\*                                       crates/datastore/src/multistore.rs
\*                                       NamespaceView::node_access_written
\*   - what the mark registers          crates/db/src/nac/edge.rs EdgeRefreshOwed::written,
\*                                       NacManager::refresh_edge_on_commit (OnCommit::Async)
\*   - the hook on every db transaction crates/db/src/database/mod.rs DB::new_db_txn
\*   - savepoints drop registrations    crates/storage/src/corekv/callbacks.rs TxnCallbacks::mark,
\*                                       TxnCallbacks::truncate
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Docs,      \* the documents the transaction may write
  NacDocs,   \* those whose write also writes NAC tuples (a projector grant)
  Bug        \* "None" for the real code; otherwise the guard a RED config breaks

ASSUME NacDocs \subseteq Docs                         \* only real documents write NAC tuples
ASSUME Bug \in {"None", "EveryWrite", "KeepOnRollback"}  \* the real code or one RED

\* RED EveryWrite: every projected document write registers a rebuild, as
\* the projector hook did before, NAC tuple or not. RED KeepOnRollback: a
\* savepoint rolled back undoes the writes but keeps the registrations made
\* after it.

VARIABLES
  written,   \* the documents written so far, in order (what survives rollbacks)
  regs,      \* how many rebuilds are registered
  saved,     \* the savepoint, if any: how many writes and registrations it kept
  done,      \* TRUE once the transaction committed
  rebuilds   \* how many times the commit rebuilt the snapshot

vars == <<written, regs, saved, done, rebuilds>>

\* The set of documents in the sequence s.
Elems(s) == {s[i] : i \in 1..Len(s)}

TypeOK ==
  /\ written \in Seq(Docs)                                 \* writes are documents
  /\ regs \in 0..Cardinality(Docs)                         \* at most one per write
  /\ saved \in {<<>>} \cup [w : 0..Cardinality(Docs), r : 0..Cardinality(Docs)]  \* none, or one savepoint
  /\ done \in BOOLEAN                                      \* committed yet
  /\ rebuilds \in 0..Cardinality(Docs)                     \* rebuild count

Init ==
  /\ written = <<>>                                        \* nothing written
  /\ regs = 0                                              \* nothing registered
  /\ saved = <<>>                                          \* no savepoint
  /\ done = FALSE                                          \* still open
  /\ rebuilds = 0                                          \* nothing rebuilt

\* The transaction writes document d once. A NAC write registers a rebuild;
\* RED EveryWrite registers one for every document.
Write(d) ==
  /\ ~done                                                 \* only while open
  /\ d \notin Elems(written)                               \* each document once
  /\ Len(written) < Cardinality(Docs)                      \* keeps the counts bounded
  /\ written' = Append(written, d)                         \* the write
  /\ regs' = IF d \in NacDocs \/ Bug = "EveryWrite"
               THEN regs + 1                               \* register a rebuild
               ELSE regs                                   \* no NAC tuple, nothing owed
  /\ UNCHANGED <<saved, done, rebuilds>>

\* The transaction sets a savepoint (one at a time keeps the model small).
Save ==
  /\ ~done                                                 \* only while open
  /\ saved = <<>>                                          \* none set
  /\ saved' = [w |-> Len(written), r |-> regs]             \* remember where things stood
  /\ UNCHANGED <<written, regs, done, rebuilds>>

\* The transaction rolls back to its savepoint: the writes made after it are
\* undone, and so are the registrations (TxnCallbacks::truncate; RED
\* KeepOnRollback keeps them).
Rollback ==
  /\ ~done                                                 \* only while open
  /\ saved # <<>>                                          \* a savepoint to go back to
  /\ written' = SubSeq(written, 1, saved.w)                \* later writes undone
  /\ regs' = IF Bug = "KeepOnRollback" THEN regs ELSE saved.r  \* later registrations dropped
  /\ saved' = <<>>                                         \* the savepoint is used up
  /\ UNCHANGED <<done, rebuilds>>

\* The transaction commits: every surviving registration rebuilds.
Commit ==
  /\ ~done                                                 \* once
  /\ done' = TRUE                                          \* committed
  /\ rebuilds' = regs                                      \* each registration runs
  /\ UNCHANGED <<written, regs, saved>>

Next ==
  \/ \E d \in Docs : Write(d)                              \* a document write
  \/ Save                                                  \* a savepoint
  \/ Rollback                                              \* a rollback to it
  \/ Commit                                                \* the commit

Spec == Init /\ [][Next]_vars

\* Whether a NAC write survived to the commit.
NacWritten == Elems(written) \cap NacDocs # {}

\* Promise 1: a committed NAC write always gets a rebuild.
INV_RebuiltAfterNacWrite == (done /\ NacWritten) => rebuilds >= 1

\* Promise 2: no rebuild for a transaction whose surviving writes touched no
\* NAC tuple.
INV_NoRebuildWithoutNacWrite == (done /\ ~NacWritten) => rebuilds = 0
====
