---------------------------- MODULE ProllyNodeReclaim ----------------------------
\* The story, in plain words.
\*
\* Who acts: a reclamation worker (`ProllyNodes::reclaim`, `node_keys`,
\* `ReclaimNodes::run` and `mark_roots` in
\* crates/engine/storage/src/stores/prolly_nodes.rs) and one writer of a
\* prolly tree, whose root the worker reads through `LiveRoots::read`.
\*
\* What can happen: the worker scans a page of stored node keys. Then, in one
\* transaction, it reads the live root, marks every node that root names, and
\* deletes the page's unmarked keys. Meanwhile the writer edits the tree: it
\* writes the nodes of a new root and publishes that root. One of the new
\* root's nodes is an old one that came back: it already sits in the store,
\* named by no root, so the page holds it as something to delete.
\*
\* What must never happen: a node the live root names is missing from the
\* store (SweepKeepsLiveRoots).
\*
\* What this model trusts. `LiveRoots::read` returns the root of every live
\* tree and reads it with a validated read, and regolith refuses the commit of
\* a transaction whose validated read changed. A node write is blind: it
\* conflicts with nothing and the later commit wins. Nothing in the workspace
\* publishes a root yet, so the writer's rule (PublishWithNodes) is a contract
\* on the caller that will store the roots, stated on `ProllyNodes::reclaim`.
\* A caller whose writer commits nodes before their root (as a tree edit's
\* flush does) is the PutAlone configuration: it must not run a page between
\* an edit and the publication of its root.
EXTENDS Naturals, FiniteSets

\* The guards. Each RED configuration turns one off.
CONSTANTS
  RootsInTxn,        \* the delete transaction reads the live root itself
  ValidateRoots,     \* a root that changed before the commit re-runs the delete
  PublishWithNodes,  \* a root is published in the transaction that writes its nodes
  OnlyDeleteUnmarked \* a delete skips every marked node

\* The nodes. "old" is the root live at the start and "new" is the root the
\* writer publishes. Both name "kept". Only the old root names "gone". Only
\* the new root names "back", the old node that comes back.
Nodes == {"old", "new", "kept", "gone", "back"}
\* The two roots.
Roots == {"old", "new"}
\* The nodes each node names directly: a root names two children, a child none.
Children ==
  [n \in Nodes |->
    \* The old root names the shared child and the one that goes away.
    IF n = "old" THEN {"kept", "gone"}
    \* The new root names the shared child and the one that comes back.
    ELSE IF n = "new" THEN {"kept", "back"}
    \* A child is a leaf: it names nothing.
    ELSE {}]
\* The nodes the writer puts for the new root. It does not put "kept": the
\* edit did not touch it, and the old root already names it.
Wrote == {"new", "back"}

\* Every node reachable from `frontier`, the frontier included.
RECURSIVE Closure(_)
Closure(frontier) ==
  \* Nothing left to visit: nothing more is reachable.
  IF frontier = {} THEN {}
  \* Otherwise take one node, keep it, and visit the rest and its children.
  ELSE LET n == CHOOSE m \in frontier : TRUE
       IN {n} \cup Closure((frontier \ {n}) \cup Children[n])

VARIABLES
  stored,    \* the node keys regolith holds
  liveRoots, \* the roots published, as the root keys hold them
  written,   \* the new root's nodes the writer committed on their own
  phase,     \* the worker: "idle", "scanned" (has a page) or "running" (in its transaction)
  page,      \* the keys the page scan found
  argRoots,  \* the roots as they were when the page was scanned
  txnRoots   \* the roots the running transaction read and marks from

\* Every variable, for the steps that leave some unchanged.
vars == <<stored, liveRoots, written, phase, page, argRoots, txnRoots>>

\* Each variable holds the kind of value it should.
TypeOK ==
  \* The store holds node names.
  /\ stored \subseteq Nodes
  \* The live roots are roots.
  /\ liveRoots \subseteq Roots
  \* The writer only writes the new root's nodes.
  /\ written \subseteq Wrote
  \* The worker is in one of its three steps.
  /\ phase \in {"idle", "scanned", "running"}
  \* A page is a set of node names.
  /\ page \subseteq Nodes
  \* The roots seen at the scan are roots.
  /\ argRoots \subseteq Roots
  \* The roots the transaction read are roots.
  /\ txnRoots \subseteq Roots

