---- MODULE MergedFieldIndex ----
\* The story: a document has a field whose value is merged rather than
\* written (text, JSON, a counter or a custom type), and an index on it. The
\* node changes the field in two ways: a local write, and a merge of a peer's
\* block. A merge is one transaction in two steps. First each field block is
\* merged, and for these types that step writes the merged value straight
\* into the stored document. Then the composite step updates the index: it
\* removes the entry of the document's old value and adds the new one. One
\* merge transaction can hold several composites (a batch), each doing both
\* steps in turn, which the model sees as merges back to back.
\*
\* Who acts: local writes and merges. A transaction runs whole or not at all,
\* so a local write never lands inside a merge.
\*
\* What must never happen: once no merge is open, the index holds anything
\* but exactly the stored value. A stale entry makes a filter read the
\* document under a value it no longer has, and a missing one hides it.
\*
\* Code it models (crates/db/src/merge/merge_handler):
\*   composite_fields.rs  process_linked_field_blocks (field merges, and the
\*                        document read the index update starts from)
\*   composite_persist.rs persist_merged_document (index update from that
\*                        read to the merged document)
EXTENDS Naturals

CONSTANTS
  ReadMode,   \* "Before": the index update starts from a read taken before the field merges | "AfterPart": from a read after them
  MaxSteps    \* how many writes and composites the check explores

\* Each switch takes one of its values.
ASSUME ReadMode \in {"Before", "AfterPart"}
\* At least one step, or nothing happens.
ASSUME MaxSteps \in Nat \ {0}

VARIABLES
  value,    \* the field's value as stored: a number that grows with each edit
  index,    \* the values the index holds an entry for, for this document
  mpc,      \* where the open composite is: "Idle" (none) or "Index" (its field step ran, its index step is next)
  before,   \* the value the index update will start from, as read by the open composite
  steps     \* writes and composites so far

vars == <<value, index, mpc, before, steps>>

\* Every variable stays in its range.
TypeOK ==
  /\ value \in Nat                               \* a stored value
  /\ index \subseteq Nat                         \* entries are stored values
  /\ mpc \in {"Idle", "Index"}                   \* a known merge step
  /\ before \in Nat                              \* a value that was read
  /\ steps \in 0..MaxSteps                       \* bounded exploration

\* The document exists with value 0 and its index entry, and no merge is open.
Init ==
  /\ value = 0          \* the first value
  /\ index = {0}        \* indexed when it was created
  /\ mpc = "Idle"       \* no merge open
  /\ before = 0         \* nothing read yet
  /\ steps = 0          \* nothing done yet

\* A local write edits the field and moves the index in the same
\* transaction: it reads the value it edits, so it knows the old entry.
LocalWrite ==
  /\ mpc = "Idle"                          \* no merge open: transactions do not interleave
  /\ steps < MaxSteps                      \* still exploring
  /\ value' = value + 1                    \* the edit changes the value
  /\ index' = (index \ {value}) \cup {value + 1}   \* old entry out, new entry in
  /\ steps' = steps + 1                    \* one more step
  /\ UNCHANGED <<mpc, before>>

\* A composite starts its field step. "Before" reads the document now, before
\* any field block of this composite is merged.
MergeFields ==
  /\ mpc = "Idle"                                  \* not in the middle of a composite
  /\ steps < MaxSteps                              \* still exploring
  /\ before' = IF ReadMode = "Before" THEN value ELSE before   \* the early read
  /\ value' = value + 1                            \* the field merge writes the merged value
  /\ mpc' = "Index"                                \* the composite step is next
  /\ steps' = steps + 1                            \* one more step
  /\ UNCHANGED index

\* The composite step updates the index from the old value to the merged one.
\* "AfterPart" reads the old value now, after the field merge wrote it.
MergeIndex ==
  /\ mpc = "Index"                                            \* a field step just ran
  /\ LET old == IF ReadMode = "Before" THEN before ELSE value \* where the update starts
     IN index' = (index \ {old}) \cup {value}                 \* old entry out, merged entry in
  /\ mpc' = "Idle"                                           \* the composite is done
  /\ UNCHANGED <<value, before, steps>>

\* Any one step happens next.
Next == LocalWrite \/ MergeFields \/ MergeIndex

\* Start in Init and take any steps.
Spec == Init /\ [][Next]_vars

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Once no composite is half done, the index holds exactly the stored value.
INV_IndexMatchesValue == mpc = "Idle" => index = {value}
====
