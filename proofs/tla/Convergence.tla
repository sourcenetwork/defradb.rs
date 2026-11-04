---- MODULE Convergence ----
\* The story. One node, the creator, holds a document's whole DAG. Every
\* other node has to end up with every block merged, while connections drop
\* and come back and nodes restart. No node goes looking for heads it does
\* not know: on connect a node re-drives only its own pending records
\* (handle_peer_connected), and DocSync or BranchableSync run only when the
\* embedder asks. A head reaches a node only while someone owes it there:
\*   - the creator's durable retry marker for that node, which its retry
\*     sweep pushes while the two are connected and the node's
\*     acknowledgement clears;
\*   - the node's own durable pending record, written before it
\*     acknowledges, which its resync sweep brings back into memory after a
\*     restart and which the merge discharges.
\* A head the creator only published (gossip, no replicator) has no owner.
\*
\* What must happen: under eventual connectivity every node merges every
\* block (CONV_EventualConnectivity). It holds when the creator replicates to
\* every node (Replicated), and fails when nothing owes the heads
\* (MC_Conv_NoHeadRediscovery).
\*
\* Every variable is state the code holds, except the restart budget:
\*   have      blockstore contents (crates/blockstore)
\*   merged    blocks the merge handler applied (crates/db merge handler)
\*   owed      the creator's retry markers per node (peerstore.rs register_scope_marker; retry.rs run_retry_pass)
\*   pendDur   a node's durable pending records (pushlog.rs process_block_inner; pending_store)
\*   wanted    roots in a node's in-memory pending map (resync_persisted_pending_dags refills it)
\*   connected the transport's directed reachability
\* Fetch slots, permits, the map's bound and the scheduler are P2PDelivery.tla's;
\* here a node fetches every block it wants, so each fetch is one task's.
EXTENDS Naturals

CONSTANTS
  Nodes,            \* the nodes
  Blocks,           \* the document's blocks
  Parents,          \* [Blocks -> SUBSET Blocks]: each block's parents
  Heads,            \* the heads every node must merge, with their histories
  Creator,          \* the node that holds every block at the start
  InitialConnected, \* the directed pairs connected at the start
  Replicated,       \* BOOLEAN: the creator holds a retry marker per node for every head
  AllowRestart      \* BOOLEAN: each node may restart once

\* Every directed pair of nodes.
NodePairs == Nodes \X Nodes

\* The creator is a node.
ASSUME Creator \in Nodes
\* Heads are blocks.
ASSUME Heads \subseteq Blocks
\* Parents are blocks.
ASSUME \A b \in Blocks : Parents[b] \subseteq Blocks
\* The starting connections are pairs of nodes.
ASSUME InitialConnected \subseteq NodePairs
\* The two switches are booleans.
ASSUME Replicated \in BOOLEAN /\ AllowRestart \in BOOLEAN

\* Every ancestor of block b. Terminates because Parents is acyclic in every scenario.
RECURSIVE AncestorsOf(_)
AncestorsOf(b) == Parents[b] \cup UNION {AncestorsOf(p) : p \in Parents[b]} \* parents, and theirs

\* Every block some head's history needs.
RequiredBlocks == UNION {{h} \cup AncestorsOf(h) : h \in Heads}

VARIABLES
  have,       \* [Nodes -> SUBSET Blocks]: blocks in each node's blockstore (durable)
  merged,     \* [Nodes -> SUBSET Blocks]: blocks each node has merged (durable)
  owed,       \* [Nodes -> SUBSET Heads]: the creator's retry markers for each node (durable)
  pendDur,    \* [Nodes -> SUBSET Heads]: each node's durable pending records
  wanted,     \* [Nodes -> SUBSET Heads]: roots in each node's in-memory pending map
  connected,  \* SUBSET NodePairs: which directed pairs are connected
  restarted   \* SUBSET Nodes: nodes that used their restart

\* Every variable.
vars == <<have, merged, owed, pendDur, wanted, connected, restarted>>

\* Every variable has the shape its comment says, and merged blocks are stored.
TypeOK ==
  /\ have \in [Nodes -> SUBSET Blocks]          \* blockstores
  /\ merged \in [Nodes -> SUBSET Blocks]        \* merged blocks
  /\ owed \in [Nodes -> SUBSET Heads]           \* retry markers
  /\ pendDur \in [Nodes -> SUBSET Heads]        \* durable pending records
  /\ wanted \in [Nodes -> SUBSET Heads]         \* in-memory pending map
  /\ connected \subseteq NodePairs              \* connections
  /\ restarted \subseteq Nodes                  \* restarts used

