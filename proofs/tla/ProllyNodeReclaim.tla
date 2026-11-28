---------------------------- MODULE ProllyNodeReclaim ----------------------------
\* The story, in plain words.
\*
\* Who acts: a reclamation worker (`ProllyNodes::reclaim`, `node_keys`,
\* `ReclaimNodes::run` and `mark_roots` in
\* crates/engine/storage/src/stores/prolly_nodes.rs), which `reclaim_every`
\* in crates/engine/storage/src/stores/prolly_nodes/schedule.rs runs one page
\* per period, and one writer of a prolly tree, whose root the worker reads
\* through `LiveRoots::read`.
\*
\* What can happen: the worker scans a page of stored node keys. Then, in one
\* transaction, it reads the live root, marks every node that root names, and
\* deletes the page's unmarked keys. Meanwhile the writer edits the tree: the
\* edit's puts wait in memory (`ProllyNodes::put`; `collapse` in
\* crates/foundation/prolly/src/tree.rs no longer writes them), and then one
\* transaction writes those nodes and the new root together
\* (`ProllyNodes::publish`, `PublishNodes::run`). One of the new root's nodes
\* is an old one that came back: it already sits in the store, named by no
\* root, so the page holds it as something to delete.
\*
\* What must never happen: a node the live root names is missing from the
\* store (SweepKeepsLiveRoots).
\*
\* What this model trusts. `LiveRoots::read` returns the root of every live
\* tree and reads it with a validated read, and regolith refuses the commit of
\* a transaction whose validated read changed. A node write is blind: it
\* conflicts with nothing and the later commit wins. The writer's puts that
\* wait in memory are not in the store, so the model leaves them out until
\* the publish commits them. The PutAlone configuration is the writer this
\* replaced: a tree edit that flushed its nodes in a transaction of their own
\* before the root was published.
EXTENDS Naturals, FiniteSets

\* The guards. Each RED configuration turns one off.
CONSTANTS
  RootsInTxn,        \* the delete transaction reads the live root itself
  ValidateRoots,     \* a root that changed before the commit re-runs the delete
  PublishWithNodes,  \* the root's nodes commit in the transaction that publishes it (`ProllyNodes::publish`)
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
  pending,   \* the new root's nodes the writer put, waiting in its memory
  written,   \* the new root's nodes the writer committed on their own
  phase,     \* the worker: "idle", "scanned" (has a page) or "running" (in its transaction)
  page,      \* the keys the page scan found
  argRoots,  \* the roots as they were when the page was scanned
  txnRoots   \* the roots the running transaction read and marks from

\* Every variable, for the steps that leave some unchanged.
vars == <<stored, liveRoots, pending, written, phase, page, argRoots, txnRoots>>

\* Each variable holds the kind of value it should.
TypeOK ==
  \* The store holds node names.
  /\ stored \subseteq Nodes
  \* The live roots are roots.
  /\ liveRoots \subseteq Roots
  \* The writer only writes the new root's nodes.
  /\ written \subseteq Wrote
  \* The writer only puts the new root's nodes.
  /\ pending \subseteq Wrote
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
  \* The writer has put nothing.
  /\ pending = {}
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
\* of the new root in a transaction of its own, before the root, as the old
\* flush at the end of every tree edit did.
PutAlone(n) ==
  \* Only when the guard is off.
  /\ ~PublishWithNodes
  \* The new root is not published yet.
  /\ liveRoots = {"old"}
  \* It is a node the edit put, not yet written.
  /\ n \in pending \ written
  \* The blind put lands: the store holds the node, whether it did or not.
  /\ stored' = stored \cup {n}
  \* The writer remembers it wrote it.
  /\ written' = written \cup {n}
  \* Nothing else changes.
  /\ UNCHANGED <<liveRoots, pending, phase, page, argRoots, txnRoots>>

\* The writer's edit puts one node of the new root (`ProllyNodes::put`): it
\* waits in memory, so the store and every page see nothing of it.
Put(n) ==
  \* The new root is not published yet.
  /\ liveRoots = {"old"}
  \* It is a node of the new root, not yet put.
  /\ n \in Wrote \ pending
  \* It waits in the writer's memory.
  /\ pending' = pending \cup {n}
  \* Nothing else changes: the store holds nothing new.
  /\ UNCHANGED <<stored, liveRoots, written, phase, page, argRoots, txnRoots>>

\* The writer publishes the new root (`ProllyNodes::publish`).
Publish ==
  \* Once: only while the old root is the live one.
  /\ liveRoots = {"old"}
  \* The edit is done: every node of the new root was put.
  /\ pending = Wrote
  \* With the guard, the same commit writes the new root's nodes.
  /\ IF PublishWithNodes
       \* The puts that waited in memory and the root land together.
       THEN stored' = stored \cup pending
       \* Without it, the nodes were written before, and may be gone by now.
       ELSE written = Wrote /\ stored' = stored
  \* The root key now names the new root.
  /\ liveRoots' = {"new"}
  \* Nothing else changes.
  /\ UNCHANGED <<pending, written, phase, page, argRoots, txnRoots>>

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
  /\ UNCHANGED <<stored, liveRoots, pending, written, txnRoots>>

\* The delete transaction starts, or starts again (`ReclaimNodes::run`).
Begin ==
  \* It has a page and no transaction running.
  /\ phase = "scanned"
  \* With the guard it reads the roots now; without it, it uses the old list.
  /\ txnRoots' = IF RootsInTxn THEN liveRoots ELSE argRoots
  \* The transaction is running.
  /\ phase' = "running"
  \* Nothing else changes.
  /\ UNCHANGED <<stored, liveRoots, pending, written, page, argRoots>>

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
  /\ UNCHANGED <<liveRoots, pending, written, page, argRoots, txnRoots>>

\* One step of the writer or of the worker.
Next ==
  \* The writer's edit puts a node in memory.
  \/ \E n \in Nodes : Put(n)
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
