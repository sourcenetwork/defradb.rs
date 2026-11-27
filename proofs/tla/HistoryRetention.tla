---- MODULE HistoryRetention ----
\* The story: a collection declares @retention(maxDepth: N), and an operator
\* asks the node to prune its history. For each document the prune keeps the
\* composite commits of the top N heights and deletes the older ones, with the
\* field blocks no kept composite links and no field has as its head. It runs
\* as one regolith transaction per document, which may be attempted more than
\* once: an attempt that conflicts runs again on fresh reads. It never runs
\* inside a client's explicit transaction (the node refuses that), so each
\* attempt is its own transaction. The operator may prune more than once.
\*
\* Who acts, while the prune runs:
\*   - a local update of the document, which writes a new composite and a new
\*     `name` field block on top of the current heads, so f1 stops being the
\*     name field's head;
\*   - a merge of another document n, whose field value matches one of the
\*     document's field blocks, so it adopts the stored block by writing only
\*     an owner edge;
\*   - a peer that sends a pruned composite again (a replay of old history):
\*     the node fetches it and the field blocks it links back into its store,
\*     with no owner, and the merge finds it already applied.
\* And anyone may read the document at one of its composites: a version read
\* walks the composite's parents and field blocks and rebuilds the document.
\*
\* The document a: composites c1..c4 at heights 1..4 on one line, and b2, a
\* concurrent branch at height 2 that is still a head beside c4. f1 is the
\* `name` field block c1 wrote, still that field's head; f2, f3, f4 and fb are
\* the field blocks the other composites wrote. s is a field block c1 shares
\* with document d, which owns it too.
\*
\* What must never happen:
\*   - a live head, composite or field, is deleted;
\*   - a block some document owns is deleted (including one a merge adopted
\*     while the prune ran);
\*   - a composite inside the window (the top N heights) is deleted;
\*   - a pruned composite a peer sends again becomes a head again;
\*   - a version read returns a document rebuilt from part of its history.
\* And the prune does its job: once it commits, c2 (below the window, not a
\* head, owned by a alone) is gone, and so is every field block the attempt
\* saw that is not kept and no other document owns, including one an earlier
\* prune kept only as a head, once a newer write superseded it.
\*
\* Code it models (crates/database/db/src/collection/ops/prune_history.rs):
\*   DB::prune_history_as, DB::prune_doc_history (one DB::transact_db attempt
\*   per document) and prune_doc_in (the window, the kept heads, the retained
\*   field blocks, the field blocks the priority index names);
\*   crate::block::cleanup::delete_owned_commit and delete_owned_block (owner edges
\*   removed blindly, a validated owner scan deciding each delete, through
\*   crate::docid::map::block_has_owners); and the merge's publication check
\*   crates/database/db/src/merge/merge_handler/history.rs composite_was_applied,
\*   which reads the priority index entries the prune keeps; and the version
\*   read crates/database/db/src/read/versioned.rs
\*   VersionedFetcher::collect_blocks_to_genesis and refuse_pruned.
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxDepth,     \* N in @retention(maxDepth: N)
  MaxActs,      \* how many updates, adoptions and replays may happen in all
  HeadMode,     \* "Keep": a live composite head is never pruned | "Prune": it is, by height alone
  FieldMode,    \* "Keep": live field heads are retained | "Prune": only kept composites' links are
  ScanMode,     \* "Validated": the owner scan deciding a delete is validated | "Plain": it is not
  MarkerMode,   \* "Keep": a pruned commit's priority entry stays | "Drop": it is deleted with the block
  WindowMode,   \* "Exact": prune heights <= top - N | "OffByOne": prune heights <= top - N + 1
  ReclaimMode,  \* "Scan": every field block the priority index names may go | "Links": only pruned composites' links
  ReadMode,     \* "Refuse": a version read that meets a pruned block fails | "Skip": it skips the block
  MaxPrunes     \* how many prunes the operator runs

\* Each switch takes one of its values.
ASSUME MaxDepth \in Nat \ {0}
ASSUME MaxActs \in Nat
ASSUME HeadMode \in {"Keep", "Prune"}
ASSUME FieldMode \in {"Keep", "Prune"}
ASSUME ScanMode \in {"Validated", "Plain"}
ASSUME MarkerMode \in {"Keep", "Drop"}
ASSUME WindowMode \in {"Exact", "OffByOne"}
ASSUME ReclaimMode \in {"Scan", "Links"}
ASSUME ReadMode \in {"Refuse", "Skip"}
ASSUME MaxPrunes \in Nat

