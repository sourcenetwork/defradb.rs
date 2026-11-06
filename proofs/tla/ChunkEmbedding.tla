---- MODULE ChunkEmbedding ----
\* The story: an article's long text is marked `@chunk`, and the chunk
\* collection carries an `@embedding`, so every chunk document holds a vector
\* made by an embedding model from the chunk's text. The model is a remote
\* service: asking it is slow and may fail, so it is never asked from inside
\* a transaction.
\*
\* Who acts: requests. Each request writes texts to some articles, in one
\* transaction attempt that may run more than once. An attempt reads which
\* chunks exist, and for every chunk its texts need that does not exist yet it
\* looks up the vector in the request's memo (answers keyed by the exact
\* model request, that is by the chunk's text). If every vector is there the
\* attempt commits: the texts and the new chunks with their vectors, unless a
\* commit from another request got in first, in which case it runs again. If
\* some vector is missing the attempt ends without writing, the request asks
\* the model for every missing vector at once (one wave, outside any
\* transaction), and the attempt runs again.
\*
\* What must never happen:
\*   - the model is asked while a transaction is open (INV_NoModelCallInTxn);
\*   - an article's text names a chunk that is missing or has no vector
\*     (INV_ListedChunkHasVector);
\*   - a chunk holds the vector of some other text (INV_VectorIsItsText);
\*   - a write asks the model for a chunk that already existed when its
\*     attempt read the store, so an unchanged paragraph is never embedded
\*     again (INV_UnchangedChunkNoCall);
\*   - a request needs more than one wave (INV_OneWave).
\*
\* Code it models:
\*   crates/db/src/chunk/sync/mod.rs ChunkWrites::create_missing (skip existing
\*     chunks, look up every missing vector, end the attempt on a miss)
\*   crates/db/src/chunk/sync/write.rs ChunkWrites::embed (the memo lookup)
\*   crates/db/src/search/embedding.rs set_embedding_from_inputs,
\*     prepared_embedding_request (the memo key is the exact request)
\*   crates/query/src/prepared.rs PreparedInputMemo::get_or_want, answer_wave,
\*     FirstMiss (a write of several documents carries on past a miss, so the
\*     misses of all its documents go into one wave)
\*   crates/query/src/mutator.rs DocMutator::create_many_as,
\*     crates/db/src/write/autocommit/one_op.rs CreateWork::run and
\*     crates/query/src/plan/mutation/update.rs UpdateNode::next (those loops)
\*   crates/db/src/write/autocommit/one_op.rs AutoCommitMutator::in_batch and
\*     crates/query/src/runner/turn.rs QueryRunner::drive (the wave between attempts)
\* Commit validation is regolith's; the model takes it as given.
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS
  KeyByText,        \* TRUE: the memo key is the chunk's text; FALSE: its position in the text
  WantOnlyMissing,  \* TRUE: only chunks that don't exist yet are embedded; FALSE: every chunk
  ResolveOutside,   \* TRUE: a miss ends the attempt; FALSE: the model is asked on the spot
  WantEveryMiss     \* TRUE: the attempt goes on past a miss and wants them all; FALSE: it stops at the first

\* Each switch is yes or no.
ASSUME KeyByText \in BOOLEAN /\ WantOnlyMissing \in BOOLEAN
ASSUME ResolveOutside \in BOOLEAN /\ WantEveryMiss \in BOOLEAN

\* Two requests and two articles.
Requests == {"r1", "r2"}
\* Articles are numbered.
Articles == {1, 2}

\* The texts, and what the splitter cuts each into, in reading order. "a" is in
\* t1 and t2, the unchanged paragraph of an edit; "b" is in t1 and t3.
Texts == {"t1", "t2", "t3"}
Split == [t \in Texts |-> CASE t = "t1" -> <<"a", "b">>
                           [] t = "t2" -> <<"a", "c">>
                           [] t = "t3" -> <<"d", "b">>]
\* Every chunk text.
Hashes == {"a", "b", "c", "d"}

\* What each request writes: r1 creates article 1 with t1 and article 2 with
\* t3, r2 edits article 1 to t2.
Job == [r \in Requests |-> IF r = "r1" THEN (1 :> "t1" @@ 2 :> "t3") ELSE (1 :> "t2")]

\* A chunk document: its article and its text. Chunks are per article.
Chunks == Articles \X Hashes

\* The chunks a request's texts need, each with its position in its text.
Needed(r) == {<<a, Split[Job[r][a]][i], i>> : a \in DOMAIN Job[r], i \in 1..2}

\* The names of the two positions in a text.
Positions == <<"p1", "p2">>
\* Every key the memo may file a vector under.
Keys == Hashes \cup {"p1", "p2"}
\* What the memo files a chunk's vector under.
Key(n) == IF KeyByText THEN n[2] ELSE Positions[n[3]]

\* No text yet, no vector yet.
None == "none"

