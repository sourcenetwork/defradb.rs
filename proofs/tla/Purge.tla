---- MODULE Purge ----
\* The story: an operator asks the node to purge some documents by DocID.
\* A purge is a hard delete on this node only: the document's values, its
\* index entries, its DocID mappings and its heads go, and nothing is written
\* to replicate. The blocks of the document's history go too, except blocks
\* another document still owns: field blocks are content-addressed, so two
\* documents with the same value share one block. In a @branchable
\* collection the history stays: the collection DAG links every version's
\* composite, so it owns them and everything they link.
\*
\* Who acts: the purge, which runs as one regolith transaction that may be
\* attempted more than once (an attempt that conflicts runs again on fresh
\* reads); a P2P sync, which may store again a block the purge is walking;
\* and a merge of a new document n whose field value matches one of the
\* purged documents' field blocks, so it adopts the block it finds already
\* stored by writing only an owner edge. The sync and the merge act at most
\* MaxActs times between them.
\*
\* What must never happen:
\*   - a block another document needs is deleted, including one a merge
\*     adopted while the purge was running;
\*   - a purged document still has a value, an index entry or a DocID alias,
\*     so a later document with the same unique value is refused;
\*   - after the purge, a block still names a purged document as its owner,
\*     so the block can never be reclaimed (Go keeps these edges when it keeps
\*     the blocks; defradb.rs drops them);
\*   - outside a @branchable collection, a block only purged documents owned
\*     survives;
\*   - in a @branchable collection, a block of the history is deleted: the
\*     collection's `_commits` and its sync need every one;
\*   - the count is wrong: a DocID named twice, or one that names nothing,
\*     is counted, or an attempt that did not commit is counted;
\*   - a block stored again by a sync makes the purge fail instead of
\*     running again.
\*
\* Code it models (crates/db/src/collection/ops/purge.rs):
\*   DB::purge_by_doc_ids_as (one DB::transact_db attempt, in
\*   crates/db/src/txn/attempt.rs; a filtered truncate's chunk,
\*   truncate_filtered_chunk in collection/ops/truncate/filtered.rs, runs
\*   the same purge_documents in its own attempt),
\*   purge_documents (values, index entries, primary key, heads, mappings;
\*   the collection's @branchable flag picks how the history is treated),
\*   crate::block::cleanup::delete_owned_dag_for_owners (outside a branchable
\*   collection: a validated owner scan decides each delete) and
\*   crate::block::cleanup::drop_dag_owners (a branchable collection: blind
\*   deletes of the purged owner edges, every block kept).
EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
  Branchable,   \* TRUE: the collection is @branchable, so its collection DAG links every composite
  HistoryMode,  \* "ByCollection": history is pruned unless the collection is branchable | "Always": it is pruned regardless
  MaxActs,      \* how many times a sync or a merge may act during the purge
  OwnerMode,    \* "Checked": a block another document owns is kept | "Blind": every reached block goes
  EdgeMode,     \* "Drop": a purge that keeps the history drops the purged owner edges | "Keep": Go's behaviour
  IndexMode,    \* "Delete": index entries go with the document | "Keep": they stay behind
  CountMode,    \* "Distinct": count distinct found documents at commit | "Requests": every DocID named | "Attempt": every attempt
  RetryMode,    \* "Retry": a conflicted attempt runs again | "Fail": it fails the purge
  ScanMode      \* "Validated": the owner scan that decides a delete is validated | "Plain": it is not

\* Each switch takes one of its values.
ASSUME Branchable \in BOOLEAN
ASSUME HistoryMode \in {"ByCollection", "Always"}
ASSUME MaxActs \in Nat
ASSUME OwnerMode \in {"Checked", "Blind"}
ASSUME EdgeMode \in {"Drop", "Keep"}
ASSUME IndexMode \in {"Delete", "Keep"}
ASSUME CountMode \in {"Distinct", "Requests", "Attempt"}
ASSUME RetryMode \in {"Retry", "Fail"}
ASSUME ScanMode \in {"Validated", "Plain"}

\* Whether this purge deletes the history: always outside a branchable
\* collection; inside one only with the "Always" switch, the planted bug.
Prune == ~Branchable \/ HistoryMode = "Always"

\* The documents: a and b get purged, c stays, x was never stored.
Docs == {"a", "b", "c"}
\* What the operator asks for, in order: a twice and a DocID with no document.
Request == <<"a", "x", "b", "a">>
\* The DocIDs the request names, once each.
Named == {Request[i] : i \in 1..Len(Request)}

\* The blocks. a1 is a's head and a0 its first version; f1 is a field block
\* a and c share (same value), f3 one a and b share, f2 a's alone.
Blocks == {"a1", "a0", "f1", "f2", "f3", "b1", "c1"}
\* The field blocks a merging document can find already stored and adopt.
Fields == {"f1", "f2", "f3"}
\* The links each block holds: a composite links its parent and field blocks.
Links(blk) ==
  CASE blk = "a1" -> {"a0", "f2"}
    [] blk = "a0" -> {"f1", "f3"}
    [] blk = "b1" -> {"f3"}
    [] blk = "c1" -> {"f1"}
    [] OTHER      -> {}
\* Each document's head blocks.
Heads(d) == CASE d = "a" -> {"a1"} [] d = "b" -> {"b1"} [] d = "c" -> {"c1"} [] OTHER -> {}

\* One round of following links from a set of blocks.
Step(s) == s \cup UNION {Links(blk) : blk \in s}
\* Every block reachable from the roots (the DAG is at most three deep).
Reach(roots) == Step(Step(Step(roots)))
\* The blocks a set of documents reaches from its heads.
DocReach(ds) == Reach(UNION {Heads(d) : d \in ds})
\* The document composites: every version of every document.
Composites == {"a1", "a0", "b1", "c1"}
\* The composites a branchable collection's DAG links: one collection block
\* per document commit, so all of them. A purge leaves the collection DAG.
CollectionLinked == IF Branchable THEN Composites ELSE {}
\* Every block the collection DAG reaches through the composites it links.
CollectionReach == Reach(CollectionLinked)

VARIABLES
  live,      \* documents with a value, a primary key and DocID aliases
  indexed,   \* documents with secondary index entries
  present,   \* blocks the blockstore holds
  owners,    \* for each block, the documents its ownership records name
  count,     \* the purge count reported once a purge commits
  phase,     \* "idle" (an attempt can start), "running", "done" or "failed"
  dirty,     \* blocks a sync stored since the running attempt began
  touched,   \* blocks whose ownership records changed since the attempt began
  snap,      \* the ownership records as the running attempt's snapshot saw them
  adopted,   \* blocks the merged document n owns
  acts       \* how many times a sync or a merge acted

\* Every variable, for UNCHANGED.
vars == <<live, indexed, present, owners, count, phase, dirty, touched, snap, adopted, acts>>

\* The ownership records at the start: each block is owned by every
\* document whose history reaches it.
Owned == [blk \in Blocks |-> {d \in Docs : blk \in DocReach({d})}]

\* The start: every document is stored and indexed, every block is held.
Init ==
  /\ live = Docs
  /\ indexed = Docs
  /\ present = Blocks
  /\ owners = Owned
  /\ count = 0
  /\ phase = "idle"
  /\ dirty = {}
  /\ touched = {}
  /\ snap = Owned
  /\ adopted = {}
  /\ acts = 0

\* An attempt begins: it reads from a snapshot taken now.
Begin ==
  /\ phase = "idle"
  /\ phase' = "running"
  /\ dirty' = {}
  /\ touched' = {}
  /\ snap' = owners
  /\ UNCHANGED <<live, indexed, present, owners, count, adopted, acts>>

\* A sync stores a block again (a content-addressed put). The bytes come back
\* and nothing about ownership changes; a running attempt has it as dirty.
Restore(blk) ==
  /\ phase \in {"idle", "running"}
  /\ acts < MaxActs
  /\ present' = present \cup {blk}
  /\ dirty' = IF phase = "running" THEN dirty \cup {blk} ELSE dirty
  /\ acts' = acts + 1
  /\ UNCHANGED <<live, indexed, owners, count, phase, touched, snap, adopted>>

\* A merge of the new document n finds a field block already stored and
\* adopts it: it writes only the owner edge (a blind put), not the block.
Adopt(blk) ==
  /\ phase \in {"idle", "running"}
  /\ acts < MaxActs
  /\ blk \in present
  /\ owners' = [owners EXCEPT ![blk] = @ \cup {"n"}]
  /\ adopted' = adopted \cup {blk}
  /\ touched' = IF phase = "running" THEN touched \cup {blk} ELSE touched
  /\ acts' = acts + 1
  /\ UNCHANGED <<live, indexed, present, count, phase, dirty, snap>>

\* The documents this attempt finds: those the request names that exist.
Found == Named \cap live
\* The blocks the attempt walks: the found documents' histories.
Walked == DocReach(Found)
\* The records in `own` with the found documents dropped from walked blocks.
Without(own) == [blk \in Blocks |-> IF blk \in Walked THEN own[blk] \ Found ELSE own[blk]]
\* The records after the commit. The edge deletes are blind, so they apply to
\* the records as they stand at commit. Go keeps them when it keeps the
\* blocks; the Keep switch reproduces that.
NewOwners == IF ~Prune /\ EdgeMode = "Keep" THEN owners ELSE Without(owners)
\* The blocks a prune deletes: walked blocks nobody else owns in the
\* attempt's snapshot, or, with the Blind switch, every walked block. A purge
\* that keeps the history deletes none.
Doomed ==
  IF ~Prune THEN {}
  ELSE IF OwnerMode = "Blind" THEN Walked
  ELSE {blk \in Walked : Without(snap)[blk] = {}}
\* A validated owner scan of a walked block saw a record change: the commit
\* must conflict.
ScanBroken == Prune /\ ScanMode = "Validated" /\ touched \cap Walked # {}
\* A delete of a block a sync stored meanwhile is validated like a delete of
\* an ordinary key, so the commit may conflict on it.
DeleteRaced == dirty \cap Doomed # {}
\* What the count grows by: each found document once, or, with the Requests
\* switch, every request entry that names a stored document.
Counted ==
  IF CountMode = "Requests"
    THEN Cardinality({i \in 1..Len(Request) : Request[i] \in live})
    ELSE Cardinality(Found)

\* The attempt commits or conflicts.
Commit ==
  /\ phase = "running"
  /\ \/ \* The commit lands, unless a validated scan changed: every effect
        \* of the attempt applies at once.
        /\ ~ScanBroken
        /\ live' = live \ Found
        /\ indexed' = IF IndexMode = "Delete" THEN indexed \ Found ELSE indexed
        /\ present' = present \ Doomed
        /\ owners' = NewOwners
        /\ count' = count + Counted
        /\ phase' = "done"
     \/ \* The commit conflicts: nothing applies. The attempt runs again, or,
        \* with the Fail switch, the purge fails.
        /\ ScanBroken \/ DeleteRaced
        /\ phase' = IF RetryMode = "Retry" THEN "idle" ELSE "failed"
        /\ count' = IF CountMode = "Attempt" THEN count + Counted ELSE count
        /\ UNCHANGED <<live, indexed, present, owners>>
  /\ dirty' = {}
  /\ touched' = {}
  /\ UNCHANGED <<snap, adopted, acts>>

\* Any actor may move next.
Next ==
  \/ Begin
  \/ Commit
  \/ \E blk \in Blocks : Restore(blk)
  \/ \E blk \in Fields : Adopt(blk)

\* The run, with the purge given its turn whenever it can take it.
Spec == Init /\ [][Next]_vars /\ WF_vars(Begin \/ Commit)

\* Invariant: every block an unpurged document reaches, or the merged
\* document adopted, is still held.
INV_OthersIntact == (DocReach(Docs \ Named) \cup adopted) \subseteq present

\* Invariant: in a @branchable collection every block of the history, which
\* the collection DAG reaches, is still held, so the collection's `_commits`
\* stays whole and a peer syncing it finds them.
INV_CollectionIntact == CollectionReach \subseteq present

\* Invariant: a committed purge leaves no value, index entry or alias.
INV_Gone == phase = "done" => (Named \cap (live \cup indexed)) = {}

\* Invariant: once committed, no block names a purged document as owner.
INV_NoStaleOwner == phase = "done" => \A blk \in Blocks : owners[blk] \cap Named = {}

\* Invariant: outside a @branchable collection, a committed purge leaves no
\* block that only purged documents reached.
INV_Pruned ==
  (phase = "done" /\ ~Branchable) =>
    (DocReach(Named \cap Docs) \ (DocReach(Docs \ Named) \cup adopted)) \cap present = {}

\* Invariant: the count is the number of distinct stored documents named,
\* and nothing is counted before the purge commits.
INV_Count ==
  /\ phase = "done" => count = Cardinality(Named \cap Docs)
  /\ phase # "done" => count = 0

\* Property: the purge always commits, however often a sync or a merge acts.
PROP_Commits == <>(phase = "done")
====
