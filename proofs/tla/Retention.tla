---- MODULE Retention ----
\* The story: a node keeps only recent data. Its retention rule names, for a
\* collection, a height field (a block number) and a floor. A peer pushes a
\* document's head; the merge walks the head's history, oldest version first,
\* all in one attempt, and judges the document the walk ends with. That
\* document is refused when its height is at or below the floor or it has no
\* valid height (missing or negative). A floor of zero or less refuses nothing
\* by height. A refusal discards the whole attempt: nothing of the walk is
\* stored or marked merged, so a later head of the same document walks the
\* same versions again. Documents written on this node are never judged,
\* deleted documents are never judged, and a branchable collection is never
\* judged, since one of its heads commits many documents at once.
\*
\* Who acts: the replication path, which merges pushed heads one at a time
\* or in batches (each head in a batch gets its own outcome), and a local
\* writer, which creates a document at height 0.
\*
\* What must never happen:
\*   - a replicated document that is stored and alive sits at or below a
\*     positive floor, or has no valid height;
\*   - a refused head is marked merged, stored or relayed;
\*   - a delete is refused;
\*   - a head whose document the rule keeps is refused, because an older
\*     version of it, or another head in its batch, would have been;
\*   - a local write, or a document of a branchable collection, is refused.
\*
\* Code it models (crates/db/src/merge/merge_handler):
\*   composite_persist.rs persist_merged_document (the check on the merged,
\*   alive document, for the walk's root composite only), composite.rs
\*   process_composite_delta_in_txn (the walk, oldest version first, in the
\*   attempt's one transaction), dispatch.rs and batch.rs (a refusal is the
\*   head's Rejected outcome; its attempt is discarded alone), and
\*   crates/db/src/retention.rs RetentionRule, RetentionFloor::refusal,
\*   crates/db/src/database/mod.rs DbOptions::retention_floor (the branchable
\*   guard).
EXTENDS Integers, FiniteSets

CONSTANTS
  Floor,        \* the floor the rule names for the collection
  Branchable,   \* TRUE: the collection is @branchable
  JudgeMode,    \* "Head": the walk's last version is judged | "Each": every version is | "Never": none is
  DeleteMode,   \* "Skip": a deleted document is not judged | "Judge": it is
  BatchMode,    \* "PerEvent": a refusal refuses its own head | "Whole": the whole batch
  FloorMode,    \* "Positive": a floor of zero or less refuses nothing by height | "Strict": it does
  RelayMode,    \* "Committed": only committed heads are relayed | "All": every head is
  LocalMode,    \* "Exempt": local writes are not judged | "Judged": they are
  BranchMode,   \* "Exempt": a branchable collection is not judged | "Judged": it is
  HeightMode    \* "NonNegative": a negative height is no height | "Any": any integer is a height

\* Each switch takes one of its values.
ASSUME Floor \in Int
ASSUME Branchable \in BOOLEAN
ASSUME JudgeMode \in {"Head", "Each", "Never"}
ASSUME DeleteMode \in {"Skip", "Judge"}
ASSUME BatchMode \in {"PerEvent", "Whole"}
ASSUME FloorMode \in {"Positive", "Strict"}
ASSUME RelayMode \in {"Committed", "All"}
ASSUME LocalMode \in {"Exempt", "Judged"}
ASSUME BranchMode \in {"Exempt", "Judged"}
ASSUME HeightMode \in {"NonNegative", "Any"}

\* An unset height field. Any negative number has no valid height, so the
\* model writes an unset field as -1; Go treats both the same way.
None == -1

\* The versions peers push, each the head of its document's history so far:
\*   x: created at 0 (c1), updated to 2 (u1), then deleted at 0 (d1);
\*   y: created at 1 (c2);
\*   z: created with no height (c3), then updated to 2 (u3).
Events == {"c1", "u1", "d1", "c2", "c3", "u3"}
\* The document each version is of.
Doc(e) == CASE e \in {"c1", "u1", "d1"} -> "x" [] e = "c2" -> "y" [] OTHER -> "z"
\* What each version does: create, update or delete.
Kind(e) == CASE e \in {"c1", "c2", "c3"} -> "create" [] e \in {"u1", "u3"} -> "update" [] OTHER -> "delete"
\* The height each version leaves on its document.
Height(e) == CASE e = "c1" -> 0 [] e = "u1" -> 2 [] e = "d1" -> 0 [] e = "c2" -> 1 [] e = "c3" -> None [] OTHER -> 2
\* The versions before e in its document's history, oldest first.
Before(e) == CASE e = "u1" -> {"c1"} [] e = "d1" -> {"c1", "u1"} [] e = "u3" -> {"c3"} [] OTHER -> {}

\* What the rule is meant to keep, whatever the code does: an alive document
\* with a non-negative height above a positive floor, or any non-negative
\* height when the floor is zero or less.
Keeps(h) == h >= 0 /\ (Floor <= 0 \/ h > Floor)
\* Whether the head e leaves a document the rule is meant to keep.
HeadKept(e) == Kind(e) = "delete" \/ Branchable \/ Keeps(Height(e))

\* The code's check on one version, with its guards as switched: Go's
\* CheckRetention, RetentionFloor::refusal in Rust.
Valid(h) == HeightMode = "Any" \/ h >= 0
Refused(e) ==
  /\ JudgeMode # "Never"
  /\ (~Branchable \/ BranchMode = "Judged")
  /\ (Kind(e) # "delete" \/ DeleteMode = "Judge")
  /\ (~Valid(Height(e)) \/ ((FloorMode = "Strict" \/ Floor > 0) /\ Height(e) <= Floor))

VARIABLES
  outcome,   \* each version's outcome: "pending", "committed" or "rejected"
  stored,    \* documents stored and alive, with the height they carry, as [doc |-> height]
  merged,    \* versions whose blocks are marked merged
  relayed,   \* heads relayed to peers
  local      \* the documents written on this node

\* Every variable, for UNCHANGED.
vars == <<outcome, stored, merged, relayed, local>>

\* The start: nothing has arrived, nothing is stored.
Init ==
  /\ outcome = [e \in Events |-> "pending"]
  /\ stored = [d \in {} |-> 0]
  /\ merged = {}
  /\ relayed = {}
  /\ local = {}

\* The versions the walk from head e merges: its unmerged history, then e.
Walk(e) == {f \in Before(e) : f \notin merged} \cup {e}

\* Whether the walk from head e is refused: on the head alone, or with the
\* Each switch on any version it walks.
WalkRefused(e) ==
  IF JudgeMode = "Each" THEN \E f \in Walk(e) : Refused(f) ELSE Refused(e)

\* The documents stored once the accepted heads' walks commit: a delete
\* removes its document, any other head leaves it at the head's height.
Apply(ok) ==
  LET alive == {Doc(e) : e \in {f \in ok : Kind(f) # "delete"}}
      gone == {Doc(e) : e \in {f \in ok : Kind(f) = "delete"}}
  IN [d \in (DOMAIN stored \cup alive) \ gone |->
        IF d \in alive THEN Height(CHOOSE e \in ok : Doc(e) = d) ELSE stored[d]]

\* A batch of pushed heads, at most one per document and none already
\* merged, is merged. Each head gets its own verdict; with the Whole switch
\* one refusal refuses all of them.
Merge(batch) ==
  LET refusedHere == {e \in batch : WalkRefused(e)}
      ok == IF BatchMode = "Whole" /\ refusedHere # {} THEN {} ELSE batch \ refusedHere
      walked == UNION {Walk(e) : e \in ok}
  IN
  /\ batch # {}
  /\ \A e \in batch : e \notin merged
  /\ \A e, f \in batch : Doc(e) = Doc(f) => e = f
  /\ outcome' = [e \in Events |-> IF e \in walked THEN "committed"
                                  ELSE IF e \in batch THEN "rejected" ELSE outcome[e]]
  /\ stored' = Apply(ok)
  /\ merged' = merged \cup walked
  /\ relayed' = relayed \cup (IF RelayMode = "All" THEN batch ELSE ok)
  /\ UNCHANGED local

\* The node writes document w at height 0 itself. With the Judged switch a
\* local write goes through the rule too.
LocalWrite ==
  /\ "w" \notin local
  /\ local' = local \cup {"w"}
  /\ stored' = IF LocalMode = "Judged" /\ ~Keeps(0) /\ ~Branchable
                 THEN stored
                 ELSE [d \in DOMAIN stored \cup {"w"} |-> IF d = "w" THEN 0 ELSE stored[d]]
  /\ UNCHANGED <<outcome, merged, relayed>>

\* Any actor may move next: a batch of up to two heads, or the local write.
Next ==
  \/ \E e \in Events : Merge({e})
  \/ \E e, f \in Events : Merge({e, f})
  \/ LocalWrite

\* The run.
Spec == Init /\ [][Next]_vars

\* Invariant: every alive replicated document of a judged collection carries
\* a height the rule keeps.
INV_NoneBelowFloor ==
  \A d \in DOMAIN stored \ local :
    (~Branchable) => Keeps(stored[d])

\* Invariant: a refused head is neither merged nor relayed.
INV_RefusedNotMerged ==
  \A e \in Events : outcome[e] = "rejected" => e \notin merged /\ e \notin relayed

\* Invariant: a delete is never refused.
INV_DeleteApplies == \A e \in Events : Kind(e) = "delete" => outcome[e] # "rejected"

\* Invariant: a head whose document the rule keeps is never refused, however
\* its older versions or its batch's other heads fare.
INV_HeadDecides == \A e \in Events : HeadKept(e) => outcome[e] # "rejected"

\* Invariant: a local write is always stored, and nothing in a branchable
\* collection is refused.
INV_LocalAndBranchable ==
  /\ ("w" \in local => "w" \in DOMAIN stored)
  /\ (Branchable => \A e \in Events : outcome[e] # "rejected")
====