VARIABLES
  text,      \* text[a]: the article's committed text, or None
  chunks,    \* the chunk documents that exist
  vecOf,     \* vecOf[c]: the text whose vector chunk c holds, or None
  pc,        \* pc[r]: "idle", "txn" (attempt open), "wave" (asking the model), or "done"
  snap,      \* snap[r]: the chunks the open attempt read
  snapText,  \* snapText[r]: the texts the open attempt read
  memo,      \* memo[r]: the answers the request holds, as <<key, text embedded>>
  wanted,    \* wanted[r]: what the next wave asks, as <<key, text, existed when read>>
  waves,     \* waves[r]: how many waves the request has needed
  callInTxn, \* the model was asked while an attempt was open
  reEmbedded \* the model was asked for a chunk that already existed

vars == <<text, chunks, vecOf, pc, snap, snapText, memo, wanted, waves, callInTxn, reEmbedded>>

\* What each variable may hold.
TypeOK ==
  \* each article has a text or none;
  /\ text \in [Articles -> Texts \cup {None}]
  \* chunks are chunk documents;
  /\ chunks \subseteq Chunks
  \* each chunk holds some text's vector, or none;
  /\ vecOf \in [Chunks -> Hashes \cup {None}]
  \* each request is at one of its steps;
  /\ pc \in [Requests -> {"idle", "txn", "wave", "done"}]
  \* snapshots are a set of chunks and the texts;
  /\ snap \in [Requests -> SUBSET Chunks]
  /\ snapText \in [Requests -> [Articles -> Texts \cup {None}]]
  \* memo answers pair a key with the text the model embedded;
  /\ memo \in [Requests -> SUBSET (Keys \X Hashes)]
  \* wants carry the key, the text and whether the chunk existed;
  /\ wanted \in [Requests -> SUBSET (Keys \X Hashes \X BOOLEAN)]
  \* a request needs a bounded number of waves;
  /\ waves \in [Requests -> 0..4]
  \* the two history flags are yes or no.
  /\ callInTxn \in BOOLEAN /\ reEmbedded \in BOOLEAN

\* Nothing written yet, every request idle with an empty memo.
Init ==
  \* No article has a text,
  /\ text = [a \in Articles |-> None]
  \* no chunk exists,
  /\ chunks = {}
  /\ vecOf = [c \in Chunks |-> None]
  \* every request is idle,
  /\ pc = [r \in Requests |-> "idle"]
  /\ snap = [r \in Requests |-> {}]
  /\ snapText = [r \in Requests |-> [a \in Articles |-> None]]
  \* with nothing in its memo, nothing wanted and no wave yet,
  /\ memo = [r \in Requests |-> {}]
  /\ wanted = [r \in Requests |-> {}]
  /\ waves = [r \in Requests |-> 0]
  \* and the model was never asked wrongly.
  /\ callInTxn = FALSE
  /\ reEmbedded = FALSE

\* The memo's answer for chunk n, if it holds one.
Answered(r, n) == \E m \in memo[r] : m[1] = Key(n)
\* The text whose vector the memo hands chunk n.
Answer(r, n) == (CHOOSE m \in memo[r] : m[1] = Key(n))[2]

\* The chunks attempt r creates: those it did not see, or every one when the
\* bug embeds all chunks.
ToCreate(r) == {n \in Needed(r) : ~WantOnlyMissing \/ <<n[1], n[2]>> \notin snap[r]}

\* The chunks of ToCreate(r) whose vector the memo lacks.
Misses(r) == {n \in ToCreate(r) : ~Answered(r, n)}

\* Request r opens an attempt and reads the store.
Begin(r) ==
  \* The request is not in an attempt or a wave.
  /\ pc[r] = "idle"
  \* It opens its transaction,
  /\ pc' = [pc EXCEPT ![r] = "txn"]
  \* reading which chunks exist and what the texts are.
  /\ snap' = [snap EXCEPT ![r] = chunks]
  /\ snapText' = [snapText EXCEPT ![r] = text]
  \* Nothing is written.
  /\ UNCHANGED <<text, chunks, vecOf, memo, wanted, waves, callInTxn, reEmbedded>>

\* The attempt meets a miss and ends without writing. It wants every miss, or
\* only the first one when the bug stops there; each want records whether
\* its chunk existed when the attempt read the store.
EndOnMiss(r) ==
  \* The attempt is open, misses end it, and it met at least one.
  /\ pc[r] = "txn"
  /\ ResolveOutside
  /\ Misses(r) # {}
  \* It wants all misses, or just one of them under the bug,
  /\ \E first \in Misses(r) :
       LET asked == IF WantEveryMiss THEN Misses(r) ELSE {first}
       \* each as its key, its text, and whether the chunk was there.
       IN wanted' = [wanted EXCEPT ![r] =
                       {<<Key(n), n[2], <<n[1], n[2]>> \in snap[r]>> : n \in asked}]
  \* The transaction closes unwritten and the request goes to ask the model.
  /\ pc' = [pc EXCEPT ![r] = "wave"]
  /\ UNCHANGED <<text, chunks, vecOf, snap, snapText, memo, waves, callInTxn, reEmbedded>>

