---- MODULE SharedQuantizer ----
\* The story, in plain words.
\*
\* A trained DiskANN index encodes every new vector with its product
\* quantizer: one codebook per subspace, written once by the training and
\* never changed after it. Rebuilding the quantizer from those codebooks on
\* every write transaction costs a read and a decode of every codebook, so
\* each node keeps one quantizer per index in memory and every transaction
\* shares it.
\*
\* An index can be trained again (a rebuild), and that writes new codebooks.
\* So the shared quantizer is tagged with the training whose codebooks it was
\* built from. A training draws its tag at random and writes it in the
\* trained record, in the same transaction as its codebooks.
\*
\* A writer reads the trained record in its snapshot, then:
\*   1. the shared quantizer carries the same training tag: it uses it;
\*   2. otherwise: it reads its snapshot's codebooks, builds a quantizer from
\*      them, and shares it under its snapshot's tag.
\* Whichever share lands last stays.
\*
\* A training runs in a transaction, and a transaction can be retried: each
\* attempt draws its tag again, with its codebooks. Before it commits,
\* another reader inside the same transaction can see the attempt's own
\* trained record and share a quantizer built from its uncommitted codebooks.
\* That attempt may then abort.
\*
\* A tiny example. Training 1 commits tag t1 with codebooks A. A writer
\* shares (t1, A). A rebuild commits tag t2 with codebooks B. A writer whose
\* snapshot holds t2 finds (t1, A), sees the tags differ, and builds from B.
\*
\* What must never happen:
\*   1. a writer encodes with codebooks other than its own snapshot's -
\*      INV_EncodesWithItsCodebooks;
\*   2. the shared quantizer pairs a tag with codebooks no attempt drew under
\*      that tag - INV_SharedIsConsistent.
\*
\* Each guard has a RED configuration that removes it and fails:
\*   MC_SharedQuantizer_Red_NoTagCheck: a writer uses the shared quantizer
\*     whatever its tag, so after a rebuild it encodes with the old codebooks;
\*   MC_SharedQuantizer_Red_TagPerRequest: a training draws its tag once for
\*     the whole request instead of once per attempt, so an aborted attempt's
\*     shared quantizer carries the tag the retry commits with other codebooks.
\*
\* Correspondence with the Rust code (crates/db/src/index/vector/engine/
\* diskann/):
\*   committed      the trained record and codebooks a snapshot reads: codec.rs
\*                  Trained (its `training` field), encode_trained,
\*                  decode_trained
\*   Train*         build.rs train_and_build, which draws `training` with
\*                  rand::random() in the attempt that writes the codebooks
\*   shared         cache.rs CodeCache::quantizer, CodeCache::share_quantizer
\*   WriterGet      mod.rs DiskAnn::quantizer
\*   PublishOwn     a second DiskAnn in the training's own transaction reading
\*                  its uncommitted trained record through DiskAnn::quantizer
\* Rust wiring: index.rs VectorIndex::share_quantizer, manager/mod.rs
\* IndexManager::with_vector_caches, write/autocommit create.rs, update.rs,
\* delete.rs.
\*
\* Codebooks are abstracted to a number per attempt: two attempts never draw
\* the same codebooks here, which is the worst case, since equal codebooks
\* under two tags would only make a stale match harmless.

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
    Writers,          \* the transactions that insert and need the quantizer
    MaxAttempts,      \* how many training attempts the run may start
    TagCheck,         \* guard: a writer compares the shared tag with its own
    TagPerAttempt     \* guard: each training attempt draws a fresh tag

\* None stands for "nothing": no committed training, no shared quantizer.
None == [tag |-> 0, books |-> 0]

VARIABLES
    committed,   \* the trained record and codebooks the store holds now
    attempt,     \* the training attempt in flight, or None
    attempts,    \* how many attempts have started; each draws new codebooks
    requestTag,  \* the tag a request draws once, used when TagPerAttempt is off
    drawn,       \* every (tag, codebooks) pair some attempt has written
    shared,      \* the node's shared quantizer: a tag and its codebooks
    snap,        \* each writer's snapshot of the trained record, or None
    used         \* the codebooks each writer encoded with, or 0 if not yet

