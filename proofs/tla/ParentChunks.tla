---- MODULE ParentChunks ----
\* The story: an article has a long text field marked `@chunk`. defradb keeps,
\* next to the text, a list naming the article's chunks in reading order, and
\* one small chunk document per piece of the text. Writers update the article's
\* text or delete the article, several at the same time. Each write is one
\* transaction: it reads a snapshot, decides, and commits only if nothing it
\* read has changed since (regolith's commit validation); otherwise it aborts
\* and runs again.
\*
\* The hook that keeps chunks in step runs inside the parent's own transaction:
\* an update writes the new text and its chunk list in one document write and
\* creates the chunk documents the list names that don't exist yet; a delete
\* deletes the article and all its chunks. Old chunks that the new list no
\* longer names are kept (a deleted document ID can never be made again, and a
\* later edit back to the old text reuses them); they are hidden because the
\* list doesn't name them.
\*
\* Who acts: the writers (each updates the text to one of a few versions, or
\* deletes the article, a bounded number of times).
\*
\* What must never happen:
\*   - a live article whose list doesn't describe its text (INV_ListMatchesText);
\*   - a live article whose list names a chunk that doesn't exist (INV_ListedChunksExist);
\*   - a chunk left behind after its article was deleted (INV_NoChunkOfDeletedParent).
\*
\* Code it models (crates/db/src/chunk/sync):
\*   mod.rs      set_lists (the list in the parent's write), ChunkWrites::create_missing,
\*               ChunkWrites::delete_all (missing chunks, and the cascade on delete)
\*   crates/db/src/write/autocommit/one_op.rs AutoCommitMutator::in_batch (a single write runs as one batch)
\*   crates/db/src/write/autocommit/batch.rs  BatchMutator::update_as, delete_as (call the hook)
\*   crates/db/src/write/doc.rs               DbDocMutator update and delete (call the hook)
\* Commit validation itself is regolith's (the deletion marker and the same
\* document parts written by two updates conflict); the model takes it as given.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Writers,   \* the writers, for example {w1, w2}
  MaxOps,    \* how many writes each writer finishes, at most
  Validate,  \* TRUE: a commit whose snapshot is stale aborts (regolith's validation)
  SameTxn,   \* TRUE: chunks are created in the parent's transaction; FALSE: in a follow-up one
  Cascade    \* TRUE: deleting the article deletes its chunks in the same transaction

\* Each writer finishes at least one write.
ASSUME MaxOps \in Nat \ {0}
\* The three switches are yes-or-no.
ASSUME Validate \in BOOLEAN /\ SameTxn \in BOOLEAN /\ Cascade \in BOOLEAN

\* The texts a writer may write. Each is cut into chunks named by their hash.
Texts == {"t1", "t2", "t3"}

\* What the splitter makes of each text, in reading order. "a" appears in two
\* texts (an unchanged paragraph keeps its chunk); "b" appears in t1 and t3.
Split == [t \in Texts |-> CASE t = "t1" -> <<"a", "b">>
                           [] t = "t2" -> <<"a", "c">>
                           [] t = "t3" -> <<"d", "b">>]

\* The chunk hashes a text's list names, as a set.
ChunksOf(t) == {Split[t][i] : i \in 1..Len(Split[t])}

\* Every chunk hash that can ever appear.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* The writes a writer may choose: update to some text, or delete.
Ops == {[kind |-> "update", text |-> t] : t \in Texts} \cup {[kind |-> "delete", text |-> "t1"]}

VARIABLES
  alive,     \* the article exists and is not deleted
  text,      \* the article's text
  list,      \* the article's chunk list (the hashes in reading order)
  chunks,    \* the chunk documents that exist, by hash
  version,   \* how many writes to the article have committed (what validation compares)
  pc,        \* pc[w]: "idle", "active" (inside its transaction), or "follow" (follow-up pending)
  op,        \* op[w]: the write the writer is doing
  snap,      \* snap[w]: the version the writer's snapshot saw
  snapAlive, \* snapAlive[w]: whether the writer's snapshot saw the article alive
  pending,   \* pending[w]: chunks a follow-up transaction still has to create
  done       \* done[w]: how many writes the writer finished

vars == <<alive, text, list, chunks, version, pc, op, snap, snapAlive, pending, done>>

\* What each variable may hold.
TypeOK ==
  \* the article is alive or not;
  /\ alive \in BOOLEAN
  \* its text is one of the texts;
  /\ text \in Texts
  \* its list is one of the splitter's outputs;
  /\ list \in {Split[t] : t \in Texts}
  \* chunks are a set of hashes;
  /\ chunks \subseteq Hashes
  \* the commit count stays within what the writers can do;
  /\ version \in 0..(MaxOps * Cardinality(Writers))
  \* each writer is idle, in its transaction, or waiting on its follow-up;
  /\ pc \in [Writers -> {"idle", "active", "follow"}]
  \* each writer's chosen write is one of the writes;
  /\ op \in [Writers -> Ops]
  \* each snapshot saw some version;
  /\ snap \in [Writers -> 0..(MaxOps * Cardinality(Writers))]
  \* and saw the article alive or not;
  /\ snapAlive \in [Writers -> BOOLEAN]
  \* pending chunks are hashes;
  /\ pending \in [Writers -> SUBSET Hashes]
  \* and each writer has finished a bounded number of writes.
  /\ done \in [Writers -> 0..MaxOps]

\* The article was created with text t1, its list, and its chunks, in one transaction.
Init ==
  \* it is alive,
  /\ alive = TRUE
  \* holds t1,
  /\ text = "t1"
  \* its list describes t1,
  /\ list = Split["t1"]
  \* and t1's chunks exist.
  /\ chunks = ChunksOf("t1")
  \* No write has committed since.
  /\ version = 0
  \* Every writer is idle,
  /\ pc = [w \in Writers |-> "idle"]
  \* with some write chosen (overwritten when it starts),
  /\ op = [w \in Writers |-> [kind |-> "delete", text |-> "t1"]]
  \* an empty snapshot,
  /\ snap = [w \in Writers |-> 0]
  /\ snapAlive = [w \in Writers |-> TRUE]
  \* nothing pending,
  /\ pending = [w \in Writers |-> {}]
  \* and nothing finished.
  /\ done = [w \in Writers |-> 0]

\* Writer w starts a transaction for write o: it reads a snapshot of the article.
Begin(w, o) ==
  \* The writer is idle and still has writes to do.
  /\ pc[w] = "idle"
  /\ done[w] < MaxOps
  \* It enters its transaction with that write,
  /\ pc' = [pc EXCEPT ![w] = "active"]
  /\ op' = [op EXCEPT ![w] = o]
  \* remembering the version and liveness its snapshot saw.
  /\ snap' = [snap EXCEPT ![w] = version]
  /\ snapAlive' = [snapAlive EXCEPT ![w] = alive]
  \* Nothing in storage changes yet.
  /\ UNCHANGED <<alive, text, list, chunks, version, pending, done>>

\* Writer w's snapshot is stale and validation is on: the commit aborts, and the
\* write starts again from a fresh snapshot.
Abort(w) ==
  \* Validation is on, the writer is in its transaction, and its snapshot is stale.
  /\ Validate
  /\ pc[w] = "active"
  /\ snap[w] # version
  \* It goes back to idle, to begin again; nothing it wrote is kept.
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ UNCHANGED <<alive, text, list, chunks, version, op, snap, snapAlive, pending, done>>

\* Writer w commits an update: the text, its list, and (with SameTxn) its missing chunks.
CommitUpdate(w) ==
  \* The writer is in its transaction doing an update,
  /\ pc[w] = "active"
  /\ op[w].kind = "update"
  \* and either validation is off or its snapshot is still current.
  /\ ~Validate \/ snap[w] = version
  \* Its snapshot saw the article alive (an update of a deleted article finds nothing).
  /\ snapAlive[w]
  \* The text and its list are written in one document write;
  /\ text' = op[w].text
  /\ list' = Split[op[w].text]
  \* with SameTxn the missing chunks are created now, in the same commit;
  /\ chunks' = IF SameTxn THEN chunks \cup ChunksOf(op[w].text) ELSE chunks
  \* without it, they are left for a follow-up transaction.
  /\ pending' = [pending EXCEPT ![w] = IF SameTxn THEN {} ELSE ChunksOf(op[w].text)]
  /\ pc' = [pc EXCEPT ![w] = IF SameTxn THEN "idle" ELSE "follow"]
  \* One more write has committed, and the writer finished one write.
  /\ version' = version + 1
  /\ done' = [done EXCEPT ![w] = done[w] + 1]
  \* The article stays as alive as it was.
  /\ UNCHANGED <<alive, op, snap, snapAlive>>

\* Writer w commits a delete: the article goes, and (with Cascade) all its chunks.
CommitDelete(w) ==
  \* The writer is in its transaction doing a delete,
  /\ pc[w] = "active"
  /\ op[w].kind = "delete"
  \* and either validation is off or its snapshot is still current.
  /\ ~Validate \/ snap[w] = version
  \* Its snapshot saw the article alive (deleting a deleted article does nothing).
  /\ snapAlive[w]
  \* The article is deleted;
  /\ alive' = FALSE
  \* with Cascade its chunks are deleted in the same commit.
  /\ chunks' = IF Cascade THEN {} ELSE chunks
  \* One more write has committed, and the writer finished one write.
  /\ version' = version + 1
  /\ done' = [done EXCEPT ![w] = done[w] + 1]
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  \* The text and list are left as they were; a deleted article shows neither.
  /\ UNCHANGED <<text, list, op, snap, snapAlive, pending>>

\* Writer w's write finds the article already deleted in its snapshot: nothing to do.
NotFound(w) ==
  \* The writer is in its transaction, and its snapshot is current or unchecked,
  /\ pc[w] = "active"
  /\ ~Validate \/ snap[w] = version
  \* and it saw the article deleted.
  /\ ~snapAlive[w]
  \* The write ends with nothing written.
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ done' = [done EXCEPT ![w] = done[w] + 1]
  /\ UNCHANGED <<alive, text, list, chunks, version, op, snap, snapAlive, pending>>

\* Without SameTxn: writer w's follow-up transaction creates the chunks it left behind.
FollowUp(w) ==
  \* The writer has a follow-up pending.
  /\ pc[w] = "follow"
  \* It creates the pending chunks,
  /\ chunks' = chunks \cup pending[w]
  \* and is done with them.
  /\ pending' = [pending EXCEPT ![w] = {}]
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ UNCHANGED <<alive, text, list, version, op, snap, snapAlive, done>>

\* Every step some writer can take.
Next ==
  \E w \in Writers :
    \* begin a write,
    \/ \E o \in Ops : Begin(w, o)
    \* abort a stale one,
    \/ Abort(w)
    \* commit an update or a delete,
    \/ CommitUpdate(w)
    \/ CommitDelete(w)
    \* find the article already gone,
    \/ NotFound(w)
    \* or run its follow-up.
    \/ FollowUp(w)

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* A live article's list is exactly what the splitter makes of its text.
INV_ListMatchesText == alive => list = Split[text]

\* A live article's list names only chunks that exist.
INV_ListedChunksExist == alive => \A i \in 1..Len(list) : list[i] \in chunks

\* A deleted article has no chunks left.
INV_NoChunkOfDeletedParent == ~alive => chunks = {}
====
