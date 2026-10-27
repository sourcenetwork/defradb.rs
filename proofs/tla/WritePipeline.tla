---- MODULE WritePipeline ----
\* The story: a write turns a document into blocks. A create gives every
\* field a block; an update gives every changed field a block on top of its
\* current head. A composite block then links them all; for a create, the
\* document's ID is derived from its CID. A write runs as attempts. An attempt can
\* end early: a field may need a remote input first (an encryption key from
\* the KMS), or the commit may lose a conflict and run again.
\*
\* The create and update pipelines compute every block first and store them
\* only once the composite exists. A missing input is noted and the pass goes
\* on through the other fields; once every field is made or noted, the attempt
\* ends, so one wave asks for every missing input. The inputs are
\* fetched after the attempt is over and kept in a memo; the next attempt
\* reads it from the memo. The document keeps one storage identity (its short
\* ID) for every attempt, so the memo, which is keyed by that identity, still
\* answers on the next one. A create draws the short ID; an update finds the
\* document's own, so for an update IdMode is "Once" by construction.
\*
\* Who acts: the write (starting attempts, computing blocks, storing them,
\* committing), the storage engine (failing a commit on a conflict), the
\* input resolver (fetching what an attempt asked for, between attempts).
\*
\* What must never happen: a block stored before the composite is computed;
\* a key minted while an attempt is running; one field's key minted twice; a
\* commit that holds only part of the write; an attempt that ends for inputs
\* while it still has a missing key it has not asked for.
\*
\* Outside the model: a batch over a transaction the caller lent it has no
\* memo and is never retried by the engine; it mints its keys directly
\* (BatchMutator::create_as and update_as, the branch without a memo). Nor are
\* the heads an update builds on: a head set is a blind merge, so two updates
\* on the same heads both commit and both become heads.
\*
\* Code it models (crates/db/src/write):
\*   create.rs              embed, identity, seed_identity, memo_identity (the memo ordinal keeps the short ID), prepare_keys, compute, compute_many, persist
\*   autocommit/create.rs   create_many_with_resolved_inputs (seed once, resolve waves), create_impl, run_create_attempt
\*   autocommit/batch.rs    BatchMutator::create_as (with a memo: memo-ordinal short ID, keys asked of the memo)
\*   doc.rs                 DbDocMutator::create_as, next_create_identity
\*   update.rs              write_local, write_blocks
\*   autocommit/update.rs   run_update_attempt
\*   autocommit/batch.rs    BatchMutator::update_as (with a memo: keys asked of the memo)
\*   doc.rs                 DbDocMutator::update_as
\*   ../block/builder/compute.rs  compute_document_blocks_with_inputs (requires_kms_keys asks the memo)
\*   ../block/builder/write.rs    build_document_blocks (builds an update, stores nothing; encrypt_with_new_key asks with get_or_want and the call sites note the miss and go on)
EXTENDS Naturals, FiniteSets

CONSTANTS
  Fields,       \* the document's fields, each needing a KMS key
  MaxAttempts,  \* how many attempts the model lets a write start
  StoreMode,    \* "AfterCompute" (store once the composite exists) | "AsComputed" (store each block as made)
  IdMode,       \* "Once" (one short ID for every attempt) | "PerAttempt" (a fresh one each attempt)
  KeySource,    \* "Memo" (a missing key is asked of the memo) | "Direct" (minted on the spot)
  WaveMode      \* "All" (the pass notes every miss, then ends) | "First" (the first miss ends it)

\* There is at least one field, and room for a rerun.
ASSUME Fields # {} /\ MaxAttempts \in Nat /\ MaxAttempts >= 2
ASSUME StoreMode \in {"AfterCompute", "AsComputed"}
ASSUME IdMode \in {"Once", "PerAttempt"}
ASSUME KeySource \in {"Memo", "Direct"}
ASSUME WaveMode \in {"All", "First"}

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
  wanted,     \* keys an attempt asked for, as <<id, field>>
  memo,       \* keys fetched and kept, as <<id, field>>
  mints,      \* ghost: mints[f] counts the keys minted for field f
  minted_in,  \* ghost: a key was minted while an attempt was running
  durable,    \* blocks the committed write holds
  done        \* the write has committed

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

\* Field f's key is missing and KeySource is "Memo": the pass notes the key as
\* wanted (get_or_want). With WaveMode "All" the pass goes on to the other
\* fields; with "First" the attempt ends right here, throwing its transaction
\* away with whatever it staged.
NeedsInput(f) ==
  /\ running /\ f \in Fields \ computed
  /\ <<id, f>> \notin memo \cup wanted
  /\ KeySource = "Memo"
  /\ wanted' = wanted \cup {<<id, f>>}
  /\ IF WaveMode = "First"
       THEN /\ running' = FALSE
            /\ computed' = {}
            /\ staged' = {}
       ELSE UNCHANGED <<running, computed, staged>>
  /\ UNCHANGED <<attempts, id, memo, mints, minted_in, durable, done>>

\* Every field is made or noted, and some key is wanted: the attempt ends
\* (first_miss is returned as NeedsInput) and its transaction is thrown away.
EndForInputs ==
  /\ running /\ wanted # {}
  /\ \A f \in Fields : f \in computed \/ <<id, f>> \in wanted
  /\ running' = FALSE
  /\ computed' = {}
  /\ staged' = {}
  /\ UNCHANGED <<attempts, id, wanted, memo, mints, minted_in, durable, done>>

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
\* away and the write runs again. The memo survives. A conflict is found at
\* commit, so it only strikes once everything is staged.
Conflict ==
  /\ running /\ staged = Blocks
  /\ running' = FALSE
  /\ computed' = {}
  /\ staged' = {}
  /\ UNCHANGED <<attempts, id, wanted, memo, mints, minted_in, durable, done>>

Next ==
  \/ Start
  \/ \E f \in Fields : ComputeKnown(f) \/ ComputeDirect(f) \/ NeedsInput(f)
  \/ EndForInputs
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

\* A committed write holds every block it adds.
INV_CommittedWhole == done => durable = Blocks

\* An attempt that ended asking for keys asked for every key it was missing,
\* so one wave of fetches is enough.
INV_OneWave ==
  (~running /\ wanted # {}) => \A f \in Fields : <<id, f>> \in memo \cup wanted

====
