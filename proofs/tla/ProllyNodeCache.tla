---------------------------- MODULE ProllyNodeCache ----------------------------
\* The story, in plain words.
\*
\* Who acts: one worker building and reading a prolly tree whose nodes live in
\* regolith (`ProllyNodes` in crates/engine/storage/src/stores/prolly_nodes.rs),
\* behind a cache with a byte budget (`Cached` in
\* crates/foundation/prolly/src/cache.rs).
\*
\* What can happen: the worker puts a node, which waits in memory (pending).
\* Publishing a root writes every pending node and the root in one regolith
\* transaction (`ProllyNodes::publish`), which commits or fails; nothing else
\* writes a node. The worker reads any node it put, and the process can
\* crash, losing everything in memory. A read that misses the
\* cache fills it, and the cache evicts nodes once it is over budget.
\*
\* What must never happen: a read misses a node that was put and not lost in
\* a crash (GetFinds); the cache holds more than its budget (CacheBounded); a
\* node too big for the budget pushes the others out (HugeEvictsNothing); a
\* published root names a node regolith does not hold (PublishedStored); a
\* build finishes with a tree that misses a chunk whose put failed (BuildWhole).
\*
\* What this model trusts. A node is named by the hash of its bytes, so the
\* node under a name never changes: a cached or pending copy is never stale,
\* and the model does not track node contents at all. The worker owns the
\* store (`&mut self` in Rust), so a publish and a read never overlap.
EXTENDS Naturals, FiniteSets

\* The guards. Each RED configuration turns one off.
CONSTANTS
  ReadPending,      \* a read looks in the pending nodes before regolith
  Evict,            \* the cache evicts once over budget
  SkipHuge,         \* a node larger than the budget is never cached
  KeepOnFail,       \* a failed write leaves the nodes pending
  NodesWithRoot,    \* the transaction that publishes a root writes every pending node
  RefuseAfterFail   \* a build whose put failed refuses to finish

\* The nodes: a, b and c are one byte each, h is three bytes; the budget is two.
Nodes == {"a", "b", "c", "h"}
\* How many bytes each node's encoding takes.
Size(n) == IF n = "h" THEN 3 ELSE 1
\* The cache's byte budget.
Budget == 2

\* The bytes a set of nodes takes, summed one node at a time.
RECURSIVE Bytes(_)
Bytes(S) == IF S = {} THEN 0 ELSE LET n == CHOOSE m \in S : TRUE IN Size(n) + Bytes(S \ {n})

VARIABLES
  put,        \* nodes put and not lost in a crash
  pending,    \* nodes put and not yet written to regolith
  stored,     \* nodes regolith holds, durably
  cache,      \* nodes the cache holds
  published,  \* nodes published as a root
  found,      \* whether the last read found its node (TRUE before any read)
  dropped,    \* the nodes the last read's cache fill evicted
  lastRead,   \* the node the last read asked for ("none" before any read)
  lostRef,    \* a put in the running build failed, so the reference it was
              \* to hand the level above is gone (`Builder::feed`)
  buildFailed, \* the builder remembers that failure (`Builder::failed`)
  finished    \* how the build ended: "no" (still running), "whole" or "short"

\* The build's variables, which only the build steps and a crash change.
buildVars == <<lostRef, buildFailed, finished>>

vars == <<put, pending, stored, cache, published, found, dropped, lastRead, lostRef, buildFailed, finished>>

\* Every variable holds what its comment says.
TypeOK ==
  /\ put \subseteq Nodes /\ pending \subseteq Nodes /\ stored \subseteq Nodes
  /\ cache \subseteq Nodes /\ published \subseteq Nodes
  /\ found \in BOOLEAN /\ dropped \subseteq Nodes /\ lastRead \in Nodes \cup {"none"}
  /\ lostRef \in BOOLEAN /\ buildFailed \in BOOLEAN /\ finished \in {"no", "whole", "short"}

\* Nothing put, stored or cached yet.
Init ==
  /\ put = {} /\ pending = {} /\ stored = {} /\ cache = {} /\ published = {}
  /\ found = TRUE /\ dropped = {} /\ lastRead = "none"
  \* A build is running and nothing has failed in it.
  /\ lostRef = FALSE /\ buildFailed = FALSE /\ finished = "no"

\* The worker puts node n: it waits, pending (`ProllyNodes::put`).
Put(n) ==
  /\ n \notin put
  /\ put' = put \cup {n}
  /\ pending' = pending \cup {n}
  /\ UNCHANGED <<stored, cache, published, found, dropped, lastRead>>
  /\ UNCHANGED buildVars

