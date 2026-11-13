---- MODULE NacEdgeRefreshOnce ----
\* The story, in plain words.
\*
\* One database transaction writes documents. For some documents, a document
\* projector (or a source grant) also writes node access control (NAC) tuples
\* into the same transaction. The edge keeps a snapshot of the NAC grants, so
\* a transaction that wrote NAC tuples owes one rebuild of that snapshot when
\* it commits. A transaction that wrote none owes nothing.
\*
\* Each NAC tuple write registers the rebuild as commit work, under one key.
\* The transaction may set a savepoint and roll back to it: every write, and
\* every registration, made after the savepoint is undone. When the
\* transaction commits, each key's first surviving registration runs and the
\* rest are dropped.
\*
\* What must never happen:
\*   1. The transaction commits with a NAC write that survived, and the
\*      snapshot is not rebuilt. (INV_RebuiltAfterNacWrite)
\*   2. The snapshot is rebuilt more than once, or for a transaction whose
\*      surviving writes touched no NAC tuple. (INV_AtMostOneRebuild)
\*
\* The Rust code this models:
\*   - a NAC tuple write marks it       crates/acp/src/zanzibar/store/transaction.rs
\*                                       TransactionZanzibarStore::written;
\*                                       crates/datastore/src/multistore.rs
\*                                       NamespaceView::node_access_written
\*   - what the mark registers          crates/db/src/nac/edge.rs EdgeRefreshOwed::written,
\*                                       NacManager::refresh_edge_on_commit (OnCommit::AsyncOnce)
\*   - the hook on every db transaction crates/db/src/database/mod.rs DB::new_db_txn
\*   - savepoints drop registrations    crates/storage/src/corekv/callbacks.rs TxnCallbacks::mark,
\*                                       TxnCallbacks::truncate
\*   - one run per key at commit        crates/storage/src/corekv/callbacks.rs TxnCallbacks::into_ends
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Docs,      \* the documents the transaction may write
  NacDocs,   \* those whose write also writes NAC tuples (a projector grant)
  Bug        \* "None" for the real code; otherwise the guard a RED config breaks

ASSUME NacDocs \subseteq Docs                   \* only real documents write NAC tuples
ASSUME Bug \in {"None", "PerDoc", "SkipAtRegister"}  \* the real code or one RED

\* RED PerDoc: every document write registers its own unkeyed rebuild, as the
\* projector hook did before. RED SkipAtRegister: the key is skipped when it
\* was registered before, at registration rather than at commit, so a
\* registration a rollback dropped still blocks the next one.

VARIABLES
  written,   \* the documents written so far, in order (what survives rollbacks)
  regs,      \* the registered commit work, in order: "keyed" or "plain"
  saved,     \* the savepoint, if any: how many writes and registrations it kept
  seen,      \* TRUE once the key was ever registered (only SkipAtRegister reads it)
  done,      \* TRUE once the transaction committed
  rebuilds   \* how many times the commit rebuilt the snapshot

vars == <<written, regs, saved, seen, done, rebuilds>>

\* The set of documents in the sequence s.
Elems(s) == {s[i] : i \in 1..Len(s)}

\* How many entries of s equal x.
Count(s, x) == Cardinality({i \in 1..Len(s) : s[i] = x})

TypeOK ==
  /\ written \in Seq(Docs)                               \* writes are documents
  /\ regs \in Seq({"keyed", "plain"})                    \* registrations of two kinds
  /\ saved \in {<<>>} \cup [w : 0..Cardinality(Docs), r : 0..Cardinality(Docs)]  \* none, or one savepoint
  /\ seen \in BOOLEAN                                    \* the flag RED SkipAtRegister reads
  /\ done \in BOOLEAN                                    \* committed yet
  /\ rebuilds \in Nat                                    \* rebuild count

Init ==
  /\ written = <<>>                                      \* nothing written
  /\ regs = <<>>                                         \* nothing registered
  /\ saved = <<>>                                        \* no savepoint
  /\ seen = FALSE                                        \* the key never registered
  /\ done = FALSE                                        \* still open
  /\ rebuilds = 0                                        \* nothing rebuilt

\* The transaction writes document d once. A NAC write registers the keyed
\* rebuild (RED SkipAtRegister: unless the key was registered before);
\* RED PerDoc registers a plain rebuild for every document instead.
Write(d) ==
  /\ ~done                                               \* only while open
  /\ d \notin Elems(written)                             \* each document once
  /\ written' = Append(written, d)                       \* the write
  /\ IF Bug = "PerDoc"
       THEN /\ regs' = Append(regs, "plain")             \* one rebuild per document
            /\ UNCHANGED seen
     ELSE IF d \in NacDocs /\ ~(Bug = "SkipAtRegister" /\ seen)
       THEN /\ regs' = Append(regs, "keyed")             \* register the keyed rebuild
            /\ seen' = TRUE                              \* remember it was registered
     ELSE UNCHANGED <<regs, seen>>                       \* no NAC tuple, nothing owed
  /\ UNCHANGED <<saved, done, rebuilds>>

\* The transaction sets a savepoint (one at a time keeps the model small).
Save ==
  /\ ~done                                               \* only while open
  /\ saved = <<>>                                        \* none set
  /\ saved' = [w |-> Len(written), r |-> Len(regs)]      \* remember where things stood
  /\ UNCHANGED <<written, regs, seen, done, rebuilds>>

\* The transaction rolls back to its savepoint: the writes and registrations
\* made after it are undone (TxnCallbacks::truncate).
Rollback ==
  /\ ~done                                               \* only while open
  /\ saved # <<>>                                        \* a savepoint to go back to
  /\ written' = SubSeq(written, 1, saved.w)              \* later writes undone
  /\ regs' = SubSeq(regs, 1, saved.r)                    \* later registrations dropped
  /\ saved' = <<>>                                       \* the savepoint is used up
  /\ UNCHANGED <<seen, done, rebuilds>>

\* The transaction commits: every plain registration rebuilds, and the keyed
\* one rebuilds once if any registration of it survived (into_ends).
Commit ==
  /\ ~done                                               \* once
  /\ done' = TRUE                                        \* committed
  /\ rebuilds' = Count(regs, "plain")                    \* plain ones each run
                 + (IF Count(regs, "keyed") > 0 THEN 1 ELSE 0)  \* the keyed one runs once
  /\ UNCHANGED <<written, regs, saved, seen>>

Next ==
  \/ \E d \in Docs : Write(d)                            \* a document write
  \/ Save                                                \* a savepoint
  \/ Rollback                                            \* a rollback to it
  \/ Commit                                              \* the commit

Spec == Init /\ [][Next]_vars

\* Whether a NAC write survived to the commit.
NacWritten == Elems(written) \cap NacDocs # {}

\* Promise 1: a committed NAC write always gets its rebuild.
INV_RebuiltAfterNacWrite == (done /\ NacWritten) => rebuilds >= 1

\* Promise 2: at most one rebuild, and none without a surviving NAC write.
INV_AtMostOneRebuild == done => rebuilds = (IF NacWritten THEN 1 ELSE 0)
====
