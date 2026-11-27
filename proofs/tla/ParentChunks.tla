---- MODULE ParentChunks ----
\* The story: an article has a long text field marked `@chunk`. defradb keeps
\* one small chunk document per piece of the text. Writers update the article's
\* text or delete the article, several at the same time. Each write is one
\* transaction: it reads a snapshot, decides, and commits only if nothing it
\* read has changed since (regolith's commit validation); otherwise it aborts
\* and runs again.
\*
\* The hook that keeps chunks in step runs inside the parent's own transaction:
\* an update writes the new text, creates the chunk documents of that text
\* that don't exist yet, and deletes the ones the new text no longer uses; a
\* delete deletes the article and all its chunks. A deleted document ID can
\* never be made again, so a later edit back to an old text makes its chunks
\* again under a new generation, a new ID (ChunkCleanup.tla models that). Here
\* a chunk is named by its hash alone.
\*
\* Who acts: a local user, who creates the article once; a peer, whose copy of
\* the same article sends its chunks first (P2P order is arbitrary, so chunks
\* can arrive before their article); and the writers (each updates the text to
\* one of a few versions, or deletes the article, a bounded number of times).
\*
\* Creating a chunk document that already exists is an error, so every write
\* first looks up which of its chunks the article already has and skips those.
\*
\* What must never happen:
\*   - a live article whose text uses a chunk that doesn't exist (INV_TextChunksExist);
\*   - a chunk left behind after its article was deleted (INV_NoChunkOfDeletedParent);
\*   - a live article keeps a chunk its text no longer uses (INV_OnlyTextChunks);
\*   - a valid create or update refused because one of its chunks exists (INV_ValidWritesSucceed).
\*
\* Code it models (crates/database/db/src/chunk/sync):
\*   mod.rs      plan_chunks (the chunks a write's text needs), ChunkWrites::sync_chunks,
\*               ChunkWrites::delete_all (missing and unused chunks, and the cascade on delete)
\*   lookup.rs   ChunkCollection::stored_of, ChunkCollection::unneeded (the parent's chunks, in one scan)
\*   crates/database/db/src/write/autocommit/one_op.rs AutoCommitMutator::in_batch (a single write runs as one batch)
\*   crates/database/db/src/write/autocommit/batch.rs  BatchMutator::update_as, delete_as (call the hook)
\*   crates/database/db/src/write/doc.rs               DbDocMutator update and delete (call the hook)
\* Commit validation itself is regolith's (the deletion marker and the same
\* document parts written by two updates conflict); the model takes it as given.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Writers,   \* the writers, for example {w1, w2}
  MaxOps,    \* how many writes each writer finishes, at most
  Validate,  \* TRUE: a commit whose snapshot is stale aborts (regolith's validation)
  SameTxn,   \* TRUE: chunks are created in the parent's transaction; FALSE: in a follow-up one
  Cascade,   \* TRUE: deleting the article deletes its chunks in the same transaction
  Lookup,    \* TRUE: a write skips the chunks that exist; FALSE: it creates every chunk
  Prune      \* TRUE: an update deletes the chunks its new text does not use

\* Each writer finishes at least one write.
ASSUME MaxOps \in Nat \ {0}
\* The first three switches are yes-or-no.
ASSUME Validate \in BOOLEAN /\ SameTxn \in BOOLEAN /\ Cascade \in BOOLEAN
\* So are the last two.
ASSUME Lookup \in BOOLEAN /\ Prune \in BOOLEAN

\* The texts a writer may write. Each is cut into chunks named by their hash.
Texts == {"t1", "t2", "t3"}

\* The chunk hashes the splitter cuts each text into. "a" appears in two texts
\* (an unchanged paragraph keeps its chunk); "b" appears in t1 and t3.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"}
                 [] t = "t2" -> {"a", "c"}
                 [] t = "t3" -> {"d", "b"}

\* Every chunk hash that can ever appear.
Hashes == UNION {ChunksOf(t) : t \in Texts}

\* The writes a writer may choose: update to some text, or delete.
Ops == {[kind |-> "update", text |-> t] : t \in Texts} \cup {[kind |-> "delete", text |-> "t1"]}

VARIABLES
  created,   \* the local user has created the article
  alive,     \* the article exists and is not deleted
  text,      \* the article's text
  chunks,    \* the chunk documents that exist, by hash
  version,   \* how many writes to the article have committed (what validation compares)
  pc,        \* pc[w]: "idle", "active" (inside its transaction), or "follow" (follow-up pending)
  op,        \* op[w]: the write the writer is doing
  snap,      \* snap[w]: the version the writer's snapshot saw
  snapAlive, \* snapAlive[w]: whether the writer's snapshot saw the article alive
  pending,   \* pending[w]: chunks a follow-up transaction still has to create
  done,      \* done[w]: how many writes the writer finished
  failed     \* a valid write was refused

\* All the variables, for stuttering steps.
vars == <<created, alive, text, chunks, version, pc, op, snap, snapAlive, pending, done, failed>>

\* The most commits there can be: the create, then one per write.
MaxVersion == 1 + MaxOps * Cardinality(Writers)

\* What each variable may hold.
TypeOK ==
  \* the article is created or not, and alive or not;
  /\ created \in BOOLEAN
  /\ alive \in BOOLEAN
  \* its text is one of the texts;
  /\ text \in Texts
  \* chunks are a set of hashes;
  /\ chunks \subseteq Hashes
  \* the commit count stays within what the writers can do;
  /\ version \in 0..MaxVersion
  \* each writer is idle, in its transaction, or waiting on its follow-up;
  /\ pc \in [Writers -> {"idle", "active", "follow"}]
  \* each writer's chosen write is one of the writes;
  /\ op \in [Writers -> Ops]
  \* each snapshot saw some version;
  /\ snap \in [Writers -> 0..MaxVersion]
  \* and saw the article alive or not;
  /\ snapAlive \in [Writers -> BOOLEAN]
  \* pending chunks are hashes;
  /\ pending \in [Writers -> SUBSET Hashes]
  \* each writer has finished a bounded number of writes;
  /\ done \in [Writers -> 0..MaxOps]
  \* and a refusal happened or not.
  /\ failed \in BOOLEAN

\* Nothing exists yet: no article and no chunk.
Init ==
  \* The article is not created,
  /\ created = FALSE
  /\ alive = FALSE
  \* its text is a placeholder until the create writes it,
  /\ text = "t1"
  \* and no chunk exists.
  /\ chunks = {}
  \* No write has committed.
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
  \* Nothing was refused.
  /\ failed = FALSE

\* The peer's chunks of the same article arrive before the article does.
Arrive ==
  \* The article is not created here yet;
  /\ ~created
  \* t1's chunks merge in on their own.
  /\ chunks' = chunks \cup ChunksOf("t1")
  /\ UNCHANGED <<created, alive, text, version, pc, op, snap, snapAlive, pending, done, failed>>

\* Whether a write creating the chunks of text t succeeds: it skips the ones
\* that exist, or none of them exists yet.
ChunksFit(t) == Lookup \/ ChunksOf(t) \cap chunks = {}

\* The local user creates the article with t1 and its chunks, in one transaction.
Create ==
  \* The article is not created yet, and its chunks fit.
  /\ ~created
  /\ ChunksFit("t1")
  \* It is alive, holds t1, and t1's chunks exist.
  /\ created' = TRUE
  /\ alive' = TRUE
  /\ text' = "t1"
  /\ chunks' = chunks \cup ChunksOf("t1")
  \* One commit more.
  /\ version' = version + 1
  /\ UNCHANGED <<pc, op, snap, snapAlive, pending, done, failed>>

\* The create writes a chunk that already arrived, and fails.
CreateRefused ==
  \* The article is not created yet, and a chunk it would create exists.
  /\ ~created
  /\ ~ChunksFit("t1")
  \* The create is refused and the user gives up.
  /\ created' = TRUE
  /\ failed' = TRUE
  /\ UNCHANGED <<alive, text, chunks, version, pc, op, snap, snapAlive, pending, done>>

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
  /\ UNCHANGED <<created, alive, text, chunks, version, pending, done, failed>>

\* Writer w's snapshot is stale and validation is on: the commit aborts, and the
\* write starts again from a fresh snapshot.
Abort(w) ==
  \* Validation is on, the writer is in its transaction, and its snapshot is stale.
  /\ Validate
  /\ pc[w] = "active"
  /\ snap[w] # version
  \* It goes back to idle, to begin again; nothing it wrote is kept.
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ UNCHANGED <<created, alive, text, chunks, version, op, snap, snapAlive, pending, done, failed>>

\* Writer w commits an update: the text, and (with SameTxn) its missing chunks.
CommitUpdate(w) ==
  \* The writer is in its transaction doing an update,
  /\ pc[w] = "active"
  /\ op[w].kind = "update"
  \* and either validation is off or its snapshot is still current.
  /\ ~Validate \/ snap[w] = version
  \* Its snapshot saw the article alive (an update of a deleted article finds nothing),
  /\ snapAlive[w]
  \* and its chunks fit, when it creates them now.
  /\ ~SameTxn \/ ChunksFit(op[w].text)
  \* The new text is written;
  /\ text' = op[w].text
  \* with Prune the chunks the new text does not use are deleted in this commit,
  \* and with SameTxn the missing ones are created now too;
  /\ LET kept == IF Prune THEN chunks \cap ChunksOf(op[w].text) ELSE chunks  \* the chunks the update keeps
     IN chunks' = IF SameTxn THEN kept \cup ChunksOf(op[w].text) ELSE kept   \* plus the new text's, now or later
  \* without it, they are left for a follow-up transaction.
  /\ pending' = [pending EXCEPT ![w] = IF SameTxn THEN {} ELSE ChunksOf(op[w].text)]
  /\ pc' = [pc EXCEPT ![w] = IF SameTxn THEN "idle" ELSE "follow"]
  \* One more write has committed, and the writer finished one write.
  /\ version' = version + 1
  /\ done' = [done EXCEPT ![w] = done[w] + 1]
  \* The article stays as alive as it was.
  /\ UNCHANGED <<created, alive, op, snap, snapAlive, failed>>

\* Writer w's update writes a chunk that already exists, and is refused.
UpdateRefused(w) ==
  \* The writer is in its transaction doing an update of the live article,
  /\ pc[w] = "active"
  /\ op[w].kind = "update"
  /\ ~Validate \/ snap[w] = version
  /\ snapAlive[w]
  \* and a chunk it creates in the same commit exists.
  /\ SameTxn
  /\ ~ChunksFit(op[w].text)
  \* The write ends refused.
  /\ failed' = TRUE
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ done' = [done EXCEPT ![w] = done[w] + 1]
  /\ UNCHANGED <<created, alive, text, chunks, version, op, snap, snapAlive, pending>>

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
  \* The text is left as it was; a deleted article shows nothing.
  /\ UNCHANGED <<created, text, op, snap, snapAlive, pending, failed>>

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
  /\ UNCHANGED <<created, alive, text, chunks, version, op, snap, snapAlive, pending, failed>>

\* Without SameTxn: writer w's follow-up transaction creates the chunks it left behind.
FollowUp(w) ==
  \* The writer has a follow-up pending.
  /\ pc[w] = "follow"
  \* It creates the pending chunks,
  /\ chunks' = chunks \cup pending[w]
  \* and is done with them.
  /\ pending' = [pending EXCEPT ![w] = {}]
  /\ pc' = [pc EXCEPT ![w] = "idle"]
  /\ UNCHANGED <<created, alive, text, version, op, snap, snapAlive, done, failed>>

\* Every step the peer, the local user or some writer can take.
Next ==
  \* the peer's chunks arrive,
  \/ Arrive
  \* the user creates the article, or is refused,
  \/ Create
  \/ CreateRefused
  \/ \E w \in Writers :
    \* begin a write,
    \/ \E o \in Ops : Begin(w, o)
    \* abort a stale one,
    \/ Abort(w)
    \* commit an update or a delete,
    \/ CommitUpdate(w)
    \/ UpdateRefused(w)
    \/ CommitDelete(w)
    \* find the article already gone,
    \/ NotFound(w)
    \* or run its follow-up.
    \/ FollowUp(w)

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every chunk a live article's text uses exists.
INV_TextChunksExist == alive => ChunksOf(text) \subseteq chunks

\* A deleted article has no chunks left.
INV_NoChunkOfDeletedParent == (created /\ ~alive /\ ~failed) => chunks = {}

\* No valid create or update is refused.
INV_ValidWritesSucceed == ~failed

\* A live article holds only the chunks its text uses.
INV_OnlyTextChunks == alive => chunks \subseteq ChunksOf(text)
====
