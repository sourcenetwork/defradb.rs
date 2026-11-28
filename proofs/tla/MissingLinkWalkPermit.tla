------------------------- MODULE MissingLinkWalkPermit -------------------------
\* The story, in plain words.
\*
\* A clock-dispatched DAG fetch starts with its source peer's fetch permit
\* already reserved. Before it asks the network for anything, it walks the
\* blocks it already holds to learn what is missing. That walk is local work
\* and can be slow. While it runs, the peer permit must be free so another
\* root from the same peer can begin its own local work. When the walk ends,
\* the network attempt takes the peer and global permits only if both are free.
\* If the peer is still busy, the root gives back its pending-DAG slot and the
\* retry clock brings it back later instead of holding the slot while it waits.
\*
\* What must never happen: a root holds its peer permit while walking local
\* blocks; another root cannot take that peer's free permit during the walk;
\* or a clock-dispatched root keeps its pending-DAG slot while the peer permit
\* is unavailable.
\*
\* Rust: crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag;
\* crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs
\* a_local_walk_releases_the_peer_reservation_while_it_reads.
EXTENDS Naturals, FiniteSets

\* Bug names the guard a RED configuration breaks; "none" keeps it.
CONSTANT Bug

Roots == {"first", "second"}

VARIABLES
  phase, \* first's phase: reserved, walking, waiting, network, or done
  held,  \* the root holding the peer's only permit, or "none"
  slot   \* the root holding the pending-DAG slot, or "none"

vars == <<phase, held, slot>>

\* The first root is dispatched with its permit reserved.
Init == phase = "reserved" /\ held = "first" /\ slot = "first"

\* Start the local walk. The reservation is returned first; the broken path
\* keeps it while reading.
StartWalk ==
  /\ phase = "reserved"
  /\ phase' = "walking"
  /\ held' = IF Bug = "holdDuringWalk" THEN held ELSE "none"
  /\ UNCHANGED slot

\* Another root from the same peer takes the free permit while the walk runs.
OtherAcquires ==
  /\ phase = "walking"
  /\ held = "none"
  /\ held' = "second"
  /\ UNCHANGED <<phase, slot>>

\* The other root finishes its local work and returns the permit.
OtherReleases ==
  /\ held = "second"
  /\ held' = "none"
  /\ UNCHANGED <<phase, slot>>

\* The first root finishes its local walk. With the peer free it takes the
\* permit and keeps its slot for the network attempt. With the peer busy it
\* gives the slot back and the retry clock owns what happens next. The broken
\* waitAfterWalk path keeps the slot and waits for the peer.
FinishWalk ==
  /\ phase = "walking"
  /\ CASE held = "none" -> /\ phase' = "network"
                           /\ held' = "first"
                           /\ UNCHANGED slot
        [] held = "second" /\ Bug = "waitAfterWalk" -> /\ phase' = "waiting"
                                                       /\ UNCHANGED <<held, slot>>
        [] OTHER -> /\ phase' = "done"
                    /\ slot' = "none"
                    /\ UNCHANGED held

\* A network attempt releases the peer permit and the pending-DAG slot.
FinishNetwork ==
  /\ phase = "network"
  /\ phase' = "done"
  /\ held' = "none"
  /\ slot' = "none"

Next == StartWalk \/ OtherAcquires \/ OtherReleases \/ FinishWalk \/ FinishNetwork

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

TypeOK == phase \in {"reserved", "walking", "waiting", "network", "done"}
          /\ held \in Roots \cup {"none"}
          /\ slot \in Roots \cup {"none"}

\* Local traversal never holds the source peer's permit.
INV_NoPermitWhileWalking == phase = "walking" => held # "first"

\* A root waiting for a peer permit never keeps the pending-DAG slot.
INV_NoSlotWhileWaiting == phase = "waiting" => slot # "first"

===============================================================================
