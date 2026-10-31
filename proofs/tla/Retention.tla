---- MODULE Retention ----
\* The story: a node keeps only recent data. Its retention rule names, for a
\* collection, a height field (a block number) and a floor. A document that
\* arrives from a peer is refused when, as merged, its height is at or below
\* the floor or it has no valid height (missing, negative, not an integer).
\* A floor of zero or less refuses nothing by height. Documents written on
\* this node are never judged, deleted documents are never judged, and a
\* branchable collection is never judged, since one of its heads commits many
\* documents at once.
\*
\* Who acts: the replication path, which merges peer events one at a time
\* or in batches (each event in a batch gets its own outcome), and a local
\* writer, which creates a document at height 0.
\*
\* What must never happen:
\*   - a replicated document that is stored and alive sits at or below a
\*     positive floor, or has no valid height;
\*   - a refused event is marked merged, stored or relayed;
\*   - a delete is refused;
\*   - an event the rule accepts is refused because another event in its
\*     batch was;
\*   - with a floor of zero or less, a document is refused for its height;
\*   - a local write, or a document of a branchable collection, is refused;
\*   - a refusal is not counted, or is counted as a drop.
\*
\* Code it models (crates/db/src/merge/merge_handler):
\*   composite_persist.rs persist_merged_document (the check on the merged,
\*   alive document), composite.rs and batch.rs (a refusal is the event's
\*   Rejected outcome; its attempt is discarded alone), and
\*   crates/db/src/retention.rs RetentionRule, RetentionFloor::refusal.
EXTENDS Integers, FiniteSets

CONSTANTS
  Floor,        \* the floor the rule names for the collection
  Branchable,   \* TRUE: the collection is @branchable
  JudgeMode,    \* "Merged": the merge judges the merged document | "Never": nothing is judged
  DeleteMode,   \* "Skip": a deleted document is not judged | "Judge": it is
  BatchMode,    \* "PerEvent": a refusal refuses its own event | "Whole": the whole batch
  FloorMode,    \* "Positive": a floor of zero or less refuses nothing by height | "Strict": it does
  RelayMode,    \* "Committed": only committed events are relayed | "All": every event is
  LocalMode     \* "Exempt": local writes are not judged | "Judged": they are

\* Each switch takes one of its values.
ASSUME Floor \in Int
ASSUME Branchable \in BOOLEAN
ASSUME JudgeMode \in {"Merged", "Never"}
ASSUME DeleteMode \in {"Skip", "Judge"}
ASSUME BatchMode \in {"PerEvent", "Whole"}
ASSUME FloorMode \in {"Positive", "Strict"}
ASSUME RelayMode \in {"Committed", "All"}
ASSUME LocalMode \in {"Exempt", "Judged"}

\* An unset height field. Any negative number has no valid height, so the
\* model writes an unset field as -1; Go treats both the same way.
None == -1

\* The replicated events: each names its document, what it does and the
\* height the document has once it is merged.
Events == {"c1", "c2", "c3", "u1", "d1"}
\* The document each event is for.
Doc(e) == CASE e = "c1" -> "x" [] e = "c2" -> "y" [] e = "c3" -> "z" [] e = "u1" -> "x" [] e = "d1" -> "x"
\* What each event does: create, update or delete.
Kind(e) == CASE e \in {"c1", "c2", "c3"} -> "create" [] e = "u1" -> "update" [] e = "d1" -> "delete"
\* The height each event leaves on its document: x at 2 then 0, y at 1, z
\* with none, and x at 0 when the delete arrives.
Height(e) == CASE e = "c1" -> 2 [] e = "c2" -> 1 [] e = "c3" -> None [] e = "u1" -> 0 [] e = "d1" -> 0
\* A height is valid when it is a non-negative integer.
Valid(h) == h >= 0

\* The refusal for a merged document at height h, or "none" when it is
\* kept; Go's CheckRetention.
Refusal(h) ==
  IF ~Valid(h) THEN "noHeight"
  ELSE IF (FloorMode = "Strict" \/ Floor > 0) /\ h <= Floor THEN "belowFloor"
  ELSE "kept"

