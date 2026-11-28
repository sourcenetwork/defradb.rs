------------------------- MODULE MissingLinkWalkPermit -------------------------
\* The story, in plain words.
\*
\* A clock-dispatched DAG fetch starts with its source peer's fetch permit
\* already reserved. Before it asks the network for anything, it walks the
\* blocks it already holds to learn what is missing. That walk is local work
\* and can be slow. While it runs, the peer permit must be free so another
\* root from the same peer can begin its own local work. The permit is taken
\* again only for the network attempt.
\*
\* What must never happen: a root holds its peer permit while walking local
\* blocks, or another root cannot take that peer's free permit during the
\* walk.
\*
\* Rust: crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag;
\* crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs
\* a_local_walk_releases_the_peer_reservation_while_it_reads.
EXTENDS Naturals, FiniteSets

\* Bug names the guard a RED configuration breaks; "none" keeps it.
CONSTANT Bug

Roots == {"first", "second"}

VARIABLES
  phase, \* first's phase: reserved, walking, or done
  held   \* the root holding the peer's only permit, or "none"

vars == <<phase, held>>

\* The first root is dispatched with its permit reserved.
Init == phase = "reserved" /\ held = "first"

\* Start the local walk. The reservation is returned first; the broken path
\* keeps it while reading.
StartWalk ==
  /\ phase = "reserved"
  /\ phase' = "walking"
  /\ held' = IF Bug = "holdDuringWalk" THEN held ELSE "none"

\* Another root from the same peer takes the free permit while the walk runs.
OtherAcquires ==
  /\ phase = "walking"
  /\ held = "none"
  /\ held' = "second"
  /\ UNCHANGED phase

\* The other root finishes its local work and returns the permit.
OtherReleases ==
  /\ held = "second"
  /\ held' = "none"
  /\ UNCHANGED phase

\* The first root finishes its local walk. The model ends there: the network
\* attempt that takes the permit again is covered by MissingLinkWalk.tla.
FinishWalk ==
  /\ phase = "walking"
  /\ phase' = "done"
  /\ UNCHANGED held

Next == StartWalk \/ OtherAcquires \/ OtherReleases \/ FinishWalk

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

TypeOK == phase \in {"reserved", "walking", "done"} /\ held \in Roots \cup {"none"}

\* Local traversal never holds the source peer's permit.
INV_NoPermitWhileWalking == phase = "walking" => held # "first"

===============================================================================
