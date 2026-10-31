---- MODULE ReplicationFilter ----
\* The story: an embedder gives the node a replication filter. Each
\* replicated commit is merged in two phases: phase 1 reads and decodes the
\* field blocks it links, phase 2 writes the field values and the document.
\* Between them the filter is asked, given the values phase 1 read, whether
\* to keep the document. A refused document leaves nothing of itself behind,
\* and the refusal is final: its head is skipped for good.
\*
\* Who acts: the replication path, which merges events one at a time or in
\* batches of two (each event of a batch gets its own outcome).
\*
\* What must never happen:
\*   - a document the filter refuses has a field value or a document stored;
\*   - an event the filter keeps is not merged because another event of its
\*     batch was refused;
\*   - a refused event is merged later, or merged again by a retry;
\*   - a document of a branchable collection is filtered one by one.
\*
\* Code it models (crates/db/src/merge/merge_handler):
\*   composite_fields.rs process_linked_field_blocks (phase 1 collects the
\*   values with crate::replication_filter::add_field_values, the filter is
\*   asked before phase 2 persists, a refusal is a terminal skip),
\*   batch.rs try_batch_merge (a skip is that event's outcome; the others go on).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Branchable,  \* TRUE: the collection is @branchable
  AskMode,     \* "BeforePersist": the filter is asked between the phases | "AfterPersist": after phase 2 wrote the fields
  BatchMode,   \* "PerEvent": a refusal skips its own event | "Whole": the whole batch
  FinalMode,   \* "Terminal": a refused head is skipped for good | "Retry": it goes back to pending
  ScopeMode    \* "SkipBranchable": a branchable collection is not filtered | "All": it is

\* Each switch takes one of its values.
ASSUME Branchable \in BOOLEAN
ASSUME AskMode \in {"BeforePersist", "AfterPersist"}
ASSUME BatchMode \in {"PerEvent", "Whole"}
ASSUME FinalMode \in {"Terminal", "Retry"}
ASSUME ScopeMode \in {"SkipBranchable", "All"}

\* The replicated events, one per document: the filter keeps k1 and k2 and
\* refuses r.
Events == {"k1", "k2", "r"}
\* What the filter answers for each event, given its fields.
Allows(e) == e # "r"
\* Whether the filter is asked for an event at all.
Asked == ~(Branchable /\ ScopeMode = "SkipBranchable")
\* Whether an event is refused.
Refused(e) == Asked /\ ~Allows(e)

VARIABLES
  outcome,   \* each event's outcome: "pending", "merged" or "skipped"
  fields,    \* the documents whose field values are stored
  docs,      \* the documents stored
  merges     \* how many times each event was merged

\* Every variable, for UNCHANGED.
vars == <<outcome, fields, docs, merges>>

\* The start: nothing has arrived.
Init ==
  /\ outcome = [e \in Events |-> "pending"]
  /\ fields = {}
  /\ docs = {}
  /\ merges = [e \in Events |-> 0]

\* A batch of pending events is merged. Each refused event is skipped; with
\* the Whole switch a refusal skips the batch. With AfterPersist the refused
\* event's phase 2 already wrote its fields before the filter said no.
Merge(batch) ==
  LET refused == {e \in batch : Refused(e)}
      kept == IF BatchMode = "Whole" /\ refused # {} THEN {} ELSE batch \ refused
  IN
  /\ batch # {}
  /\ \A e \in batch : outcome[e] = "pending"
  /\ outcome' = [e \in Events |->
                   IF e \in kept THEN "merged"
                   ELSE IF e \in batch
                     THEN (IF FinalMode = "Retry" THEN "pending" ELSE "skipped")
                   ELSE outcome[e]]
  /\ fields' = fields \cup kept \cup (IF AskMode = "AfterPersist" THEN refused ELSE {})
  /\ docs' = docs \cup kept
  /\ merges' = [e \in Events |-> IF e \in kept THEN merges[e] + 1 ELSE merges[e]]

\* A retried refused event is asked again and the filter changed its mind:
\* only a Retry switch lets it come back as pending, so this is the case
\* that switch opens.
FilterChanged ==
  /\ FinalMode = "Retry"
  /\ \E e \in Events : outcome[e] = "pending" /\ Refused(e) /\ merges[e] = 0
       /\ outcome' = [outcome EXCEPT ![e] = "merged"]
       /\ fields' = fields \cup {e}
       /\ docs' = docs \cup {e}
       /\ merges' = [merges EXCEPT ![e] = 1]

\* Any batch of one or two events, or the filter changing its answer.
Next ==
  \/ \E e \in Events : Merge({e})
  \/ \E e, f \in Events : e # f /\ Merge({e, f})
  \/ FilterChanged

\* The run.
Spec == Init /\ [][Next]_vars

\* Invariant: a refused document has neither field values nor a document.
INV_RefusedNothingStored == \A e \in Events : Refused(e) => e \notin fields /\ e \notin docs

\* Invariant: an event the filter keeps is never skipped.
INV_KeptMerged == \A e \in Events : ~Refused(e) => outcome[e] # "skipped"

\* Invariant: no event is merged more than once.
INV_MergedOnce == \A e \in Events : merges[e] <= 1

\* Invariant: in a branchable collection, every event that ran merged.
INV_BranchableUnfiltered == Branchable => \A e \in Events : outcome[e] # "skipped"
====