\* Whether the rule judges an event: the collection is covered and not
\* branchable, the merge judges, and the merged document is alive.
Judged(e) ==
  /\ JudgeMode = "Merged"
  /\ ~Branchable
  /\ (Kind(e) # "delete" \/ DeleteMode = "Judge")

\* The refusal an event gets on its own.
Verdict(e) == IF Judged(e) THEN Refusal(Height(e)) ELSE "kept"

VARIABLES
  outcome,   \* each event's outcome: "pending", "committed" or "rejected"
  stored,    \* documents stored and alive, with the height they carry, as [doc |-> height]
  deleted,   \* documents a merged delete tombstoned
  merged,    \* events whose heads are marked merged
  relayed,   \* events relayed to peers
  local,     \* the documents written on this node
  refused    \* the refusal counters, by reason

\* Every variable, for UNCHANGED.
vars == <<outcome, stored, deleted, merged, relayed, local, refused>>

\* The start: nothing has arrived, nothing is stored.
Init ==
  /\ outcome = [e \in Events |-> "pending"]
  /\ stored = [d \in {} |-> 0]
  /\ deleted = {}
  /\ merged = {}
  /\ relayed = {}
  /\ local = {}
  /\ refused = [belowFloor |-> 0, noHeight |-> 0]

\* An event can be merged once the document it changes is held: a create
\* first, an update or a delete after its document's create committed.
Ready(e) ==
  /\ outcome[e] = "pending"
  /\ (Kind(e) = "create" \/ Doc(e) \in DOMAIN stored)

\* The documents stored after applying the accepted events of a batch.
Apply(ok) ==
  [d \in (DOMAIN stored \cup {Doc(e) : e \in {f \in ok : Kind(f) # "delete"}}) \ {Doc(e) : e \in {f \in ok : Kind(f) = "delete"}} |->
     IF \E e \in ok : Doc(e) = d /\ Kind(e) # "delete"
       THEN Height(CHOOSE e \in ok : Doc(e) = d /\ Kind(e) # "delete")
       ELSE stored[d]]

\* A batch of ready events, at most one per document, is merged. Each gets
\* its own verdict; with the Whole switch one refusal refuses all of them.
Merge(batch) ==
  LET refusedHere == {e \in batch : Verdict(e) # "kept"}
      ok == IF BatchMode = "Whole" /\ refusedHere # {} THEN {} ELSE batch \ refusedHere
      rejected == batch \ ok
  IN
  /\ batch # {}
  /\ \A e \in batch : Ready(e)
  /\ \A e, f \in batch : Doc(e) = Doc(f) => e = f
  /\ outcome' = [e \in Events |-> IF e \in ok THEN "committed" ELSE IF e \in rejected THEN "rejected" ELSE outcome[e]]
  /\ stored' = Apply(ok)
  /\ deleted' = deleted \cup {Doc(e) : e \in {f \in ok : Kind(f) = "delete"}}
  /\ merged' = merged \cup ok
  /\ relayed' = relayed \cup (IF RelayMode = "All" THEN batch ELSE ok)
  /\ refused' = [belowFloor |-> refused.belowFloor + Cardinality({e \in rejected : Verdict(e) = "belowFloor"}),
                 noHeight |-> refused.noHeight + Cardinality({e \in rejected : Verdict(e) = "noHeight"})]
  /\ UNCHANGED local

\* The node writes document w at height 0 itself. With the Judged switch a
\* local write goes through the rule too.
LocalWrite ==
  /\ "w" \notin local
  /\ local' = local \cup {"w"}
  /\ stored' = IF LocalMode = "Judged" /\ Refusal(0) # "kept" /\ ~Branchable
                 THEN stored
                 ELSE [d \in DOMAIN stored \cup {"w"} |-> IF d = "w" THEN 0 ELSE stored[d]]
  /\ UNCHANGED <<outcome, deleted, merged, relayed, refused>>

\* Any actor may move next: a batch of up to two events, or the local write.
Next ==
  \/ \E e \in Events : Merge({e})
  \/ \E e, f \in Events : Merge({e, f})
  \/ LocalWrite

\* The run.
Spec == Init /\ [][Next]_vars

\* Invariant: every alive replicated document carries a height the rule keeps.
INV_NoneBelowFloor ==
  \A d \in DOMAIN stored \ local :
    (~Branchable) => Refusal(stored[d]) = "kept"

\* Invariant: a refused event is neither merged nor relayed.
INV_RefusedNotMerged ==
  \A e \in Events : outcome[e] = "rejected" => e \notin merged /\ e \notin relayed

\* Invariant: a delete is never refused.
INV_DeleteApplies == \A e \in Events : Kind(e) = "delete" => outcome[e] # "rejected"

\* Invariant: an event the rule keeps on its own is never refused.
INV_BatchIsolation == \A e \in Events : Verdict(e) = "kept" => outcome[e] # "rejected"

\* Invariant: a floor of zero or less refuses nothing for its height.
INV_ZeroFloor == Floor <= 0 => refused.belowFloor = 0

\* Invariant: a local write is always stored, and nothing in a branchable
\* collection is refused.
INV_LocalAndBranchable ==
  /\ ("w" \in local => "w" \in DOMAIN stored)
  /\ (Branchable => \A e \in Events : outcome[e] # "rejected")

\* Invariant: each refusal is counted once, under its reason.
INV_Counted == refused.belowFloor + refused.noHeight = Cardinality({e \in Events : outcome[e] = "rejected"})
====