\* Document a's composites, c5 being the one a local update writes.
Composites == {"c1", "c2", "b2", "c3", "c4", "c5"}
\* The field blocks; f5 is the one the update writes, d1 is d's composite.
Fields == {"f1", "f2", "fb", "f3", "f4", "f5", "s"}
\* Every block.
Blocks == Composites \cup Fields \cup {"d1"}
\* The documents: a is pruned, d shares s, n is merged during the prune.
Docs == {"a", "d", "n"}

\* Each composite's height (its priority).
H(c) == CASE c = "c1" -> 1 [] c = "c2" -> 2 [] c = "b2" -> 2
          [] c = "c3" -> 3 [] c = "c4" -> 4 [] c = "c5" -> 5 [] OTHER -> 0

\* The field blocks a composite links (its parents are not field links).
FieldsOf(c) == CASE c = "c1" -> {"f1", "s"} [] c = "c2" -> {"f2"} [] c = "b2" -> {"fb"}
                 [] c = "c3" -> {"f3"} [] c = "c4" -> {"f4"} [] c = "c5" -> {"f5"}
                 [] OTHER -> {}
\* The field blocks of a set of composites.
LinkedBy(cs) == UNION {FieldsOf(c) : c \in cs}

\* Each composite's parents: the line c1..c4, b2 branching off c1, and c5
\* written on top of the two heads c4 and b2.
Parents(c) == CASE c = "c2" -> {"c1"} [] c = "b2" -> {"c1"} [] c = "c3" -> {"c2"}
              [] c = "c4" -> {"c3"} [] c = "c5" -> {"c4", "b2"} [] OTHER -> {}

\* Everything a version read from the blocks in S walks to: their parents and
\* field blocks, again and again until nothing new turns up.
RECURSIVE Reach(_)
Reach(S) == LET next == S \cup UNION {Parents(x) \cup FieldsOf(x) : x \in S}
            IN IF next = S THEN S ELSE Reach(next)

\* The largest height in a non-empty set of composites.
Top(cs) == CHOOSE h \in {H(c) : c \in cs} : \A c \in cs : H(c) <= h

VARIABLES
  present,     \* blocks the blockstore holds
  owners,      \* for each block, the documents its owner edges name
  heads,       \* a's live composite heads
  fieldHeads,  \* a's live field heads
  published,   \* blocks whose priority index entry says they were published
  phase,       \* "idle" (an attempt can start) or "running"
  snap,        \* what the running attempt read: [owners, heads, fieldHeads, present]
  touched,     \* blocks whose owner edges changed since the attempt began
  dirty,       \* blocks stored again since the attempt began
  pruned,      \* blocks a committed prune deleted, kept to check replays
  acts,        \* how many updates, adoptions and replays happened
  prunes,      \* how many prunes committed
  justPruned   \* TRUE right after a prune commits, until the next step

\* Every variable, for UNCHANGED.
vars == <<present, owners, heads, fieldHeads, published, phase, snap, touched, dirty, pruned, acts,
          prunes, justPruned>>

\* The blocks stored at the start: everything but the update's c5 and f5.
Stored0 == Blocks \ {"c5", "f5"}
\* The owner edges at the start: a owns its blocks, d owns d1 and s.
Owners0 == [blk \in Blocks |->
              IF blk \in {"d1"} THEN {"d"}
              ELSE IF blk = "s" THEN {"a", "d"}
              ELSE IF blk \in {"c5", "f5"} THEN {}
              ELSE {"a"}]

\* Every value each variable may hold.
TypeOK ==
  /\ present \subseteq Blocks
  /\ owners \in [Blocks -> SUBSET Docs]
  /\ heads \subseteq Composites
  /\ fieldHeads \subseteq Fields
  /\ published \subseteq Blocks
  /\ phase \in {"idle", "running"}
  /\ touched \subseteq Blocks
  /\ dirty \subseteq Blocks
  /\ pruned \subseteq Blocks
  /\ acts \in 0..MaxActs
  /\ prunes \in 0..MaxPrunes
  /\ justPruned \in BOOLEAN