vars == <<committed, attempt, attempts, requestTag, drawn, shared, snap, used>>

\* A tag is a number; a fresh one is never drawn twice. Attempt k draws k,
\* the request tag is what the request's first attempt drew.
Init ==
    /\ committed = None
    /\ attempt = None
    /\ attempts = 0
    /\ requestTag = 0
    /\ drawn = {}
    /\ shared = None
    /\ snap = [w \in Writers |-> None]
    /\ used = [w \in Writers |-> 0]

\* A training attempt starts: it writes new codebooks and a tag in its own
\* transaction, which nothing outside it sees until it commits.
TrainBegin ==
    /\ attempt = None
    /\ attempts < MaxAttempts
    /\ LET books == attempts + 1
           \* with the guard a tag per attempt; without it, the request's tag
           tag == IF TagPerAttempt \/ requestTag = 0 THEN attempts + 1 ELSE requestTag
       IN /\ attempt' = [tag |-> tag, books |-> books]
          /\ drawn' = drawn \cup {[tag |-> tag, books |-> books]}
          /\ requestTag' = IF requestTag = 0 THEN tag ELSE requestTag
    /\ attempts' = attempts + 1
    /\ UNCHANGED <<committed, shared, snap, used>>

\* A reader inside the training's own transaction sees its uncommitted
\* trained record and shares a quantizer built from its codebooks.
PublishOwn ==
    /\ attempt # None
    /\ shared' = attempt
    /\ UNCHANGED <<committed, attempt, attempts, requestTag, drawn, snap, used>>

\* The attempt commits: its record and codebooks become what snapshots read,
\* and the request is done, so the next training is a new request.
TrainCommit ==
    /\ attempt # None
    /\ committed' = attempt
    /\ attempt' = None
    /\ requestTag' = 0
    /\ UNCHANGED <<attempts, drawn, shared, snap, used>>

\* The attempt aborts (a conflict), and the request retries with a new
\* attempt, which keeps the request's tag when TagPerAttempt is off.
TrainAbort ==
    /\ attempt # None
    /\ attempt' = None
    /\ UNCHANGED <<committed, attempts, requestTag, drawn, shared, snap, used>>

\* A writer begins: its snapshot holds whatever training has committed.
WriterBegin(w) ==
    /\ snap[w] = None
    /\ committed # None
    /\ snap' = [snap EXCEPT ![w] = committed]
    /\ UNCHANGED <<committed, attempt, attempts, requestTag, drawn, shared, used>>

\* A writer gets its quantizer: the shared one on a tag match (or always,
\* without the guard), else one it builds from its snapshot and shares.
WriterGet(w) ==
    /\ snap[w] # None
    /\ used[w] = 0
    /\ IF shared # None /\ (~TagCheck \/ shared.tag = snap[w].tag)
         THEN /\ used' = [used EXCEPT ![w] = shared.books]
              /\ UNCHANGED shared
         ELSE /\ used' = [used EXCEPT ![w] = snap[w].books]
              /\ shared' = snap[w]
    /\ UNCHANGED <<committed, attempt, attempts, requestTag, drawn, snap>>

\* A writer that has encoded finishes, and its transaction can start again.
WriterEnd(w) ==
    /\ used[w] # 0
    /\ snap' = [snap EXCEPT ![w] = None]
    /\ used' = [used EXCEPT ![w] = 0]
    /\ UNCHANGED <<committed, attempt, attempts, requestTag, drawn, shared>>

Next ==
    \/ TrainBegin \/ PublishOwn \/ TrainCommit \/ TrainAbort
    \/ \E w \in Writers : WriterBegin(w) \/ WriterGet(w) \/ WriterEnd(w)

Spec == Init /\ [][Next]_vars

\* Every writer that has encoded used exactly its snapshot's codebooks.
INV_EncodesWithItsCodebooks ==
    \A w \in Writers : used[w] # 0 => used[w] = snap[w].books

\* The shared quantizer's codebooks are the only ones ever drawn under its
\* tag, so a tag match means the same codebooks.
INV_SharedIsConsistent ==
    shared # None =>
        \A d \in drawn : d.tag = shared.tag => d.books = shared.books

Symmetry == Permutations(Writers)
====