\* The start: the old tree is stored and live, "back" is a leftover no root
\* names, and the new root does not exist yet.
Init ==
  \* The old root, its two children, and the leftover.
  /\ stored = {"old", "kept", "gone", "back"}
  \* Only the old root is published.
  /\ liveRoots = {"old"}
  \* The writer has written nothing.
  /\ written = {}
  \* The worker has not started.
  /\ phase = "idle"
  \* It has no page.
  /\ page = {}
  \* It has seen no roots.
  /\ argRoots = {}
  \* Its transaction has read no roots.
  /\ txnRoots = {}

\* The planted bug of the PutAlone configuration: the writer commits one node
\* of the new root in a transaction of its own, before the root.
PutAlone(n) ==
  \* Only when the guard is off.
  /\ ~PublishWithNodes
  \* The new root is not published yet.
  /\ liveRoots = {"old"}
  \* It is a node of the new root, not yet written.
  /\ n \in Wrote \ written
  \* The blind put lands: the store holds the node, whether it did or not.
  /\ stored' = stored \cup {n}
  \* The writer remembers it wrote it.
  /\ written' = written \cup {n}
  \* Nothing else changes.
  /\ UNCHANGED <<liveRoots, phase, page, argRoots, txnRoots>>

\* The writer publishes the new root.
Publish ==
  \* Once: only while the old root is the live one.
  /\ liveRoots = {"old"}
  \* With the guard, the same commit writes the new root's nodes.
  /\ IF PublishWithNodes
       \* The puts and the root land together.
       THEN stored' = stored \cup Wrote
       \* Without it, the nodes were written before, and may be gone by now.
       ELSE written = Wrote /\ stored' = stored
  \* The root key now names the new root.
  /\ liveRoots' = {"new"}
  \* Nothing else changes.
  /\ UNCHANGED <<written, phase, page, argRoots, txnRoots>>

\* The worker scans one page of stored keys (`node_keys`).
Scan ==
  \* It is not in the middle of a page.
  /\ phase = "idle"
  \* The page is any part of what is stored: a page has a bounded size.
  /\ page' \in SUBSET stored
  \* The roots as they are now: what a caller would pass as a fixed list.
  /\ argRoots' = liveRoots
  \* It has a page to decide on.
  /\ phase' = "scanned"
  \* Nothing else changes.
  /\ UNCHANGED <<stored, liveRoots, written, txnRoots>>

\* The delete transaction starts, or starts again (`ReclaimNodes::run`).
Begin ==
  \* It has a page and no transaction running.
  /\ phase = "scanned"
  \* With the guard it reads the roots now; without it, it uses the old list.
  /\ txnRoots' = IF RootsInTxn THEN liveRoots ELSE argRoots
  \* The transaction is running.
  /\ phase' = "running"
  \* Nothing else changes.
  /\ UNCHANGED <<stored, liveRoots, written, page, argRoots>>

\* The keys the transaction deletes: the page without the marked nodes, or,
\* with the guard off, the whole page.
Doomed == IF OnlyDeleteUnmarked THEN page \ Closure(txnRoots) ELSE page

\* The roots the transaction read have changed since it read them.
RootsChanged == RootsInTxn /\ ValidateRoots /\ txnRoots # liveRoots

\* The delete transaction tries to commit.
Commit ==
  \* It is running.
  /\ phase = "running"
  \* A root was published since the read: regolith refuses the commit.
  /\ IF RootsChanged
       \* Nothing is deleted, and the transaction will run again.
       THEN phase' = "scanned" /\ UNCHANGED stored
       \* Otherwise the deletes land and the page is done.
       ELSE phase' = "idle" /\ stored' = stored \ Doomed
  \* Nothing else changes.
  /\ UNCHANGED <<liveRoots, written, page, argRoots, txnRoots>>

\* One step of the writer or of the worker.
Next ==
  \* The writer commits a node on its own (only with the guard off).
  \/ \E n \in Nodes : PutAlone(n)
  \* The writer publishes the new root.
  \/ Publish
  \* The worker scans a page.
  \/ Scan
  \* The worker starts its delete transaction.
  \/ Begin
  \* The worker's delete transaction commits or is refused.
  \/ Commit

\* The system starts at Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* Every node a live root names is stored.
SweepKeepsLiveRoots == Closure(liveRoots) \subseteq stored

\* Every variable keeps its kind of value.
TypeHolds == TypeOK
================================================================================