\* The start: c4 and b2 are a's composite heads; f1 (name), f4 (age) and fb
\* its field heads; every stored block was published.
Init ==
  /\ present = Stored0
  /\ owners = Owners0
  /\ heads = {"c4", "b2"}
  /\ fieldHeads = {"f1", "f4", "fb"}
  /\ published = Stored0
  /\ phase = "idle"
  /\ snap = [owners |-> Owners0, heads |-> {"c4", "b2"},
             fieldHeads |-> {"f1", "f4", "fb"}, present |-> Stored0]
  /\ touched = {}
  /\ dirty = {}
  /\ pruned = {}
  /\ acts = 0
  /\ prunes = 0
  /\ justPruned = FALSE

\* An attempt begins: it reads heads, owners and blocks from a snapshot taken now.
Begin ==
  /\ phase = "idle"
  /\ prunes < MaxPrunes
  /\ phase' = "running"
  /\ snap' = [owners |-> owners, heads |-> heads, fieldHeads |-> fieldHeads, present |-> present]
  /\ touched' = {}
  /\ dirty' = {}
  /\ justPruned' = FALSE
  /\ UNCHANGED <<present, owners, heads, fieldHeads, published, pruned, acts, prunes>>

\* The composites the attempt found: a's composites its snapshot holds.
Found == snap.present \cap Composites
\* The lowest height the window keeps is above this one.
Cutoff == IF WindowMode = "Exact"
          THEN Top(Found) - MaxDepth
          ELSE Top(Found) - MaxDepth + 1
\* The composites the attempt prunes: at or below the cutoff, and, with the
\* Keep switch, not a live head.
PrunedC == {c \in Found : H(c) <= Cutoff /\ (HeadMode = "Prune" \/ c \notin snap.heads)}
\* The field blocks kept: those a kept composite links, and, with the Keep
\* switch, every live field head.
Retained == LinkedBy(Found \ PrunedC) \cup (IF FieldMode = "Keep" THEN snap.fieldHeads ELSE {})
\* The field blocks the attempt may delete: with the Scan switch, every field
\* block of a its snapshot holds (the priority index names them all); with
\* the Links switch, only those a pruned composite links.
FieldCandidates == IF ReclaimMode = "Scan"
                   THEN snap.present \cap Fields
                   ELSE LinkedBy(PrunedC)
\* The blocks whose owner edge for a the attempt removes.
Walked == PrunedC \cup (FieldCandidates \ Retained)
\* The walked blocks no other document owns, as the snapshot saw them: the
\* attempt deletes these.
Doomed == {blk \in Walked : snap.owners[blk] \ {"a"} = {}}
\* The commit must conflict: a validated owner scan of a walked block saw a
\* change, or a doomed block was stored again (its delete is validated).
Conflicts == (ScanMode = "Validated" /\ touched \cap Walked # {}) \/ dirty \cap Doomed # {}

\* The attempt conflicts and runs again later, on fresh reads.
Abort ==
  /\ phase = "running"
  /\ Conflicts
  /\ phase' = "idle"
  /\ justPruned' = FALSE
  /\ UNCHANGED <<present, owners, heads, fieldHeads, published, snap, touched, dirty, pruned, acts,
                 prunes>>

\* The attempt commits: a's edges on walked blocks go (blind deletes, applied
\* to the edges as they stand), the doomed blocks go, and, with the Drop
\* switch, their priority entries too. The operator may prune again later.
Commit ==
  /\ phase = "running"
  /\ ~Conflicts
  /\ phase' = "idle"
  /\ prunes' = prunes + 1
  /\ justPruned' = TRUE
  /\ owners' = [blk \in Blocks |-> IF blk \in Walked THEN owners[blk] \ {"a"} ELSE owners[blk]]
  /\ present' = present \ Doomed
  /\ published' = IF MarkerMode = "Drop" THEN published \ Doomed ELSE published
  /\ pruned' = pruned \cup Doomed
  /\ UNCHANGED <<heads, fieldHeads, snap, touched, dirty, acts>>

\* A local update of a: c5 and f5 on top of the current heads, owned by a and
\* published; c5 becomes the only composite head and f5 replaces the name
\* field's head f1. It writes only new keys, so it conflicts with nothing.
Update ==
  /\ acts < MaxActs
  /\ "c5" \notin present
  /\ heads \subseteq present
  /\ present' = present \cup {"c5", "f5"}
  /\ owners' = [owners EXCEPT !["c5"] = {"a"}, !["f5"] = {"a"}]
  /\ heads' = {"c5"}
  /\ fieldHeads' = (fieldHeads \ {"f1"}) \cup {"f5"}
  /\ published' = published \cup {"c5", "f5"}
  /\ acts' = acts + 1
  /\ justPruned' = FALSE
  /\ UNCHANGED <<phase, snap, touched, dirty, pruned, prunes>>

