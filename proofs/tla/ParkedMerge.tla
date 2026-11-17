------------------------------ MODULE ParkedMerge ------------------------------
\* The story, in plain words.
\*
\* A node merges replicated changes to one document whose access is decided
\* by a remote provider. Three changes arrive:
\*   * "x1", signed by w, who may or may not hold write when it arrives;
\*   * "x2", signed by c, built on top of x1 (it descends from x1, so it can
\*     only merge after x1: its history contains x1);
\*   * "y", signed by c, on another branch that does not contain x1.
\* c always holds write. The owner may grant w write and revoke it again,
\* any number of times.
\*
\* When the provider refuses a change's signer, the node does not reject the
\* change for good (a rejection is final for that block on that node).
\* It parks the change instead: the change waits, indexed under its
\* document, and is tried again when that document's relationships change
\* on this node, and on a bounded schedule (the sweep). A change that arrives
\* on top of a parked one is parked under the same document: its history
\* holds the parked change. A change on another branch is not held up.
\*
\* The index is bounded. A change that does not fit is not indexed: it stays
\* unmerged and the replication layer tries it again on its own retry clock,
\* as every retryable skip is tried again.
\*
\* What must never happen:
\*   - a change is rejected for good because its signer was refused
\*     - INV_NeverQuarantined;
\*   - a change merges while its signer does not hold write, such as after
\*     the signer's revocation - INV_MergeAuthorized;
\*   - x2 merges before x1 - INV_HistoryFirst;
\*   - more changes are indexed than the index holds - INV_Bounded;
\*   - once w holds write for good, x1 and x2 do not end merged
\*     - LIVE_RedrivenOnGrant;
\*   - y does not end merged, whatever happens to x1 - LIVE_OtherBranchMerges.
\*
\* Rust owners: crates/db/src/merge/merge_handler/composite.rs
\* prepare_composite_merge (park); crates/db/src/merge/governance/
\* awaited.rs WaitKey::AcpObject; crates/db/src/merge/governance/
\* deferred.rs DeferredMerges (defer, release, MAX_WAITERS_PER_DEPENDENCY,
\* MAX_DEFERRED_COMPOSITES); crates/db/src/merge/merge_handler/parked_acp.rs
\* ParkedRelease, sweep_parked_acp; crates/acp/src/external.rs
\* ExternalDocumentAcp::watch_object_changes.
EXTENDS Naturals, FiniteSets

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug
\* How many changes the index holds.
CONSTANT Capacity

\* The changes.
Commits == {"x1", "x2", "y"}
\* Who signed each change.
Signer(k) == IF k = "x1" THEN "w" ELSE "c"
\* The changes each change's history contains, beside itself.
History(k) == IF k = "x2" THEN {"x1"} ELSE {}

VARIABLES
  writes,   \* TRUE while w holds write
  status,   \* per change: "pending", "parked", "merged" or "rejected"
  indexed,  \* the parked changes the index holds
  changed   \* TRUE when the document's relationships changed since the last release

vars == <<writes, status, indexed, changed>>

\* Nothing has arrived; w may or may not hold write.
Init ==
  /\ writes \in BOOLEAN                         \* either way at the start
  /\ status = [k \in Commits |-> "pending"]     \* every change is still to come
  /\ indexed = {}                               \* the index is empty
  /\ changed = FALSE                            \* nothing changed yet

\* Whether the provider lets s write now.
HoldsWrite(s) == s = "c" \/ writes

\* Whether k's history is merged.
HistoryMerged(k) == \A h \in History(k) : status[h] = "merged"

\* Park k: index it if there is room. The planted bugs reject it for good,
\* or index it past the room there is.
Park(k) ==
  IF Bug = "quarantine"
    THEN /\ status' = [status EXCEPT ![k] = "rejected"]
         /\ indexed' = indexed
    ELSE /\ status' = [status EXCEPT ![k] = "parked"]
         /\ indexed' = IF Cardinality(indexed) < Capacity \/ Bug = "unbounded"
                         THEN indexed \cup {k}
                         ELSE indexed