\* The transaction that publishes node n as a root commits
\* (`ProllyNodes::publish`): with NodesWithRoot it stores every pending node
\* in the same commit, and only now is the pending set cleared. Without the
\* guard the root goes out alone and its nodes stay in memory.
Publish(n) ==
  \* The root is a node the worker put.
  /\ n \in put
  \* The root key now names it.
  /\ published' = published \cup {n}
  \* The pending nodes land in the same commit, or, without the guard, wait on.
  /\ stored' = IF NodesWithRoot THEN stored \cup pending ELSE stored
  \* Committed nodes are no longer pending.
  /\ pending' = IF NodesWithRoot THEN {} ELSE pending
  \* Nothing else changes.
  /\ UNCHANGED <<put, cache, found, dropped, lastRead>>
  /\ UNCHANGED buildVars

\* The publishing transaction fails, or the publish is dropped before it
\* commits: nothing is stored and no root goes out. The nodes stay pending, so
\* they can still be read and the next publish writes them. The attempt shares
\* the pending set instead of taking it, so a dropped publish has nothing to
\* put back.
PublishFails ==
  \* There is something it would have written.
  /\ pending # {}
  \* Without KeepOnFail the nodes taken for the write are dropped.
  /\ pending' = IF KeepOnFail THEN pending ELSE {}
  \* Nothing else changes.
  /\ UNCHANGED <<put, stored, cache, published, found, dropped, lastRead>>
  /\ UNCHANGED buildVars

\* Where a read finds node n: the cache, the pending nodes, or regolith.
Finds(n) == n \in cache \/ (ReadPending /\ n \in pending) \/ n \in stored

\* What the cache may hold after filling with n: everything, when it fits or
\* nothing evicts; otherwise any part of it that fits the budget.
Fills(n) ==
  LET full == cache \cup {n} IN
  IF Bytes(full) <= Budget \/ ~Evict THEN {full}
  ELSE {S \in SUBSET full : Bytes(S) <= Budget}

\* The worker reads node n, a node it put (`Cached::get`). A hit changes
\* nothing in the cache; a miss that finds the node fills the cache with it,
\* unless it is too big for the budget.
Read(n) ==
  /\ n \in put
  /\ found' = Finds(n)
  /\ lastRead' = n
  /\ IF n \in cache \/ ~Finds(n) \/ (SkipHuge /\ Size(n) > Budget)
       THEN /\ cache' = cache /\ dropped' = {}
       ELSE \E S \in Fills(n) : /\ cache' = S /\ dropped' = cache \ S
  /\ UNCHANGED <<put, pending, stored, published>>
  /\ UNCHANGED buildVars

\* The process crashes: memory is lost, regolith is not. Nodes never stored
\* were never published by a correct worker, so they are simply gone.
Crash ==
  /\ put' = stored
  /\ pending' = {}
  /\ cache' = {}
  /\ found' = TRUE
  /\ dropped' = {}
  /\ lastRead' = "none"
  \* The builder was in memory too: the build starts over, nothing failed.
  /\ lostRef' = FALSE /\ buildFailed' = FALSE /\ finished' = "no"
  /\ UNCHANGED <<stored, published>>

\* A put inside the running build returns an error (a store whose put
\* fails), or the push is dropped while the put waits. The
\* node may still be pending, but the reference `child` was to hand the level
\* above is gone.
BuildPutFails ==
  /\ finished = "no"
  /\ ~buildFailed
  /\ lostRef' = TRUE
  \* With RefuseAfterFail the builder remembers the failure: `push` marks it
  \* failed before it waits on the put and clears that only once the put returns.
  /\ buildFailed' = RefuseAfterFail
  /\ UNCHANGED <<finished, put, pending, stored, cache, published, found, dropped, lastRead>>

\* The build finishes (`Builder::finish`). A builder that remembers a failure
\* returns `BuildFailed` instead, so this step cannot run; otherwise the tree
\* is whole only if no reference was lost.
FinishBuild ==
  /\ finished = "no"
  /\ ~buildFailed
  /\ finished' = IF lostRef THEN "short" ELSE "whole"
  /\ UNCHANGED <<lostRef, buildFailed, put, pending, stored, cache, published, found, dropped, lastRead>>

\* Any of the steps above, in any order.
Next ==
  \/ \E n \in Nodes : Put(n) \/ Publish(n) \/ Read(n)
  \/ PublishFails \/ Crash \/ BuildPutFails \/ FinishBuild

Spec == Init /\ [][Next]_vars

\* A read of a node that was put always finds it.
GetFinds == found

\* The cache never holds more bytes than its budget.
CacheBounded == Bytes(cache) <= Budget

\* Reading a node too big to cache never evicts another node.
HugeEvictsNothing == (lastRead # "none" /\ Size(lastRead) > Budget) => dropped = {}

\* Every published root is a node regolith holds, crash or not.
PublishedStored == published \subseteq stored

\* A finished build's tree holds every entry pushed: it never ends short.
BuildWhole == finished # "short"
================================================================================
