---- MODULE ChunkSigner ----
\* The story: Alice owns an article under a policy. Its `@chunk` text has
\* chunk documents of their own, and those travel to a receiving node R one
\* by one, apart from the article. A chunk carries what its writer made: its
\* text and, once chunks get vectors, the vector that decides what a search
\* finds. So a chunk is an update of its article.
\*
\* Alice writes the text, and her node sends its chunk to R. Bob is a peer
\* that syncs with R. At R he may be granted the right to update the article,
\* and that right may be taken away again. Bob can send R a chunk of the
\* article at any time, signed by him or not signed at all; he cannot sign as
\* Alice. The article itself may reach R before or after any chunk.
\*
\* R merges a chunk in a transaction. With the fix, the transaction reads
\* whether the article has arrived and whether the chunk's signer may update
\* it. A chunk whose article has not arrived waits; one whose signer may not
\* update the article is refused for good. A local provider joins the
\* transaction, so a grant or revoke that commits first aborts the merge and
\* the merge runs again on the new state. A remote provider cannot join it:
\* its signer check is made at the edge before the transaction, so a revoke
\* committed after that check can leave the chunk with a signer who may no
\* longer update the article. That staleness is the remote provider contract,
\* not the local one.
\*
\* With the fix the writer signs a protected article's chunks, so each one
\* names who made it, and R builds no chunk of a protected article itself: a
\* chunk R built would be unsigned, its ID would differ from the writer's
\* signed copy, and R would hold the chunk twice.
\*
\* What must never happen:
\*   - R stores a chunk whose signer could not update the article when the
\*     chunk merged (INV_NoRogueChunk);
\*   - R refuses Alice's chunk for good (INV_WriterChunkKept);
\*   - R holds Alice's chunk as two documents (INV_EachChunkOnce).
\*
\* Code it models:
\*   crates/database/db/src/merge/merge_handler/chunk_signer.rs  check_chunk_signer
\*     (the parent and signer check, in the merge's transaction), parent_state
\*     (a parent not yet stored makes the chunk wait; read_hint validates only
\*     the stored version and deletion marker), linked_parent (the source
\*     link, decrypted when encrypted), prepare_chunk_merge
\*     (a chunk whose parent has not arrived waits, indexed under the parent's
\*     DocID; the signer check before the transaction, for a provider outside it)
\*   crates/database/db/src/merge/governance/judge.rs  release_merged_composite
\*     (the parent's first merge re-drives the chunks waiting for it)
\*   crates/database/db/src/merge/merge_handler/protected_update.rs  check_signed_update
\*     (the chunk is judged by its own signature as an update of its parent)
\*   crates/database/db/src/merge/merge_handler/composite_persist.rs  persist_merged_document
\*     (a new chunk is checked before it is stored)
\*   crates/database/db/src/chunk/sync/mod.rs  plan_chunks (a protected parent's chunks are signed)
\*   crates/database/db/src/chunk/sync/write.rs  ChunkWrites::create (signs them)
\*   crates/database/db/src/chunk/rebuild.rs  build_missing (builds no chunk of a protected parent)
\* One writer is modelled. Two people who may both update the article and
\* write the same text apart each sign their own copy of its chunk, so a peer
\* that gets both holds it twice, as with encrypted chunks in ChunkMerge.tla.
\* A writer started without signing is the Red_Unsigned configuration: its
\* chunks name nobody, so protected chunk replication is unavailable until the
\* node signs. That is the documented consequence of --no-signing, not a
\* rebuild fallback.
\* Who may read a chunk is ChunkRead.tla's, and a chunk's hash and immutability
\* are ChunkMerge.tla's.
\* Numbers and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  CheckParent,   \* TRUE: a merge refuses a chunk whose signer may not update the article (the fix)
  Sign,          \* TRUE: the writer signs a protected article's chunks (the fix)
  WaitForParent, \* TRUE: a chunk whose article has not arrived waits instead of being refused (the fix)
  Provider,      \* "Local": the merge reads the article and grants in its transaction | "Remote": the provider judges at the edge before it
  Rebuild,       \* TRUE: R builds the chunks of a protected article itself (the bug)
  MaxFlips       \* how many grants and revokes of Bob's right R sees, at most

\* The switches are yes-or-no.
ASSUME CheckParent \in BOOLEAN
ASSUME Sign \in BOOLEAN
ASSUME WaitForParent \in BOOLEAN
ASSUME Provider \in {"Local", "Remote"}
ASSUME Rebuild \in BOOLEAN
\* The number of grants and revokes is a natural number.
ASSUME MaxFlips \in Nat

\* Who can sign a chunk: Alice, Bob, or nobody when it is unsigned.
Signers == {"alice", "bob", "none"}

\* A chunk document: whose text it holds and who signed it. Its ID is its
\* first block's CID, so the signer is part of it.
Docs == [text : {"alice", "bob"}, sig : Signers]

\* Alice's chunk as her node writes it.
AliceChunk == [text |-> "alice", sig |-> IF Sign THEN "alice" ELSE "none"]

\* Alice's chunk as R would build it itself: never signed.
BuiltChunk == [text |-> "alice", sig |-> "none"]

VARIABLES
  arrived,  \* whether the article has reached R
  updaters, \* who may update the article at R
  flips,    \* how many grants and revokes R has seen
  inflight, \* chunks sent to R and not yet merged or refused
  pending,  \* merges that have read R's state and not yet committed
  stored,   \* the chunk documents R stores
  rogue,    \* the chunks R stored while their signer could not update the article
  refused   \* the chunks R refused for good

\* All the variables, for stuttering steps.
vars == <<arrived, updaters, flips, inflight, pending, stored, rogue, refused>>

\* The type of every variable.
TypeOK ==
  /\ arrived \in BOOLEAN                                         \* the article is here or not
  /\ updaters \subseteq {"alice", "bob"}                         \* the people who may update it
  /\ flips \in 0..MaxFlips                                       \* grants and revokes so far
  /\ inflight \subseteq Docs                                     \* chunks on their way
  /\ pending \subseteq [doc : Docs, saw : SUBSET {"alice", "bob"}, here : BOOLEAN]  \* merges mid-way
  /\ stored \subseteq Docs                                       \* chunks R holds
  /\ rogue \subseteq Docs                                        \* chunks R should not hold
  /\ refused \subseteq Docs                                      \* chunks refused for good

\* At the start Alice owns the article, nothing has reached R, and Bob may not
\* update it.
Init ==
  /\ arrived = FALSE        \* the article is not at R yet
  /\ updaters = {"alice"}   \* only Alice may update it
  /\ flips = 0              \* no grant or revoke yet
  /\ inflight = {}          \* no chunk sent
  /\ pending = {}           \* no merge running
  /\ stored = {}            \* R holds no chunk
  /\ rogue = {}             \* nothing wrong stored
  /\ refused = {}           \* nothing refused

\* Alice's node sends her chunk to R.
AliceSends ==
  /\ AliceChunk \notin inflight \cup stored \cup refused   \* not sent yet
  /\ inflight' = inflight \cup {AliceChunk}                 \* now on its way
  /\ UNCHANGED <<arrived, updaters, flips, pending, stored, rogue, refused>>  \* nothing else changes

\* Bob sends R a chunk of his own text, signed by him or by nobody.
BobSends ==
  \E s \in {"bob", "none"} :                                \* signed by Bob, or unsigned
    LET d == [text |-> "bob", sig |-> s]                    \* the chunk he sends
    IN /\ d \notin inflight \cup stored \cup refused        \* not sent yet
       /\ inflight' = inflight \cup {d}                     \* now on its way
       /\ UNCHANGED <<arrived, updaters, flips, pending, stored, rogue, refused>>  \* nothing else changes

\* The article reaches R. The bug also builds Alice's chunk there, unsigned.
ParentArrives ==
  /\ ~arrived                                               \* not here yet
  /\ arrived' = TRUE                                        \* now it is
  /\ stored' = IF Rebuild THEN stored \cup {BuiltChunk} ELSE stored  \* the bug builds the chunk
  /\ UNCHANGED <<updaters, flips, inflight, pending, rogue, refused>>  \* nothing else changes

\* Alice grants Bob the right to update the article at R, or takes it away.
FlipBob ==
  /\ flips < MaxFlips                                       \* grants and revokes left
  /\ updaters' = IF "bob" \in updaters THEN {"alice"} ELSE {"alice", "bob"}  \* grant or revoke
  /\ flips' = flips + 1                                     \* one more flip
  /\ UNCHANGED <<arrived, inflight, pending, stored, rogue, refused>>  \* nothing else changes

\* A merge of chunk d starts: it reads whether the article is here and who
\* may update it.
Begin ==
  \E d \in inflight :                                       \* a chunk on its way
    /\ pending' = pending \cup {[doc |-> d, saw |-> updaters, here |-> arrived]}  \* what it read
    /\ inflight' = inflight \ {d}                           \* it is being merged
    /\ UNCHANGED <<arrived, updaters, flips, stored, rogue, refused>>  \* nothing else changes

\* The merge decides on what it read. In its own transaction, a change to what
\* it read since then aborts it, and the chunk goes back to wait for a re-run.
Commit ==
  \E p \in pending :                                        \* a merge mid-way
    LET d == p.doc                                          \* its chunk
        stale == p.saw # updaters \/ p.here # arrived       \* what it read has changed
        allowed == d.sig \in p.saw                          \* its signer may update, as it read
        in_txn == Provider = "Local"                       \* whether the provider joins the transaction
    IN /\ pending' = pending \ {p}                          \* the merge ends
       /\ IF in_txn /\ stale
          THEN \* The transaction aborts and the chunk is merged again later.
               /\ inflight' = inflight \cup {d}             \* back on its way
               /\ UNCHANGED <<stored, rogue, refused>>      \* nothing stored or refused
          ELSE IF CheckParent /\ ~p.here
          THEN \* The article had not arrived: wait for it, or (the bug) refuse.
               IF WaitForParent
               THEN /\ inflight' = inflight \cup {d}        \* back on its way
                    /\ UNCHANGED <<stored, rogue, refused>> \* nothing stored or refused
               ELSE /\ refused' = refused \cup {d}          \* refused for good
                    /\ UNCHANGED <<inflight, stored, rogue>>  \* nothing stored
          ELSE IF CheckParent /\ ~allowed
          THEN \* The signer may not update the article: refused for good.
               /\ refused' = refused \cup {d}               \* refused
               /\ UNCHANGED <<inflight, stored, rogue>>     \* nothing stored
          ELSE \* Stored. It is rogue when its signer may not update the article now.
               /\ stored' = stored \cup {d}                 \* R holds it
               /\ rogue' = IF d.sig \in updaters THEN rogue ELSE rogue \cup {d}  \* judged by now
               /\ UNCHANGED <<inflight, refused>>           \* nothing else
       /\ UNCHANGED <<arrived, updaters, flips>>            \* R's other state stays

\* Every step that can happen.
Next ==
  \/ AliceSends      \* Alice's chunk is sent,
  \/ BobSends        \* Bob sends one,
  \/ ParentArrives   \* the article arrives,
  \/ FlipBob         \* Bob is granted or revoked,
  \/ Begin           \* a merge reads R's state,
  \/ Commit          \* or a merge decides

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* R never holds a chunk whose signer could not update the article when it merged.
INV_NoRogueChunk == rogue = {}

\* R never refuses Alice's chunk for good.
INV_WriterChunkKept == AliceChunk \notin refused

\* R holds Alice's chunk as one document at most.
INV_EachChunkOnce == Cardinality({d \in stored : d.text = "alice"}) <= 1
====
