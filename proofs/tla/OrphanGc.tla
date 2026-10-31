---- MODULE OrphanGc ----
\* The story: a node fetches blocks from its peers during P2P sync. Each
\* fetched block is stored with a to-merge mark holding the time it was
\* fetched; merging the block clears the mark. A fetch that is abandoned
\* leaves its block marked forever, and nothing else ever deletes it: a
\* history prune walks out from documents, and no document reaches it. So a
\* periodic sweep reclaims blocks whose mark is older than a cutoff.
\*
\* Who acts:
\*   - the fetcher, which stores a block with a fresh mark; when a peer
\*     sends a stored block again, the put finds it and writes nothing;
\*   - the merger, which merges a block: it clears the block's mark and the
\*     mark of the block's signature block, and records both as owned by
\*     the document, all in one transaction;
\*   - a local write, which may adopt a stored block (a field value it
\*     writes is byte-identical to a fetched one) by recording an owner,
\*     without touching the mark;
\*   - time, which ages a fresh mark past the cutoff;
\*   - the sweep, which nominates expired marks from a snapshot, then
\*     decides each nominee in a transaction: an unowned block is deleted
\*     with its mark (reclaimed), an owned one only loses its mark
\*     (repaired). A merge that commits first aborts the decision.
\* A block written by an older build carries a mark with no time; the
\* sweep never touches it.
\*
\* What must never happen:
\*   - a merged block is deleted;
\*   - a block a document owns is deleted;
\*   - a merged block still carries a to-merge mark (it would read as not
\*     merged and be fetched again, or be swept);
\*   - a merged block's signature block is not owned by the document;
\*   - a block whose mark has no time is deleted.
\* What must eventually happen: once nothing else acts, a stored block that
\* is unowned and whose mark expired is reclaimed.
\*
\* Code it models:
\*   crates/storage/src/stores/blockstore.rs BlockWritesAttempt::run (put),
\*   marker_value, marker_time;
\*   crates/db/src/block/orphan_gc.rs DB::reclaim_orphan_blocks, nominate_orphans,
\*   DB::decide_orphans (the decision);
\*   crates/db/src/merge/merge_handler/doc_identity.rs record_block_ownership
\*   and the merge's mark clearing (the signature block's owner and mark).
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxActs,      \* how many refetches and adoptions may happen in all
  ReadMode,     \* "Validated": the decision re-reads mark and owners in its transaction | "Snapshot": it trusts the nomination
  OwnerMode,    \* "Checked": an owned block is repaired, not deleted | "Ignored": every expired block is deleted
  RefreshMode,  \* "None": sending a stored block again changes nothing | "Blind": it marks the block again
  LegacyMode,   \* "Skip": a mark with no time is never nominated | "Reclaim": it is treated as expired
  SigMarkMode,  \* "Clear": merging clears the signature block's mark | "Leave": it stays
  SigOwnerMode  \* "Owned": merging records the signature block's owner | "Unowned": it does not

\* Each switch takes one of its values.
ASSUME MaxActs \in Nat
ASSUME ReadMode \in {"Validated", "Snapshot"}
ASSUME OwnerMode \in {"Checked", "Ignored"}
ASSUME RefreshMode \in {"None", "Blind"}
ASSUME LegacyMode \in {"Skip", "Reclaim"}
ASSUME SigMarkMode \in {"Clear", "Leave"}
ASSUME SigOwnerMode \in {"Owned", "Unowned"}

\* The blocks: b, a block a peer sends; s, b's signature block, which comes
\* with it; o, a block nobody ever merges (an abandoned fetch); l, a block
\* an older build stored, whose mark has no time.
Blocks == {"b", "s", "o", "l"}
\* The signature block each block links, if it has one.
Sig(x) == IF x = "b" THEN {"s"} ELSE {}
\* The blocks a merge can merge: b, with its signature block.
Mergeable == {"b"}

\* A mark: "none" (no mark: merged, or never stored), "fresh" (fetched
\* recently), "old" (fetched before the cutoff), "legacy" (no time).
Marks == {"none", "fresh", "old", "legacy"}

VARIABLES
  stored,   \* the blocks in the blockstore
  mark,     \* each block's to-merge mark
  owned,    \* the blocks some document records as its own
  merged,   \* the blocks whose merge committed
  nominees, \* the blocks the sweep nominated and has not decided yet
  seen,     \* what the nomination saw of each nominee: its mark and whether it was owned
  acts      \* how many refetches and adoptions have happened

vars == <<stored, mark, owned, merged, nominees, seen, acts>>

\* At the start the older build's block l is stored with its untimed mark;
\* nothing else is stored, owned or merged, and the sweep is idle.
Init ==
  /\ stored = {"l"}
  /\ mark = [x \in Blocks |-> IF x = "l" THEN "legacy" ELSE "none"]
  /\ owned = {}
  /\ merged = {}
  /\ nominees = {}
  /\ seen = [x \in Blocks |-> <<"none", FALSE>>]
  /\ acts = 0