\* A merge of document n finds a field block of a already stored and adopts
\* it: a blind put of its owner edge, nothing more.
Adopt(blk) ==
  /\ acts < MaxActs
  /\ blk \in present \cap Fields
  /\ owners' = [owners EXCEPT ![blk] = @ \cup {"n"}]
  /\ touched' = IF phase = "running" THEN touched \cup {blk} ELSE touched
  /\ acts' = acts + 1
  /\ justPruned' = FALSE
  /\ UNCHANGED <<present, heads, fieldHeads, published, phase, snap, dirty, pruned, prunes>>

\* A peer sends a composite of a that is not stored. The node fetches it and
\* the field blocks it links to complete the DAG; they arrive with no owner.
\* If its priority entry says it was published, the merge sees it as applied
\* and does nothing more: the bytes stay, unowned, for the next prune.
\* Otherwise the merge applies it as new: stores it, owns it, publishes it,
\* and makes it a head.
Replay(c) ==
  /\ acts < MaxActs
  /\ c \in Composites \ {"c5"}
  /\ c \notin present
  /\ acts' = acts + 1
  /\ justPruned' = FALSE
  /\ IF c \in published
     THEN LET back == {c} \cup FieldsOf(c)
          IN /\ present' = present \cup back
             /\ dirty' = IF phase = "running" THEN dirty \cup back ELSE dirty
             /\ UNCHANGED <<owners, heads, published>>
     ELSE /\ present' = present \cup {c}
          /\ owners' = [owners EXCEPT ![c] = @ \cup {"a"}]
          /\ heads' = heads \cup {c}
          /\ published' = published \cup {c}
          /\ dirty' = IF phase = "running" THEN dirty \cup {c} ELSE dirty
  /\ UNCHANGED <<fieldHeads, phase, snap, touched, pruned, prunes>>

\* Every step the system can take.
Next ==
  \/ Begin
  \/ Abort
  \/ Commit
  \/ Update
  \/ \E blk \in Fields : Adopt(blk)
  \/ \E c \in Composites : Replay(c)

\* The whole behaviour: start in Init and take Next steps.
Spec == Init /\ [][Next]_vars

\* Every live composite head is stored.
INV_HeadsStored == heads \subseteq present

\* Every live field head is stored.
INV_FieldHeadsStored == fieldHeads \subseteq present

\* Every block some document owns is stored.
INV_OwnedStored == \A blk \in Blocks : owners[blk] # {} => blk \in present

\* Every published composite within N heights of the highest published one is stored.
INV_WindowKept ==
  LET written == published \cap Composites
  IN  \A c \in written : H(c) + MaxDepth > Top(written) => c \in present

\* A composite a committed prune deleted never becomes a head again.
INV_NoStaleHead == heads \cap pruned = {}

\* Right after a prune commits, c2 (below the window, no head, owned by a alone)
\* is gone, unless a peer sent it back while the attempt ran (the next prune
\* takes it then).
INV_Pruned == justPruned => ("c2" \notin present \/ "c2" \in dirty)

\* Right after a prune commits, every field block its attempt saw is gone
\* unless it is kept (a kept composite links it or it was a field head),
\* another document owns it, or it was stored again while the attempt ran.
INV_Reclaimed ==
  justPruned =>
    \A blk \in (snap.present \cap Fields) \ (Retained \cup dirty) :
      blk \notin present \/ owners[blk] \ {"a"} # {}

\* What a version read at composite c returns: the whole document when every
\* block it walks to is stored; a refusal when one is missing and its
\* priority entry says a prune deleted it (with the Refuse switch); otherwise
\* a document rebuilt from the blocks it found, which is partial.
ReadOutcome(c) ==
  LET missing == Reach({c}) \ present
  IN IF missing = {} THEN "whole"
     ELSE IF ReadMode = "Refuse" /\ missing \cap published # {} THEN "refused"
     ELSE "partial"

\* No version read of a stored composite returns a partial document.
INV_NoPartialRead == \A c \in present \cap Composites : ReadOutcome(c) # "partial"
====
