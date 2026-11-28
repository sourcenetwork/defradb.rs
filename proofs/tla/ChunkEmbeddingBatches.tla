---- MODULE ChunkEmbeddingBatches ----
\* An edit needs vectors for several new chunks. The caller asks for a
\* bounded batch at a time per slot, giving each batch a fresh deadline.
\* Some answers can fail. Answers already received belong to the write
\* even if the shared cache holds none of them.
\* Correspondence: crates/database/db/src/write/prepare/embedding_batches.rs resolve_embeddings
\* and crates/building-blocks/inputs/src/service.rs Service::get_many.
EXTENDS Naturals, FiniteSets

\* These switches turn off one rule at a time.
CONSTANTS CapBatch, FreshDeadline, KeepAnswers
\* Three chunks are enough to require a second batch of size two.
Chunks == 1..3
\* A batch can use its entire time budget.
Budget == 2

\* Track waiting chunks, the current batch in one slot, elapsed time,
\* all answers received, and the answers still held by the caller.
VARIABLES waiting, batch, elapsed, left, received, memo
\* The other slots follow the same rules independently.
vars == <<waiting, batch, elapsed, left, received, memo>>

\* At first every chunk is waiting and no batch has started.
Init ==
    /\ waiting = Chunks
    /\ batch = {}
    /\ elapsed = 0
    /\ left = 0
    /\ received = {}
    /\ memo = {}

\* Start the next batch in a free slot.
Start ==
    /\ batch = {}
    /\ \E selected \in SUBSET waiting:
        \* Choose up to two waiting chunks and remove them from the queue.
        /\ selected # {}
        /\ ~CapBatch \/ Cardinality(selected) <= 2
        /\ batch' = selected
        /\ waiting' = waiting \ selected
    \* Give these chunks their own time, even if earlier batches used theirs.
    /\ left' = IF FreshDeadline THEN Budget ELSE Budget - elapsed
    /\ UNCHANGED <<elapsed, received, memo>>

\* A batch uses its remaining time and returns any subset of answers.
Finish ==
    /\ batch # {}
    /\ \E answered \in SUBSET batch:
        \* Some or all answers may be missing. Keep each answer that came.
        /\ received' = received \cup answered
        /\ memo' = IF KeepAnswers \/ answered = batch
                   THEN memo \cup answered ELSE {}
        \* If this batch failed, the slot starts no more batches.
        /\ waiting' = IF answered = batch THEN waiting ELSE {}
    \* Free the slot. Remember that the wave has already spent one budget.
    /\ elapsed' = Budget
    /\ batch' = {}
    /\ left' = 0

\* A step starts or finishes the batch.
Next == Start \/ Finish
\* The caller may stop changing at any point.
Spec == Init /\ [][Next]_vars

\* No batch has more than two chunks.
INV_Bounded == Cardinality(batch) <= 2
\* An active batch always starts with its full time budget.
INV_Fresh == batch # {} => left = Budget
\* Every received answer remains available in the caller's memo.
INV_Retained == received = memo
====