\* The bug: the attempt asks the model for its misses on the spot, with its
\* transaction open, and carries on.
CallInAttempt(r) ==
  \* The attempt is open, the bug is on, and it met a miss.
  /\ pc[r] = "txn"
  /\ ~ResolveOutside
  /\ Misses(r) # {}
  \* The model answers every miss into the memo,
  /\ memo' = [memo EXCEPT ![r] = memo[r] \cup {<<Key(n), n[2]>> : n \in Misses(r)}]
  \* while the transaction is still open,
  /\ callInTxn' = TRUE
  \* and some of those may be chunks that already existed.
  /\ reEmbedded' = (reEmbedded \/ \E n \in Misses(r) : <<n[1], n[2]>> \in snap[r])
  \* The attempt stays open and goes on.
  /\ UNCHANGED <<text, chunks, vecOf, pc, snap, snapText, wanted, waves>>

\* Every vector is in the memo and nothing the attempt read has changed: it
\* commits the texts and creates its chunks, each with its memo's vector.
Commit(r) ==
  \* The attempt is open and the memo answers every chunk it creates,
  /\ pc[r] = "txn"
  /\ Misses(r) = {}
  \* and no other commit changed what it read.
  /\ chunks = snap[r] /\ text = snapText[r]
  \* The request's texts are written,
  /\ text' = [a \in Articles |-> IF a \in DOMAIN Job[r] THEN Job[r][a] ELSE text[a]]
  \* its new chunks are created,
  /\ chunks' = chunks \cup {<<n[1], n[2]>> : n \in ToCreate(r)}
  \* each with the vector the memo holds for it; other chunks keep theirs.
  /\ vecOf' = [c \in Chunks |->
                 IF \E n \in ToCreate(r) : <<n[1], n[2]>> = c
                 THEN Answer(r, CHOOSE n \in ToCreate(r) : <<n[1], n[2]>> = c)
                 ELSE vecOf[c]]
  \* The request is finished.
  /\ pc' = [pc EXCEPT ![r] = "done"]
  /\ UNCHANGED <<snap, snapText, memo, wanted, waves, callInTxn, reEmbedded>>

\* Another request committed since the attempt read the store: regolith
\* aborts it, and it runs again.
Abort(r) ==
  \* The attempt is open and has its vectors,
  /\ pc[r] = "txn"
  /\ Misses(r) = {}
  \* but what it read changed under it.
  /\ ~(chunks = snap[r] /\ text = snapText[r])
  \* It closes unwritten, to run again; the memo stays.
  /\ pc' = [pc EXCEPT ![r] = "idle"]
  /\ UNCHANGED <<text, chunks, vecOf, snap, snapText, memo, wanted, waves, callInTxn, reEmbedded>>

\* With no transaction open, the request asks the model for everything it
\* wants in one call, keeps the answers, and runs the attempt again.
Wave(r) ==
  \* The request left its attempt to ask the model; the bound keeps the model small.
  /\ pc[r] = "wave"
  /\ waves[r] < 4
  \* The model's answers go into the memo,
  /\ memo' = [memo EXCEPT ![r] = memo[r] \cup {<<w[1], w[2]>> : w \in wanted[r]}]
  \* and asking for a chunk that already existed is noted.
  /\ reEmbedded' = (reEmbedded \/ \E w \in wanted[r] : w[3])
  \* Nothing is wanted any more, one more wave was spent,
  /\ wanted' = [wanted EXCEPT ![r] = {}]
  /\ waves' = [waves EXCEPT ![r] = waves[r] + 1]
  \* and the attempt runs again.
  /\ pc' = [pc EXCEPT ![r] = "idle"]
  /\ UNCHANGED <<text, chunks, vecOf, snap, snapText, callInTxn>>

\* Every step some request can take.
Next ==
  \E r \in Requests :
    \* open an attempt,
    \/ Begin(r)
    \* end it on a miss, or ask the model inside it under the bug,
    \/ EndOnMiss(r)
    \/ CallInAttempt(r)
    \* commit it or lose it to another commit,
    \/ Commit(r)
    \/ Abort(r)
    \* or ask the model between attempts.
    \/ Wave(r)

\* Start from Init and take any steps.
Spec == Init /\ [][Next]_vars

\* The model is never asked while a transaction is open.
INV_NoModelCallInTxn == ~callInTxn

\* Every chunk an article's text names exists and holds a vector.
INV_ListedChunkHasVector ==
  \* for each article with a text, each of its two chunks
  \A a \in Articles : text[a] # None =>
    \A i \in 1..2 : <<a, Split[text[a]][i]>> \in chunks /\ vecOf[<<a, Split[text[a]][i]>>] # None

\* Every chunk holds the vector of its own text.
INV_VectorIsItsText == \A c \in chunks : vecOf[c] = c[2]

\* No write asks the model for a chunk that existed when its attempt read the store.
INV_UnchangedChunkNoCall == ~reEmbedded

\* Each request's misses are answered in at most one wave.
INV_OneWave == \A r \in Requests : waves[r] <= 1
====