\* The creator holds everything; it owes every head to every other node only
\* when it replicates to them. Nobody else holds anything yet.
Init ==
  /\ have = [n \in Nodes |-> IF n = Creator THEN Blocks ELSE {}]      \* the creator's blockstore
  /\ merged = [n \in Nodes |-> IF n = Creator THEN Blocks ELSE {}]    \* the creator's merges
  /\ owed = [n \in Nodes |-> IF Replicated /\ n # Creator THEN Heads ELSE {}] \* markers, if replicated
  /\ pendDur = [n \in Nodes |-> {}]                                   \* no pending record
  /\ wanted = [n \in Nodes |-> {}]                                    \* nothing in memory
  /\ connected = InitialConnected                                     \* the starting connections
  /\ restarted = {}                                                   \* no restart yet

\* m reaches n.
Connected(m, n) == <<m, n>> \in connected
\* Every pair is connected.
FullyConnected == connected = NodePairs
\* Some node connected to n holds block b.
HasConnectedProvider(n, b) == \E m \in Nodes : Connected(m, n) /\ b \in have[m]

\* The creator's retry sweep pushes head h to n. n acknowledges: a head it
\* has merged needs nothing more; otherwise it first writes a durable pending
\* record and puts the root in memory. The acknowledgement clears the marker.
Push(n, h) ==
  /\ h \in owed[n]                                                   \* a marker drives the push
  /\ Connected(Creator, n)                                           \* the push gets through
  /\ owed' = [owed EXCEPT ![n] = @ \ {h}]                            \* the ack clears the marker
  /\ IF h \in merged[n]                                                      \* already merged
       THEN UNCHANGED <<pendDur, wanted>>                            \* nothing written
       ELSE /\ pendDur' = [pendDur EXCEPT ![n] = @ \cup {h}]         \* recorded durably first
            /\ wanted' = [wanted EXCEPT ![n] = @ \cup {h}]           \* and due in memory
  /\ UNCHANGED <<have, merged, connected, restarted>>                \* nothing else moves

\* The resync sweep brings n's durable records back into memory.
Resync(n) ==
  /\ pendDur[n] \ wanted[n] # {}                                     \* a record is not in memory
  /\ wanted' = [wanted EXCEPT ![n] = @ \cup pendDur[n]]              \* it is again
  /\ UNCHANGED <<have, merged, owed, pendDur, connected, restarted>> \* nothing else moves

\* Block b belongs to the history of a root n wants.
FetchTarget(n, b) == \E h \in wanted[n] : b = h \/ b \in AncestorsOf(h)

\* n fetches a missing block of a root it wants from a connected node that has it.
Fetch(n, b) ==
  /\ FetchTarget(n, b)                                               \* a wanted root needs it
  /\ b \notin have[n]                                                \* it is missing
  /\ HasConnectedProvider(n, b)                                      \* someone connected serves it
  /\ have' = [have EXCEPT ![n] = @ \cup {b}]                         \* it is stored
  /\ UNCHANGED <<merged, owed, pendDur, wanted, connected, restarted>> \* nothing else moves

\* n merges a stored block whose parents are merged. Merging a root
\* discharges its pending record and its place in memory.
Merge(n, b) ==
  /\ b \in have[n]                                                   \* stored
  /\ b \notin merged[n]                                              \* not merged yet
  /\ Parents[b] \subseteq merged[n]                                  \* its parents are
  /\ merged' = [merged EXCEPT ![n] = @ \cup {b}]                     \* merged
  /\ pendDur' = [pendDur EXCEPT ![n] = @ \ {b}]                      \* the record is discharged
  /\ wanted' = [wanted EXCEPT ![n] = @ \ {b}]                        \* and leaves memory
  /\ UNCHANGED <<have, owed, connected, restarted>>                  \* nothing else moves

\* n restarts once: its in-memory map is gone; everything durable stays.
Restart(n) ==
  /\ AllowRestart                                                    \* restarts are allowed
  /\ n \notin restarted                                              \* n has not restarted yet
  /\ wanted' = [wanted EXCEPT ![n] = {}]                             \* memory gone
  /\ restarted' = restarted \cup {n}                                 \* counted
  /\ UNCHANGED <<have, merged, owed, pendDur, connected>>            \* durable state stays

\* The world changes which pairs are connected.
SetConnectivity(c) ==
  /\ c \in SUBSET NodePairs                                          \* any set of pairs
  /\ c # connected                                                   \* a change
  /\ connected' = c                                                  \* now connected
  /\ UNCHANGED <<have, merged, owed, pendDur, wanted, restarted>>    \* nothing else moves

\* Every step anyone can take.
Next ==
  \/ \E c \in SUBSET NodePairs : SetConnectivity(c)                  \* connections change
  \/ \E n \in Nodes, h \in Heads : Push(n, h)                        \* the retry sweep
  \/ \E n \in Nodes : Resync(n)                                      \* the resync sweep
  \/ \E n \in Nodes, b \in Blocks : Fetch(n, b) \/ Merge(n, b)       \* fetch and merge
  \/ \E n \in Nodes : Restart(n)                                     \* restarts

\* Fairness for each node's tasks: the creator's retry sweep toward it, its
\* resync sweep, its fetches and its merge loop. Connectivity and restarts
\* are the world's.
Fairness ==
  \A n \in Nodes :                                                   \* for each node
    /\ WF_vars(\E h \in Heads : Push(n, h))                          \* the retry sweep runs
    /\ WF_vars(Resync(n))                                            \* the resync sweep runs
    /\ WF_vars(\E b \in Blocks : Fetch(n, b))                        \* fetches run
    /\ WF_vars(\E b \in Blocks : Merge(n, b))                        \* the merge loop runs

\* The system: start, take steps, with fairness.
Spec == Init /\ [][Next]_vars /\ Fairness

\* ---- Properties ----

\* Promise: a node merges only what its blockstore holds.
INV_DurableMerge == \A n \in Nodes : merged[n] \subseteq have[n]

\* Every node has merged every required block.
AllConverged == \A n \in Nodes : RequiredBlocks \subseteq merged[n]

\* From some point on, every pair stays connected.
EventuallyConnected == <>[]FullyConnected

\* Promise: under eventual connectivity, every node ends up merging every
\* required block and stays that way.
CONV_EventualConnectivity == EventuallyConnected => <>[]AllConverged
====
