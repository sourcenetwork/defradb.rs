---- MODULE ChunkMerge ----
\* The story: an article with a `@chunk` text lives on two peers. Each peer
\* edits it on its own and the peers swap their edits later. An edit sets the
\* text, and creates the chunks of the new text on the peer that made it.
\* Chunks travel to the other peer as documents of their own, at any time.
\*
\* The text merges as a "last writer wins" field: the edit with the higher
\* height wins, and two edits at the same height are broken by comparing the
\* two texts, the same way on every peer.
\*
\* A reader shows the chunks of the current text: it cuts the text itself to
\* find them. A merge that keeps another peer's text builds the chunks of that
\* text this peer lacks: those chunks may never travel, for example when a
\* peer syncs only the article.
\*
\* A chunk document's ID comes from its first block. A chunk's blocks are not
\* signed, so two peers that build the chunk of one text give it the same ID
\* and hold one document. Signed, each peer's copy has its own ID, and a peer
\* that receives the other's copy holds two documents for one chunk.
\*
\* An edit can encrypt its text. Each node encrypts a chunk with a key of its
\* own, so a chunk another node builds would get an ID of its own. A merge
\* that keeps an encrypted text therefore builds none of its chunks. They come
\* from the edit's writer instead: a chunk's push follows its article's push,
\* and a document sync of the article sends the chunks of its current text.
\* Until they arrive, the chunks of that text are owed to the peer.
\*
\* Two peers that write the same text apart and encrypt it therefore each hold
\* a document of their own for one chunk, and once the chunks travel both hold
\* both. A reader shows one of them: of the documents with one chunk's hash,
\* the one with the smallest DocID. Every peer picks the same one from the same
\* documents.
\*
\* A peer can also send a chunk document that lies. One lie is a chunk whose
\* hash is not the hash of its text. Another is a new version of a chunk the
\* receiver already stores, which would change it after it was created. The
\* merge guard refuses both before anything is stored.
\*
\* A peer may delete the article. That delete also deletes every chunk it
\* stores, and each chunk delete travels on its own, so a chunk delete can reach
\* a peer before the article's delete. A peer can also send a chunk delete no
\* article delete made. A delete names its chunk only through the chunk's first
\* block, so it can't merge on a peer before that block has merged there. It
\* waits while the article here is live or has not arrived, and merges once
\* the article is gone. A chunk the text does not use now may be used again by
\* a later edit, so it waits too.
\*
\* What must never happen:
\*   - once every edit and chunk has reached both peers, they show different
\*     chunks, or miss a chunk of the text (INV_SettledPeersAgree);
\*   - a peer stores a chunk whose hash does not match its text (INV_NoForgedChunk);
\*   - a stored chunk is changed after it was created (INV_ChunksNeverChange);
\*   - a live article loses a chunk its text uses (INV_UsedChunksStay);
\*   - a peer that holds the article's delete refuses one of its chunk deletes
\*     for good, so it keeps chunks the other peers deleted (INV_DeletesNotLost);
\*   - a live article's text has a chunk this peer does not store, other than
\*     the chunks of an encrypted text still on their way (INV_TextChunksStored);
\*   - a peer waits for chunks of a text that is not encrypted (INV_OwedIsEncrypted);
\*   - a reader sees one chunk as two documents (INV_EachChunkOnce);
\*   - without encryption, a peer stores one chunk as two documents
\*     (INV_PlainChunkStoredOnce).
\*
\* Code it models:
\*   crates/engine/query/src/plan/chunk_parent.rs  parent_hashes (the read rule: cut the text)
\*   crates/database/db/src/chunk/sync/mod.rs         plan_chunks (an edit's chunks, from its text)
\*   crates/building-blocks/crdt/src/field/lww.rs            merge (the text's tie break)
\*   crates/database/db/src/chunk/guard.rs            check_chunk_merge
\*   crates/database/db/src/merge/merge_handler/composite_persist.rs  the guard's call on a chunk merge
\*   crates/database/db/src/chunk/peer_delete.rs      delete_must_wait (a chunk delete waits for its article)
\*   crates/database/db/src/merge/merge_handler/doc_identity.rs  resolve_composite_doc_id_in_txn (a delete needs its chunk's first block)
\*   crates/database/db/src/merge/merge_handler/composite.rs     process_composite_delta_locked (older blocks merge first)
\*   crates/database/db/src/chunk/rebuild.rs          build_missing (a merge builds the chunks it lacks,
\*                                                     but not those of an encrypted text)
\*   crates/database/db/src/chunk/follow.rs           current_chunk_ids (a document sync sends the
\*                                                     chunks of the current text; ChunkFollow.tla)
\*   crates/database/p2p/src/sync/coordinator/broadcast.rs  SyncCoordinator::push_jobs (a chunk's
\*                                                     push follows its article's; ChunkFollow.tla)
\*   crates/database/db/src/chunk/sync/write.rs       ChunkWrites::create (unsigned chunk blocks)
\*   crates/database/db/src/block/builder/write.rs    compute_document_blocks (the DocID is the first block's CID)
\*   crates/engine/query/src/plan/chunk_once.rs       shown_copy (a reader shows the smallest DocID of a hash)
\*   crates/engine/query/src/plan/chunk_gate.rs       ChunkGate::allows (the query's read gate asks shown_copy)
\* The deleted-parent guard, the read-time hash check and races with a reader are
\* ChunkRead.tla's. An encrypted chunk's hash can't be checked at merge, so a
\* forged encrypted chunk is not modelled here. A copy a reader can't decrypt
\* fails the read-time hash check, so the reader picks among the others; that
\* check is ChunkRead.tla's, and every copy here can be read.
\* Numbers and finite sets are all this model needs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Peers,       \* the two peers, for example {p1, p2}
  MaxEdits,    \* how many edits each peer makes, at most
  CheckHash,   \* TRUE: the merge refuses a chunk whose hash does not match its text (the fix)
  Immutable,   \* TRUE: the merge refuses any change to a chunk that is already stored (the fix)
  Deletes,     \* TRUE: the article and its chunks can be deleted (kept off where a model would be too big)
  DeleteRule,  \* how a chunk delete is judged: "wait" (the fix), "used", "none" or "refuse"
  GenesisFirst, \* TRUE: a chunk delete merges only after the chunk's first block (the fix)
  Rebuild,     \* TRUE: a merge builds the merged text's chunks this peer lacks (the fix)
  SameIds,     \* TRUE: chunk blocks are unsigned, so a chunk's ID is the same on every peer (the fix)
  Encrypt,     \* TRUE: an edit may encrypt its text
  Ship,        \* TRUE: an encrypted text's chunks travel with its article's push and sync (the fix)
  OneCopy      \* TRUE: a reader shows one document per chunk, the smallest DocID (the fix)

\* Each peer edits at least once.
ASSUME MaxEdits \in Nat \ {0}
\* The switches are yes-or-no.
ASSUME CheckHash \in BOOLEAN
ASSUME Immutable \in BOOLEAN
\* Deletes are on or off.
ASSUME Deletes \in BOOLEAN
\* "used" waits only for a chunk the text uses now, "none" merges every chunk
\* delete, and "refuse" refuses a blocked one for good.
ASSUME DeleteRule \in {"wait", "used", "none", "refuse"}
\* Needing the chunk's first block is on or off.
ASSUME GenesisFirst \in BOOLEAN
\* The merge's rebuild and unsigned chunks are each on or off.
ASSUME Rebuild \in BOOLEAN
ASSUME SameIds \in BOOLEAN
\* Encryption and the shipping of encrypted chunks are each on or off.
ASSUME Encrypt \in BOOLEAN
ASSUME Ship \in BOOLEAN
\* Showing one copy of a chunk is on or off.
ASSUME OneCopy \in BOOLEAN

\* The texts an edit may write.
Texts == {"t1", "t2", "t3"}

\* The chunks the splitter cuts each text into.
ChunksOf(t) == CASE t = "t1" -> {"a", "b"}
                 [] t = "t2" -> {"a", "c"}
                 [] t = "t3" -> {"d"}

\* How two texts compare in a tie: the bytes of the text.
TextRank(t) == CASE t = "t1" -> 1 [] t = "t2" -> 2 [] t = "t3" -> 3

\* Every chunk any text is cut into.
AllChunks == UNION {ChunksOf(t) : t \in Texts}

\* The ID of chunk c's document when peer p builds it. Unsigned, it depends
\* only on the chunk; signed, on the peer whose key signed it too.
ChunkId(p, c) == IF SameIds THEN <<c, "any">> ELSE <<c, p>>

\* The ID of chunk c's document when peer p builds it for a text that is
\* encrypted (x) or not. An encrypted chunk takes p's own key, so its ID is p's.
DocId(p, c, x) == IF x THEN <<c, p>> ELSE ChunkId(p, c)

\* Every chunk document ID there can be.
AllIds == AllChunks \X (Peers \cup {"any"})

\* Whether an edit encrypts its text: never with encryption off.
EncChoices == IF Encrypt THEN BOOLEAN ELSE {FALSE}

\* An edit: who made it, at which height, the text it wrote, and whether that
\* text is encrypted.
Edits == [peer : Peers, height : 1..(2 * MaxEdits), text : Texts, enc : BOOLEAN]

VARIABLES
  made,      \* every edit made so far, by anyone
  known,     \* known[p]: the edits peer p has merged
  chunks,    \* chunks[p]: the chunks stored on peer p
  count,     \* count[p]: how many edits peer p has made
  forged,    \* forged[p]: chunks stored on p whose hash does not match their text
  changed,   \* changed[p]: chunks on p that were changed after they were stored
  dead,      \* dead[p]: whether peer p holds the article's delete
  chunkDels, \* the chunks some article delete deleted, whose deletes now travel
  gone,      \* gone[p]: the chunks deleted on peer p
  refused,   \* refused[p]: the chunk deletes peer p refused for good
  ids,       \* ids[p]: the IDs of the chunk documents peer p stores
  owed       \* owed[p]: encrypted edits p merged whose chunks are still on their way to p

\* All the variables, for stuttering steps.
vars == <<made, known, chunks, count, forged, changed, dead, chunkDels, gone, refused, ids, owed>>

\* The edit among `es` whose text wins: highest height first, then the
\* bigger text.
Winner(es) ==
  CHOOSE e \in es : \A f \in es :                                 \* e beats every other edit f:
    \/ f.height < e.height                                         \* f is lower,
    \/ (f.height = e.height /\ TextRank(f.text) <= TextRank(e.text)) \* or as high and not bigger

\* The text peer p holds: the winner by height, then text bytes.
TextOn(p) == Winner(known[p]).text

\* The chunks the read rule allows on peer p: those it cuts from the text p holds.
Allowed(p) == ChunksOf(TextOn(p))

\* The chunks a reader on peer p sees: allowed, stored there and not deleted.
\* A peer that has merged nothing, or the article's delete, has nothing to show.
Shown(p) == IF known[p] = {} \/ dead[p] THEN {} ELSE (Allowed(p) \cap chunks[p]) \ gone[p]

\* The chunk documents peer p stores for chunk c.
Docs(p, c) == {d \in ids[p] : d[1] = c}

\* The rank of a chunk in the DocID order used by the model.
ChunkRank(c) == CASE c = "a" -> 0 [] c = "b" -> 1 [] c = "c" -> 2 [] c = "d" -> 3

\* The rank of a DocID writer. "any" stands for an unsigned chunk ID, which is
\* the same on every peer.
WriterRank(w) ==
  IF w = "any" THEN Cardinality(Peers) ELSE IF w = "p1" THEN 0 ELSE 1

\* A numeric order for DocIDs. The code compares DocID strings; the order only
\* has to be total and identical on every peer.
DocRank(d) == ChunkRank(d[1]) * (Cardinality(Peers) + 1) + WriterRank(d[2])

\* The smallest DocID among docs.
MinDoc(docs) == CHOOSE d \in docs : \A e \in docs : DocRank(d) <= DocRank(e)

\* The documents a reader on p shows for chunk c. With the fix, one readable
\* copy: the one with the smallest DocID, which every peer picks from the same
\* documents. The bug shows every one.
ShownDocs(p, c) ==
  IF OneCopy /\ Docs(p, c) # {}
  THEN {MinDoc(Docs(p, c))}              \* the smallest DocID, the same everywhere
  ELSE Docs(p, c)                         \* every copy, or none stored

\* Whether a chunk delete of c must not merge on p yet: the article is not gone
\* here. The "used" rule blocks only a chunk the article uses now.
Blocked(p, c) ==
  /\ ~dead[p]                                                   \* the article is not gone here
  /\ (DeleteRule = "used" => (known[p] = {} \/ c \in Allowed(p)))  \* and, for "used", uses c

\* The heights an edit may use: one above what the peer has merged.
NextHeight(p) ==
  IF known[p] = {} THEN 1                       \* a first edit is at height 1
  ELSE 1 + Winner(known[p]).height              \* later ones one above the top

\* The type of every variable.
TypeOK ==
  /\ made \subseteq Edits                                          \* edits are edits
  /\ known \in [Peers -> SUBSET Edits]                              \* each peer knows some
  /\ chunks \in [Peers -> SUBSET AllChunks]                         \* stored chunks are real chunks
  /\ count \in [Peers -> 0..MaxEdits]                               \* edits made, per peer
  /\ forged \in [Peers -> SUBSET AllChunks]                         \* forged ones are real chunk names
  /\ changed \in [Peers -> SUBSET AllChunks]                        \* changed ones are real chunk names
  /\ dead \in [Peers -> BOOLEAN]                                     \* each peer holds the delete or not
  /\ chunkDels \subseteq AllChunks                                   \* deleted chunks are real chunk names
  /\ gone \in [Peers -> SUBSET AllChunks]                           \* so are the ones gone on a peer
  /\ refused \in [Peers -> SUBSET AllChunks]                        \* and the refused ones
  /\ ids \in [Peers -> SUBSET AllIds]                               \* stored documents have chunk IDs
  /\ owed \in [Peers -> SUBSET Edits]                               \* owed chunks belong to edits

\* At the start no one has edited anything.
Init ==
  /\ made = {}                        \* no edits yet
  /\ known = [p \in Peers |-> {}]      \* no peer knows any
  /\ chunks = [p \in Peers |-> {}]     \* no chunks stored
  /\ count = [p \in Peers |-> 0]       \* no peer has edited
  /\ forged = [p \in Peers |-> {}]     \* nothing forged is stored
  /\ changed = [p \in Peers |-> {}]    \* nothing was changed
  /\ dead = [p \in Peers |-> FALSE]     \* the article is not deleted
  /\ chunkDels = {}                     \* no chunk delete exists
  /\ gone = [p \in Peers |-> {}]       \* no chunk is deleted
  /\ refused = [p \in Peers |-> {}]    \* nothing was refused
  /\ ids = [p \in Peers |-> {}]        \* no chunk document is stored
  /\ owed = [p \in Peers |-> {}]       \* and nothing is owed

\* Peer p edits the article to text t, encrypted when x: one write sets the
\* text, and the chunks of t are created on p in the same transaction.
Edit(p, t, x) ==
  \* p still has edits left.
  /\ count[p] < MaxEdits
  \* The new edit sits one above what p has seen.
  /\ LET e == [peer |-> p, height |-> NextHeight(p), text |-> t, enc |-> x]
     IN /\ made' = made \cup {e}                            \* the edit exists
        /\ known' = [known EXCEPT ![p] = @ \cup {e}]         \* and p has it
  /\ chunks' = [chunks EXCEPT ![p] = @ \cup ChunksOf(t)]    \* p stores t's chunks
  \* A chunk p already stores is not built again.
  /\ ids' = [ids EXCEPT ![p] = @ \cup {DocId(p, c, x) : c \in ChunksOf(t) \ chunks[p]}]
  /\ count' = [count EXCEPT ![p] = @ + 1]                  \* one more edit by p
  /\ ~dead[p]                                              \* p's article is not deleted
  /\ UNCHANGED <<owed, forged, changed, dead, chunkDels, gone, refused>>  \* no lie or delete is involved

\* Peer p merges an edit another peer made. The text merges by its tie break.
\* With the fix the merge then builds the chunks of the text that won that p
\* lacks, in the same transaction. A text that is encrypted is not built: when
\* it is the new edit's, its writer's chunks are owed to p, and come with the
\* article.
MergeEdit(p) ==
  \E e \in made \ known[p] :                      \* an edit p lacks
    LET k == known[p] \cup {e}                      \* what p knows after the merge
        w == Winner(k)                              \* the edit whose text won
        t == w.text                                 \* the text that won
        build == IF Rebuild /\ ~dead[p] /\ ~w.enc   \* a deleted article or an encrypted text builds nothing
                 THEN ChunksOf(t) \ chunks[p]        \* the chunks of t p lacks
                 ELSE {}
    IN /\ known' = [known EXCEPT ![p] = k]          \* the edit is merged on p
       /\ chunks' = [chunks EXCEPT ![p] = @ \cup build]                         \* p stores the built chunks
       /\ ids' = [ids EXCEPT ![p] = @ \cup {ChunkId(p, c) : c \in build}]       \* under p's IDs for them
       \* With the fix, an encrypted text that just won is owed to p by its writer.
       /\ owed' = [owed EXCEPT ![p] = IF Ship /\ w.enc /\ w = e THEN @ \cup {e} ELSE @]
       /\ UNCHANGED <<made, count, forged, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* The chunks of an owed encrypted edit reach p: the writer's documents for
\* the edit's text, sent with the article's push or a document sync of it.
ShipChunks(p) ==
  \E e \in owed[p] :                                                  \* an edit whose chunks p is owed
    LET docs == {d \in ids[e.peer] : d[1] \in ChunksOf(e.text)}        \* its writer's chunk documents
    IN /\ chunks' = [chunks EXCEPT ![p] = @ \cup {d[1] : d \in docs}]  \* p stores their chunks
       /\ ids' = [ids EXCEPT ![p] = @ \cup docs]                       \* as those documents
       /\ owed' = [owed EXCEPT ![p] = @ \ {e}]                         \* and is owed them no more
       /\ UNCHANGED <<made, known, count, forged, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* A chunk document reaches peer p from the peer that stores it. One with an
\* ID p already stores is the same document, so only a new ID adds one.
MergeChunk(p) ==
  \E q \in Peers \ {p} :              \* another peer q
    \E d \in ids[q] \ ids[p] :         \* stores a chunk document p lacks
      \* The merge guard accepts it: its hash is its text's, and it never changes.
      /\ chunks' = [chunks EXCEPT ![p] = @ \cup {d[1]}]  \* p now stores its chunk
      /\ ids' = [ids EXCEPT ![p] = @ \cup {d}]           \* as this document
      /\ UNCHANGED <<owed, made, known, count, forged, changed, dead, chunkDels, gone, refused>>  \* nothing else changes

\* A peer sends p a chunk whose hash does not match its text.
PlantForged(p) ==
  /\ ~CheckHash                                         \* only when the merge does not check the hash
  /\ \E c \in AllChunks :                               \* some chunk name
       /\ chunks' = [chunks EXCEPT ![p] = @ \cup {c}]   \* p stores it
       /\ forged' = [forged EXCEPT ![p] = @ \cup {c}]   \* and it is a forgery
  /\ UNCHANGED <<owed, made, known, count, changed, dead, chunkDels, gone, refused, ids>>  \* nothing else changes

\* A peer sends p a new version of a chunk p already stores, with a hash that
\* matches the new version. The hash check alone can't tell.
Rewrite(p) ==
  /\ ~Immutable                                         \* only when the merge allows changes
  /\ \E c \in chunks[p] :                               \* a chunk p stores
       changed' = [changed EXCEPT ![p] = @ \cup {c}]    \* is now a different chunk
  /\ UNCHANGED <<owed, made, known, count, chunks, forged, dead, chunkDels, gone, refused, ids>>  \* nothing else changes

\* Peer p deletes the article it holds, and every chunk it stores with it.
DeleteArticle(p) ==
  /\ Deletes                                              \* deletes are modelled
  /\ known[p] # {} /\ ~dead[p]                            \* p holds a live article
  /\ dead' = [dead EXCEPT ![p] = TRUE]                     \* p deletes it
  /\ chunkDels' = chunkDels \cup chunks[p]                 \* its chunk deletes now travel
  /\ gone' = [gone EXCEPT ![p] = @ \cup chunks[p]]         \* and its chunks are gone on p
  /\ UNCHANGED <<owed, made, known, chunks, count, forged, changed, refused, ids>>  \* nothing else changes

\* The article's delete reaches peer p from a peer that holds it.
MergeArticleDelete(p) ==
  /\ ~dead[p] /\ \E q \in Peers : dead[q]                  \* p lacks a delete some peer holds
  /\ dead' = [dead EXCEPT ![p] = TRUE]                     \* p holds it now
  /\ UNCHANGED <<owed, made, known, chunks, count, forged, changed, chunkDels, gone, refused, ids>>  \* nothing else changes

\* A delete of chunk c reaches peer p: an honest one an article delete made,
\* or a forged one. DeleteRule decides what p does with it.
ReceiveChunkDelete(p) ==
  /\ Deletes                                              \* deletes are modelled
  /\ \E c \in chunks[p] \ (gone[p] \cup refused[p]) :       \* a chunk p stores and still judges
      \* With "wait" or "used" a blocked delete does not merge yet; it stays on its way.
      /\ (DeleteRule \in {"wait", "used"}) => ~Blocked(p, c)
      \* With "refuse" a blocked delete is refused for good.
      /\ IF DeleteRule = "refuse" /\ Blocked(p, c)
         THEN /\ refused' = [refused EXCEPT ![p] = @ \cup {c}]  \* refused
              /\ UNCHANGED gone                                 \* and the chunk stays
         ELSE /\ gone' = [gone EXCEPT ![p] = @ \cup {c}]        \* the delete merges
              /\ UNCHANGED refused                              \* nothing refused
      /\ UNCHANGED <<owed, made, known, chunks, count, forged, changed, dead, chunkDels, ids>>  \* nothing else changes

\* A delete of chunk c reaches peer p before c does. With the fix it can't name
\* c yet, so it does not merge; the bug merges it with no article check.
ReceiveUnstoredChunkDelete(p) ==
  /\ Deletes                                                   \* deletes are modelled
  /\ ~GenesisFirst                                             \* only when a delete can merge before its chunk
  /\ \E c \in AllChunks \ (chunks[p] \cup gone[p] \cup refused[p]) : \* a chunk p does not store
       gone' = [gone EXCEPT ![p] = @ \cup {c}]                  \* is marked deleted on p
  /\ UNCHANGED <<owed, made, known, chunks, count, forged, changed, dead, chunkDels, refused, ids>>  \* nothing else changes

\* Every step a peer can take.
Next ==
  \E p \in Peers :
    \/ \E t \in Texts, x \in EncChoices : Edit(p, t, x)  \* p edits the article,
    \/ MergeEdit(p)                 \* merges an edit,
    \/ ShipChunks(p)                \* receives an encrypted text's chunks with its article,
    \/ MergeChunk(p)                \* receives a chunk,
    \/ PlantForged(p)               \* is sent a forged chunk,
    \/ Rewrite(p)                   \* is sent a change to a stored chunk,
    \/ DeleteArticle(p)             \* deletes the article,
    \/ MergeArticleDelete(p)        \* merges the article's delete,
    \/ ReceiveChunkDelete(p)        \* is sent a chunk delete,
    \/ ReceiveUnstoredChunkDelete(p) \* or a chunk delete before the chunk

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* No peer stores a chunk whose hash does not match its text.
INV_NoForgedChunk == \A p \in Peers : forged[p] = {}  \* nothing forged anywhere

\* No stored chunk is ever changed.
INV_ChunksNeverChange == \A p \in Peers : changed[p] = {}  \* nothing changed anywhere

\* A live article keeps every stored chunk its text uses.
INV_UsedChunksStay ==
  \A p \in Peers : (known[p] # {} /\ ~dead[p]) => (Allowed(p) \cap chunks[p] \cap gone[p]) = {}  \* per live article

\* A peer that holds the article's delete never refused one of its chunk deletes.
INV_DeletesNotLost == \A p \in Peers : dead[p] => (refused[p] \cap chunkDels) = {}  \* per deleted article

\* Whether the text p holds is an encrypted one whose chunks are on their way.
Owed(p) == Winner(known[p]) \in owed[p]

\* A live article's text has every one of its chunks stored on the peer, once
\* none of them is still on its way with the article.
INV_TextChunksStored ==
  \A p \in Peers : (known[p] # {} /\ ~dead[p] /\ ~Owed(p)) =>  \* per live article not waiting,
    ChunksOf(TextOn(p)) \subseteq chunks[p]                    \* every chunk of the text is stored

\* A peer is owed chunks only of encrypted edits it merged; every other text
\* it merges has its chunks built at once.
INV_OwedIsEncrypted ==
  \A p \in Peers : \A e \in owed[p] : e.enc /\ e \in known[p]  \* per owed edit

\* A reader sees each chunk it shows as exactly one document.
INV_EachChunkOnce ==
  \A p \in Peers : \A c \in Shown(p) :                       \* for every chunk shown on p,
    Cardinality(ShownDocs(p, c)) = 1                         \* one document shows it

\* In a config that permits no encrypted edit, a peer stores each chunk as at
\* most one document: unsigned blocks give every peer's copy the same ID. This
\* is a config guard, not a check that no state happened to contain encryption.
INV_PlainChunkStoredOnce ==
  ~Encrypt =>                                                \* in a config without encryption,
    \A p \in Peers : \A c \in AllChunks :                     \* for every chunk on every peer,
      Cardinality(Docs(p, c)) <= 1                           \* at most one document holds it

\* Every edit and chunk has reached every peer.
Settled ==
  /\ made # {}                                    \* someone edited
  /\ \A p \in Peers : known[p] = made              \* every peer has every edit
  /\ \A p, q \in Peers : chunks[p] = chunks[q]    \* and every chunk
  /\ \A p \in Peers : owed[p] = {}                 \* and no encrypted chunk is still owed
  /\ (Encrypt => \A p, q \in Peers : ids[p] = ids[q]) \* and encryption's extra copies have all arrived
  /\ \A p \in Peers : ~dead[p]                     \* and the article is live

\* Settled peers show the same chunks: all of the text's, and no others.
INV_SettledPeersAgree ==
  Settled =>
    \A p, q \in Peers :                       \* for any two peers,
      /\ Shown(p) = Shown(q)                  \* they show the same chunks,
      /\ Shown(p) = ChunksOf(TextOn(p))       \* exactly those of the text

\* Settled peers show the same document for every chunk. This is the property
\* the smallest-DocID choice gives them when their stored documents agree.
INV_SettledPeersShowSameDocument ==
  Settled =>
    \A p, q \in Peers :                       \* for any two settled peers,
      \A c \in Shown(p) :                     \* and every chunk they show,
        ShownDocs(p, c) = ShownDocs(q, c)     \* they show the same document
====
