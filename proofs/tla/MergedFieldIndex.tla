---- MODULE MergedFieldIndex ----
\* The story: a document has a field whose value is merged rather than
\* written (text, JSON, a counter or a custom type), and an index on it. The
\* node changes the field in two ways: a local write, and a merge of a peer's
\* blocks. A merge is one transaction in steps. First each field block is
\* merged, and for these types that step writes the merged value straight
\* into the stored document. Then the composite step updates the index: it
\* removes the entry of the document's old value and adds the new one. A
\* peer's composite can link two blocks of the same field, and one merge
\* transaction can hold several composites (a batch), which the model sees
\* as composites back to back. A parked block replayed later is a composite
\* with one block.
\*
\* The merge keeps one read of the document and shares it between its steps,
\* so it reads the document once. When a field block writes the stored value,
\* that kept read has to follow, or the next step works from a value that is
\* no longer stored.
\*
\* Who acts: local writes and merges. A transaction runs whole or not at all,
\* so a local write never lands inside a merge.
\*
\* What must never happen: once no merge is open, the index holds anything
\* but exactly the stored value, or an edit is lost. A stale entry makes a
\* filter read the document under a value it no longer has, a missing one
\* hides it, and a lost edit means a peer's change never counted.
\*
\* Code it models (crates/db/src/merge/merge_handler):
\*   composite_fields.rs  process_linked_field_blocks (field merges, and the
\*                        document read the index update starts from)
\*   composite_persist.rs persist_merged_document (index update from that
\*                        read to the merged document)
\*   block_reads.rs       BlockReads::document and wrote_field (the kept read)
\*   parked.rs            replay_in_txn (a parked block merged later)
EXTENDS Naturals

CONSTANTS
  ReadMode,   \* "Before": the index update starts from a read taken before the field merges | "AfterPart": from a read after them
  CacheMode,  \* "Update": a field write puts its value in the kept read | "Clear": it drops the kept read | "Keep": it leaves the kept read as it was
  MaxSteps    \* how many writes and field blocks the check explores

\* Each switch takes one of its values.
ASSUME ReadMode \in {"Before", "AfterPart"}
\* The three ways a field write can treat the kept read.
ASSUME CacheMode \in {"Update", "Clear", "Keep"}
\* At least one step, or nothing happens.
ASSUME MaxSteps \in Nat \ {0}

VARIABLES
  value,    \* the field's value as stored: a number that grows with each edit
  edits,    \* how many edits were made, so the value they add up to
  index,    \* the values the index holds an entry for, for this document
  mpc,      \* where the composite is: "Idle" (none open) or "Fields" (merging its field blocks)
  blocks,   \* how many field blocks the open composite has merged
  before,   \* the value the index update will start from, as read by the open composite
  cached,   \* the merge's kept read of the value: {v} when it keeps v, {} when it keeps nothing
  steps     \* writes and field blocks so far

vars == <<value, edits, index, mpc, blocks, before, cached, steps>>

\* Every variable stays in its range.
TypeOK ==
  /\ value \in Nat                               \* a stored value
  /\ edits \in Nat                               \* a count
  /\ index \subseteq Nat                         \* entries are stored values
  /\ mpc \in {"Idle", "Fields"}                  \* a known merge step
  /\ blocks \in 0..2                             \* at most two blocks per composite
  /\ before \in Nat                              \* a value that was read
  /\ cached \in SUBSET Nat                       \* a kept read or none
  /\ \A a, b \in cached : a = b                  \* at most one kept value
  /\ steps \in 0..MaxSteps                       \* bounded exploration

\* The document exists with value 0 and its index entry, and no merge is open.
Init ==
  /\ value = 0          \* the first value
  /\ edits = 0          \* no edit yet
  /\ index = {0}        \* indexed when it was created
  /\ mpc = "Idle"       \* no merge open
  /\ blocks = 0         \* no block merged
  /\ before = 0         \* nothing read yet
  /\ cached = {}        \* nothing kept
  /\ steps = 0          \* nothing done yet

\* What the merge sees when it reads the value: the kept read if it has one,
\* the stored value otherwise.
Read == IF cached = {} THEN value ELSE CHOOSE kept \in cached : TRUE

\* A local write edits the field and moves the index in the same
\* transaction: it reads the value it edits, so it knows the old entry.
LocalWrite ==
  /\ mpc = "Idle"                          \* no merge open: transactions do not interleave
  /\ steps < MaxSteps                      \* still exploring
  /\ value' = value + 1                    \* the edit changes the value
  /\ edits' = edits + 1                    \* and counts
  /\ index' = (index \ {value}) \cup {value + 1}   \* old entry out, new entry in
  /\ steps' = steps + 1                    \* one more step
  /\ UNCHANGED <<mpc, blocks, before, cached>>

\* A composite opens. "Before" reads the document now, before any field block
\* is merged, and keeps that read. The kept read of an earlier transaction is
\* gone, since it never outlives one.
MergeStart ==
  /\ mpc = "Idle"                                     \* not in the middle of a composite
  /\ steps < MaxSteps                                 \* room for at least one block
  /\ before' = IF ReadMode = "Before" THEN value ELSE before   \* the early read
  /\ cached' = IF ReadMode = "Before" THEN {value} ELSE {}    \* which the merge keeps
  /\ mpc' = "Fields"                                 \* field blocks are next
  /\ blocks' = 0                                     \* none merged yet
  /\ UNCHANGED <<value, edits, index, steps>>

\* One field block merges: it reads the current value through the kept read,
\* applies its edit to that, and writes the result into the stored document.
\* Then the kept read follows the write, is dropped, or stays as it was.
MergeBlock ==
  /\ mpc = "Fields"                                  \* a composite is open
  /\ blocks < 2                                      \* at most two blocks
  /\ steps < MaxSteps                                \* still exploring
  /\ value' = Read + 1                               \* the merged value, from what was read
  /\ edits' = edits + 1                              \* the block's edit counts
  /\ cached' = CASE CacheMode = "Update" -> {Read + 1} \* the kept read gets the written value
                 [] CacheMode = "Clear"  -> {}         \* the next read goes to the store
                 [] CacheMode = "Keep"   -> {Read}     \* the read stays, now stale
  /\ blocks' = blocks + 1                            \* one more block in this composite
  /\ steps' = steps + 1                              \* one more step
  /\ UNCHANGED <<index, mpc, before>>

\* The composite step updates the index from the old value to the merged one.
\* "AfterPart" takes the old value from the merge's read now, after the
\* field blocks wrote it.
MergeIndex ==
  /\ mpc = "Fields"                                           \* a composite is open
  /\ blocks > 0                                               \* and merged a block
  /\ LET old == IF ReadMode = "Before" THEN before ELSE Read \* where the update starts
     IN index' = (index \ {old}) \cup {value}                 \* old entry out, merged entry in
  /\ mpc' = "Idle"                                           \* the composite is done
  /\ cached' = {}                                            \* the document write drops the kept read
  /\ UNCHANGED <<value, edits, blocks, before, steps>>

\* Any one step happens next.
Next == LocalWrite \/ MergeStart \/ MergeBlock \/ MergeIndex

\* Start in Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Once no composite is half done, the index holds exactly the stored value.
INV_IndexMatchesValue == mpc = "Idle" => index = {value}

\* Every edit, local or merged, shows in the stored value.
INV_NoLostEdit == value = edits
====
