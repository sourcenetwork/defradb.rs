---- MODULE CreatePipeline ----
\* The story: a create turns one document into blocks. Every field gets a
\* block, then a composite block links them all, and the document's ID is
\* derived from the composite's CID. A create runs as attempts. An attempt can
\* end early: a field may need a remote input first (an encryption key from
\* the KMS), or the commit may lose a conflict and run again.
\*
\* The create pipeline computes every block first and stores them only once
\* the composite exists. A missing input ends the attempt; the input is
\* fetched after the attempt is over and kept in a memo; the next attempt
\* reads it from the memo. The document keeps one storage identity (its short
\* ID) for every attempt, so the memo, which is keyed by that identity, still
\* answers on the next one.
\*
\* Who acts: the create (starting attempts, computing blocks, storing them,
\* committing), the storage engine (failing a commit on a conflict), the
\* input resolver (fetching what an attempt asked for, between attempts).
\*
\* What must never happen: a block stored before the composite is computed;
\* a key minted while an attempt is running; one field's key minted twice; a
\* commit that holds only part of the document.
\*
\* Outside the model: a batch over a transaction the caller lent it has no
\* memo and is never retried by the engine; it mints its keys directly
\* (BatchMutator::create_as, the branch without a memo).
\*
\* Code it models (crates/db/src/write):
\*   create.rs              embed, identity, seed_identity, memo_identity (the memo ordinal keeps the short ID), prepare_keys, compute, compute_many, persist
\*   autocommit/create.rs   create_many_with_resolved_inputs (seed once, resolve waves), create_impl, run_create_attempt
\*   autocommit/batch.rs    BatchMutator::create_as (with a memo: memo-ordinal short ID, keys asked of the memo)
\*   doc.rs                 DbDocMutator::create_as, next_create_identity
\*   ../block/builder/compute.rs  compute_document_blocks_with_inputs (requires_kms_keys asks the memo)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Fields,       \* the document's fields, each needing a KMS key
  MaxAttempts,  \* how many attempts the model lets a create start
  StoreMode,    \* "AfterCompute" (store once the composite exists) | "AsComputed" (store each block as made)
  IdMode,       \* "Once" (one short ID for every attempt) | "PerAttempt" (a fresh one each attempt)
  KeySource     \* "Memo" (a missing key ends the attempt) | "Direct" (minted on the spot)

\* There is at least one field, and room for a rerun.
ASSUME Fields # {} /\ MaxAttempts \in Nat /\ MaxAttempts >= 2
ASSUME StoreMode \in {"AfterCompute", "AsComputed"}
ASSUME IdMode \in {"Once", "PerAttempt"}
ASSUME KeySource \in {"Memo", "Direct"}

\* The composite block, named apart from the fields.
Composite == "C"

\* Every block of the document: one per field and the composite.
Blocks == Fields \cup {Composite}

\* The short IDs an attempt can draw.
Ids == 1..MaxAttempts

VARIABLES
  attempts,   \* attempts started so far
  running,    \* an attempt is running now
  id,         \* the document's short ID (0 before the first draw)
  computed,   \* blocks this attempt has computed, in memory
  staged,     \* blocks this attempt has written into its transaction
  wanted,     \* keys an ended attempt asked for, as <<id, field>>
  memo,       \* keys fetched and kept, as <<id, field>>
  mints,      \* ghost: mints[f] counts the keys minted for field f
  minted_in,  \* ghost: a key was minted while an attempt was running
  durable,    \* blocks the committed create holds
  done        \* the create has committed

vars == <<attempts, running, id, computed, staged, wanted, memo, mints, minted_in, durable, done>>

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ attempts \in 0..MaxAttempts
  /\ running \in BOOLEAN
  /\ id \in 0..MaxAttempts
  /\ computed \subseteq Blocks
  /\ staged \subseteq Blocks
  /\ wanted \subseteq (Ids \X Fields)
  /\ memo \subseteq (Ids \X Fields)
  /\ mints \in [Fields -> Nat]
  /\ minted_in \in BOOLEAN
  /\ durable \subseteq Blocks
  /\ done \in BOOLEAN

\* Nothing has started, nothing is stored, no key exists.
Init ==
  /\ attempts = 0
  /\ running = FALSE
  /\ id = 0
  /\ computed = {}
  /\ staged = {}
  /\ wanted = {}
  /\ memo = {}
  /\ mints = [f \in Fields |-> 0]
  /\ minted_in = FALSE
  /\ durable = {}
  /\ done = FALSE