\* A peer sends block x (and b's signature block with it): each one not yet
\* stored is stored with a fresh mark.
Fetch(x) ==
  /\ x \in {"b", "o"}
  /\ x \notin stored
  /\ LET new == {x} \cup Sig(x) IN
       /\ stored' = stored \cup new
       /\ mark' = [y \in Blocks |-> IF y \in new \ stored THEN "fresh" ELSE mark[y]]
  /\ UNCHANGED <<owned, merged, nominees, seen, acts>>

\* A peer sends a stored block x again. None: the put finds x stored and
\* writes nothing, so a merged block stays merged and a waiting one keeps
\* its age. Blind: x is marked fresh whatever it was.
Refetch(x) ==
  /\ acts < MaxActs
  /\ x \in stored \ {"l"}
  /\ IF RefreshMode = "None"
       THEN UNCHANGED mark
       ELSE mark' = [mark EXCEPT ![x] = "fresh"]
  /\ acts' = acts + 1
  /\ UNCHANGED <<stored, owned, merged, nominees, seen>>

\* Time passes: a fresh mark becomes older than the cutoff.
Age(x) ==
  /\ mark[x] = "fresh"
  /\ mark' = [mark EXCEPT ![x] = "old"]
  /\ UNCHANGED <<stored, owned, merged, nominees, seen, acts>>

\* The merge of a stored, unmerged block x commits: x and its signature
\* block are merged; x's mark goes, and the signature block's too unless
\* SigMarkMode leaves it; x is owned, and the signature block too unless
\* SigOwnerMode forgets it.
Merge(x) ==
  /\ x \in Mergeable
  /\ x \in stored
  /\ x \notin merged
  /\ Sig(x) \subseteq stored
  /\ merged' = merged \cup {x} \cup Sig(x)
  /\ mark' = [y \in Blocks |->
                IF y = x THEN "none"
                ELSE IF y \in Sig(x) /\ SigMarkMode = "Clear" THEN "none"
                ELSE mark[y]]
  /\ owned' = owned \cup {x} \cup (IF SigOwnerMode = "Owned" THEN Sig(x) ELSE {})
  /\ UNCHANGED <<stored, nominees, seen, acts>>

\* A local write adopts the stored block x: it records an owner and leaves
\* the mark as it is (the mark outlives the claim).
Adopt(x) ==
  /\ acts < MaxActs
  /\ x \in stored \ {"l"}
  /\ x \notin owned
  /\ owned' = owned \cup {x}
  /\ acts' = acts + 1
  /\ UNCHANGED <<stored, mark, merged, nominees, seen>>

\* Whether a mark makes its block a candidate: an expired mark, and with
\* LegacyMode "Reclaim" an untimed one too.
Expired(m) == m = "old" \/ (LegacyMode = "Reclaim" /\ m = "legacy")

\* The sweep reads a snapshot: every stored block whose mark expired is
\* nominated, and the snapshot remembers its mark and whether it was owned.
Nominate ==
  /\ nominees = {}
  /\ LET found == {x \in stored : Expired(mark[x])} IN
       /\ found # {}
       /\ nominees' = found
       /\ seen' = [x \in Blocks |-> IF x \in found THEN <<mark[x], x \in owned>> ELSE seen[x]]
  /\ UNCHANGED <<stored, mark, owned, merged, acts>>

\* The sweep decides nominee x in its own transaction. Validated: it reads
\* x's mark and owners now (a merge that committed since the nomination
\* shows); Snapshot: it uses what the nomination saw. A block whose mark
\* is gone is left alone; an owned one loses its mark (OwnerMode "Ignored"
\* deletes it anyway); any other is deleted with its mark.
Decide(x) ==
  /\ x \in nominees
  /\ LET m == IF ReadMode = "Validated" THEN mark[x] ELSE seen[x][1]
         isOwned == IF ReadMode = "Validated" THEN x \in owned ELSE seen[x][2] IN
       IF ~Expired(m)
         THEN UNCHANGED <<stored, mark>>
       ELSE IF isOwned /\ OwnerMode = "Checked"
         THEN /\ mark' = [mark EXCEPT ![x] = "none"]
              /\ UNCHANGED stored
       ELSE /\ stored' = stored \ {x}
            /\ mark' = [mark EXCEPT ![x] = "none"]
  /\ nominees' = nominees \ {x}
  /\ UNCHANGED <<owned, merged, seen, acts>>

Next ==
  \/ \E x \in Blocks : Fetch(x) \/ Refetch(x) \/ Age(x) \/ Merge(x) \/ Adopt(x) \/ Decide(x)
  \/ Nominate

\* The sweep keeps running, and fetches and merges happen when they can.
Fairness ==
  /\ WF_vars(Nominate)
  /\ \A x \in Blocks : WF_vars(Decide(x)) /\ WF_vars(Fetch(x)) /\ WF_vars(Age(x))

Spec == Init /\ [][Next]_vars /\ Fairness

\* Every variable holds a value of its kind.
TypeOK ==
  /\ stored \subseteq Blocks
  /\ mark \in [Blocks -> Marks]
  /\ owned \subseteq Blocks
  /\ merged \subseteq Blocks
  /\ nominees \subseteq Blocks
  /\ acts \in 0..MaxActs

\* A merged block is never deleted.
INV_NoMergedLoss == merged \subseteq stored

\* A block a document owns is never deleted.
INV_NoOwnedLoss == owned \subseteq stored

\* A merged block carries no to-merge mark.
INV_MergedUnmarked == \A x \in merged : mark[x] = "none"

\* A merged block's signature block is owned by the document.
INV_SigOwned == \A x \in merged : Sig(x) \subseteq owned

\* The older build's block, whose mark has no time, is never deleted.
INV_LegacyKept == "l" \in stored

\* Once the refetches and adoptions are spent, the abandoned block o,
\* fetched and never merged or owned, is eventually reclaimed.
PROP_Reclaims == ("o" \in stored /\ acts = MaxActs /\ "o" \notin owned) ~> ("o" \notin stored)
====