\* Whether k waits behind a parked change: one its history holds. The
\* planted bug holds every change of the document behind any parked one.
Blocked(k) ==
  \/ ~HistoryMerged(k)
  \/ Bug = "parkBlocksObject" /\ \E j \in Commits \ {k} : status[j] = "parked"

\* The node tries change k: one not yet merged or rejected that is pending,
\* or that a release, the sweep or the replication retry hands back. If it
\* waits behind a parked change, it is parked too (the planted bug merges it
\* over its parked history). If its signer may write it merges; otherwise
\* it is parked. A planted bug merges a re-driven change without asking again.
Try(k) ==
  /\ status[k] \in {"pending", "parked"}
  /\ IF Blocked(k) /\ Bug # "mergeOverParked"
       THEN Park(k)
       ELSE IF HoldsWrite(Signer(k)) \/ (Bug = "redriveAdmits" /\ status[k] = "parked")
              THEN /\ status' = [status EXCEPT ![k] = "merged"]
                   /\ indexed' = indexed \ {k}
              ELSE Park(k)
  /\ UNCHANGED <<writes, changed>>

\* The owner grants w write.
Grant ==
  /\ ~writes
  /\ writes' = TRUE
  /\ changed' = TRUE
  /\ UNCHANGED <<status, indexed>>

\* The owner revokes w's write.
Revoke ==
  /\ writes
  /\ writes' = FALSE
  /\ changed' = TRUE
  /\ UNCHANGED <<status, indexed>>

\* A relationship change of the document releases what the index holds
\* under it: each is tried again.
Release(k) ==
  /\ changed
  /\ k \in indexed
  /\ Try(k)

\* The relationship change has been handed to every indexed change. The
\* planted bug forgets it at once, and also has no sweep: a parked change
\* then waits for ever.
Settle ==
  /\ changed
  /\ indexed = {} \/ Bug = "noRelease"
  /\ changed' = FALSE
  /\ UNCHANGED <<writes, status, indexed>>

\* The sweep, on its bounded schedule, tries an indexed change again.
Sweep(k) ==
  /\ Bug # "noRelease"
  /\ k \in indexed
  /\ Try(k)

\* The replication layer tries a parked change the index did not take
\* again, on its own retry clock.
Retry(k) ==
  /\ status[k] = "parked"
  /\ k \notin indexed
  /\ Try(k)

\* One step of anyone.
Next ==
  \/ \E k \in Commits : Try(k) /\ status[k] = "pending"
  \/ \E k \in Commits : Release(k) \/ Sweep(k) \/ Retry(k)
  \/ Grant
  \/ Revoke
  \/ Settle

\* The node keeps trying: arrivals, releases, the sweep and the retry clock
\* each come round. The owner grants and revokes as it likes.
Fairness ==
  /\ \A k \in Commits :
       /\ WF_vars(Try(k) /\ status[k] = "pending")
       /\ SF_vars(Sweep(k))
       /\ SF_vars(Retry(k))
       /\ SF_vars(Release(k))

Spec == Init /\ [][Next]_vars
LiveSpec == Spec /\ Fairness

\* The variables hold values of the expected kinds.
TypeOK ==
  /\ writes \in BOOLEAN
  /\ status \in [Commits -> {"pending", "parked", "merged", "rejected"}]
  /\ indexed \subseteq Commits
  /\ changed \in BOOLEAN

\* A refused change is never rejected for good.
INV_NeverQuarantined == \A k \in Commits : status[k] # "rejected"

\* A change merges only while its signer may write.
INV_MergeAuthorized ==
  [][\A k \in Commits :
       (status[k] # "merged" /\ status'[k] = "merged") => HoldsWrite(Signer(k))]_vars

\* x2 never merges before x1.
INV_HistoryFirst == status["x2"] = "merged" => status["x1"] = "merged"

\* The index never holds more than it may.
INV_Bounded == Cardinality(indexed) <= Capacity

\* Once w holds write for good, x1 and x2 end merged.
LIVE_RedrivenOnGrant ==
  <>[]writes => <>(status["x1"] = "merged" /\ status["x2"] = "merged")

\* y ends merged whatever happens to x1.
LIVE_OtherBranchMerges == <>(status["y"] = "merged")
==============================================================================
