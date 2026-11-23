---------------------------- MODULE ProllyNodeReclaim ----------------------------
\* The story, in plain words.
\
\* Who acts: a reclamation worker and one writer of prolly-tree roots
\* (`ProllyNodes::reclaim` in
\* crates/engine/storage/src/stores/prolly_nodes.rs).
\
\* What can happen: the worker scans a page of stored node keys, marks every
\* node the live roots name, and deletes unmarked keys. The writer can publish
\* a new root between the scan and the delete.
\
\* What must never happen: a node named by a live root is deleted.
\
\* What this model trusts. The roots given to the sweep name every live tree,
\* a root is published only after every node it names is stored, and a failed
\* delete retries the same transaction rather than publishing a partial one.

EXTENDS Naturals, FiniteSets

\* The guards. The RED configuration turns rechecking off.
CONSTANTS
  RecheckRoots,      \* the delete transaction marks the roots again
  OnlyDeleteUnmarked \* a delete skips every marked node

\* Nodes r is a root; i and j are its children; k is an old node no live root
\* names.
Nodes == {"r", "i", "j", "k"}
Roots == {"r"}
Children == [n \in Nodes |-> IF n = "r" THEN {"i", "j"} ELSE {}]

\* Every node reachable from `frontier`, including the frontier.
RECURSIVE Closure(_)
Closure(frontier) ==
  IF frontier = {} THEN {}
  ELSE LET n == CHOOSE m \in frontier : TRUE
       IN {n} \cup Closure((frontier \ {n}) \cup Children[n])

VARIABLES
  stored,    \* node keys regolith holds
  liveRoots, \* roots the caller says are live
  candidates,\* keys the page scan nominated
  marked,    \* keys the current marks reach
  deleted,   \* keys this sweep removed
  phase      \* "scan", "delete" or "done"

vars == <<stored, liveRoots, candidates, marked, deleted, phase>>

TypeOK ==
  /\ stored \subseteq Nodes
  /\ liveRoots \subseteq Roots
  /\ candidates \subseteq Nodes
  /\ marked \subseteq Nodes
  /\ deleted \subseteq Nodes
  /\ phase \in {"scan", "delete", "done"}

Init ==
  /\ stored = {"r", "i", "j", "k"}
  /\ liveRoots = Roots
  /\ candidates = {}
  /\ marked = {}
  /\ deleted = {}
  /\ phase = "scan"

\* The writer publishes a root only after its nodes are stored.
Publish(root, named) ==
  /\ root \in Roots
  /\ named \subseteq Nodes
  /\ named \cup {root} \subseteq stored
  /\ liveRoots' = liveRoots \cup {root}
  /\ UNCHANGED <<stored, candidates, marked, deleted, phase>>

\* The worker scans one page and marks the live roots it knows.
Scan ==
  /\ phase = "scan"
  /\ candidates' \subseteq stored
  /\ marked' = Closure(liveRoots)
  /\ phase' = "delete"
  /\ UNCHANGED <<stored, liveRoots, deleted>>

\* The delete transaction marks the roots again before it decides.
Recheck == marked' = Closure(liveRoots)

\* The worker deletes one candidate that the current marks do not name.
Delete(n) ==
  /\ phase = "delete"
  /\ n \in candidates
  /\ RecheckRoots => Recheck
  /\ OnlyDeleteUnmarked => n \notin marked'
  /\ stored' = stored \ {n}
  /\ deleted' = deleted \cup {n}
  /\ candidates' = candidates \ {n}
  /\ UNCHANGED <<liveRoots, marked, phase>>

\* The page ends when every candidate has been decided.
Finish ==
  /\ phase = "delete"
  /\ candidates = {}
  /\ phase' = "done"
  /\ UNCHANGED <<stored, liveRoots, marked, deleted>>

Next ==
  \/ \E root \in Roots : \E named \in SUBSET Nodes : Publish(root, named)
  \/ Scan
  \/ \E n \in Nodes : Delete(n)
  \/ Finish

Spec == Init /\ [][Next]_vars

\* Every node a live root names is still stored.
SweepKeepsLiveRoots == Closure(liveRoots) \subseteq stored

TypeHolds == TypeOK
================================================================================