\* An attempt starts with nothing computed or staged. The first attempt draws
\* the short ID; with IdMode "PerAttempt" every attempt draws a new one.
Start ==
  /\ ~running /\ ~done /\ wanted = {}
  /\ attempts < MaxAttempts
  /\ attempts' = attempts + 1
  /\ running' = TRUE
  /\ id' = IF id = 0 \/ IdMode = "PerAttempt" THEN attempts + 1 ELSE id
  /\ computed' = {}
  /\ staged' = {}
  /\ UNCHANGED <<wanted, memo, mints, minted_in, durable, done>>

\* The block of field f, stored at once when StoreMode is "AsComputed".
Made(f) ==
  /\ computed' = computed \cup {f}
  /\ staged' = IF StoreMode = "AsComputed" THEN staged \cup {f} ELSE staged

\* Field f's key is in the memo under this attempt's ID: its block is made.
ComputeKnown(f) ==
  /\ running /\ f \in Fields \ computed
  /\ <<id, f>> \in memo
  /\ Made(f)
  /\ UNCHANGED <<attempts, running, id, wanted, memo, mints, minted_in, durable, done>>

\* Field f's key is missing and KeySource is "Direct": the key is minted on
\* the spot, while the attempt runs, and the block is made.
ComputeDirect(f) ==
  /\ running /\ f \in Fields \ computed
  /\ <<id, f>> \notin memo
  /\ KeySource = "Direct"
  /\ memo' = memo \cup {<<id, f>>}
  /\ mints' = [mints EXCEPT ![f] = @ + 1]
  /\ minted_in' = TRUE
  /\ Made(f)
  /\ UNCHANGED <<attempts, running, id, wanted, durable, done>>

\* Field f's key is missing and KeySource is "Memo": the attempt ends, asking
\* for the key, and its transaction is thrown away with whatever it staged.
NeedsInput(f) ==
  /\ running /\ f \in Fields \ computed
  /\ <<id, f>> \notin memo
  /\ KeySource = "Memo"
  /\ wanted' = {<<id, f>>}
  /\ running' = FALSE
  /\ computed' = {}
  /\ staged' = {}
  /\ UNCHANGED <<attempts, id, memo, mints, minted_in, durable, done>>

\* Between attempts the resolver mints what was asked for and keeps it.
Resolve ==
  /\ ~running /\ wanted # {}
  /\ memo' = memo \cup wanted
  /\ mints' = [f \in Fields |-> mints[f] + Cardinality({w \in wanted : w[2] = f})]
  /\ wanted' = {}
  /\ UNCHANGED <<attempts, running, id, computed, staged, minted_in, durable, done>>

\* Every field block exists: the composite that links them is made.
ComputeComposite ==
  /\ running /\ Fields \subseteq computed /\ Composite \notin computed
  /\ computed' = computed \cup {Composite}
  /\ staged' = IF StoreMode = "AsComputed" THEN staged \cup {Composite} ELSE staged
  /\ UNCHANGED <<attempts, running, id, wanted, memo, mints, minted_in, durable, done>>

\* persist: once the composite exists, every block goes into the transaction.
Persist ==
  /\ running /\ Composite \in computed /\ staged # Blocks
  /\ staged' = Blocks
  /\ UNCHANGED <<attempts, running, id, computed, wanted, memo, mints, minted_in, durable, done>>

\* The commit makes the staged blocks durable.
Commit ==
  /\ running /\ staged = Blocks
  /\ durable' = staged
  /\ done' = TRUE
  /\ running' = FALSE
  /\ UNCHANGED <<attempts, id, computed, staged, wanted, memo, mints, minted_in>>

\* The commit loses a conflict: the engine throws the attempt's transaction
\* away and the create runs again. The memo survives.
Conflict ==
  /\ running
  /\ running' = FALSE
  /\ computed' = {}
  /\ staged' = {}
  /\ UNCHANGED <<attempts, id, wanted, memo, mints, minted_in, durable, done>>

Next ==
  \/ Start
  \/ \E f \in Fields : ComputeKnown(f) \/ ComputeDirect(f) \/ NeedsInput(f)
  \/ Resolve
  \/ ComputeComposite
  \/ Persist
  \/ Commit
  \/ Conflict

Spec == Init /\ [][Next]_vars

\* Nothing is written into the transaction before the composite is computed.
INV_NoStoreBeforeComposite == staged # {} => Composite \in computed

\* No key is ever minted while an attempt is running.
INV_NoMintInAttempt == ~minted_in

\* Each field's key is minted at most once, however many attempts run.
INV_OneKeyPerField == \A f \in Fields : mints[f] <= 1

\* A committed create holds every block of its document.
INV_CommittedWhole == done => durable = Blocks

====
