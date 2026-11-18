---- MODULE StickyDisconnect ----
\* The story, in plain words.
\*
\* Three nodes connect to each other. Whenever two of them connect, each
\* hands the other a list of the peers it is connected to right then
\* (pinc's peer exchange; a Kademlia answer names peers the same way). A
\* list can take its time to arrive. A node that gets a list dials every peer
\* on it that it has no link to: that is discovery.
\*
\* A node may hang up on a peer on purpose (DisconnectPeers). The hang-up
\* sends the peer a close, which also takes its time to arrive. A link can
\* also just drop (a crash, a restart, a lost network), which nobody chose.
\*
\* Who acts: each node (it connects explicitly, hangs up, or reads a list or
\* a close) and the network (it drops a link).
\*
\* What must never happen:
\*   1. a node dials, through discovery, a peer it hung up on, before
\*      something explicit named that peer again - NoLocalRedial;
\*   2. on a transport whose close says it was deliberate, the peer that was
\*      hung up on dials the node back through discovery either -
\*      NoRedialAfterDeliberateClose (both halves).
\* What must still happen: after a real drop, discovery may reconnect the
\* two (NoRedialAfterDrop is a witness that must FAIL).
\*
\* Why it can go wrong: a list sent before the hang-up can arrive after it,
\* naming the peer that was just hung up on. Without a memory of the hang-up
\* the reader dials it straight back (#2992).
\*
\* Correspondence with the Rust code (crates/database/p2p/src):
\*
\*   | Model             | Rust                                                     |
\*   |-------------------|----------------------------------------------------------|
\*   | quiet[x]          | quiet_peers.rs QuietPeers, held by                       |
\*   |                   | behaviour/dual_kademlia.rs DefraKademlia::quiet          |
\*   | Connect(x, y)     | host/p2p_host/mod.rs dial_peer: DualKademlia::unquiet    |
\*   |                   | (RED Unquiet = FALSE skips it), then the dial            |
\*   |                   | (ConnectPeers, a replicator)                             |
\*   | Disconnect(x, y)  | host/command_handler/network.rs handle_disconnect:       |
\*   |                   | DualKademlia::quiet (RED LocalQuiet = FALSE skips it)    |
\*   | RequestDial(x, y) | a dial another protocol starts to a peer it has no       |
\*   |                   | connection to (a request-response or PushLog request).   |
\*   |                   | Its ConnectionId is not in DefraKademlia::dials, so      |
\*   |                   | handle_pending_outbound_connection lets it through (RED  |
\*   |                   | OnlyDiscoveryChecked = FALSE refuses it)                 |
\*   | DeliverClose(m)   | the peer sees the connection close. libp2p carries no    |
\*   |                   | reason, so RemoteQuiet = FALSE on libp2p                 |
\*   | DeliverList(m)    | a Kademlia query dialing the peers an answer names:      |
\*   |                   | DefraKademlia::poll records its dial, and                |
\*   |                   | handle_pending_outbound_connection refuses it when the   |
\*   |                   | peer is quiet                                            |
\*   | Drop(x, y)        | a connection error, or the peer restarting: nothing      |
\*   |                   | quiets                                                   |
\*
\* On pinc (crates/database/p2p/src/pinc), where both halves hold:
\*
\*   | Model             | Rust                                                     |
\*   |-------------------|----------------------------------------------------------|
\*   | quiet[x]          | endpoint.rs Node::quiet, read by Node::is_quiet          |
\*   | Connect(x, y)     | endpoint.rs Command::Dial and Node::allow lift it        |
\*   | RequestDial(x, y) | any other dial: only exchange.rs learn_peers reads       |
\*   |                   | is_quiet, so nothing else is refused                     |
\*   | Disconnect(x, y)  | endpoint.rs Node::hang_up: quiets y, closes each link    |
\*   |                   | with CLOSE_DISCONNECTED (RED LocalQuiet = FALSE skips    |
\*   |                   | the quiet)                                               |
\*   | DeliverClose(m)   | serve.rs serve_connection ends and hung_up_on reads the  |
\*   |                   | code: quiets x. is_quiet also reads a closed link not    |
\*   |                   | yet forgotten, so the close and its reason are one step  |
\*   | DeliverList(m)    | exchange.rs learn_peers: known_or_barred skips a quiet   |
\*   |                   | peer                                                     |
\*   | Drop(x, y)        | a timeout, the peer restarting or any other close code:  |
\*   |                   | not quiet                                                |
\*
\* Gaps, explained:
\*   * A discovery dial is one step: it is decided and lands at once. A list
\*     names a peer only when the reader has no link to it, and a node never
\*     hangs up on a peer it has no link to, so a dial already in flight
\*     cannot be overtaken by a hang-up of the same pair.
\*   * Links are one truth value per side, not one per connection. An
\*     explicit connect waits until any close between the two has arrived,
\*     so an old close never lands on a new link.
\*   * A Kademlia answer can name a routing-table peer that is no longer
\*     connected. Every routing-table peer was connected once, so a list
\*     taken while it was connected and delivered late stands in for it.
\*   * Two explicit connects (MaxConnects) keep the model finite. Discovery
\*     adds links of its own, so two are enough for each RED to plant its
\*     bug, and every RED configuration fails within that bound.
\*   * The quiet table is bounded (MAX_QUIET_PEERS, oldest evicted first).
\*     Three nodes never fill it; eviction is checked by the Rust test
\*     a_full_table_forgets_its_oldest_peer_and_counts_it.
\*   * libp2p's Kademlia bootstraps (the refresh Go's kad-dht runs when a
\*     new peer joins a small routing table, DefraKademlia::refresh_if_low,
\*     at most one at a time per table, and the periodic one) are more lists
\*     a node hands itself: a bootstrap's answers name peers, and its dials
\*     pass the same quiet check. How many run at once is load, not safety,
\*     so the model leaves it out; ten_new_peers_start_one_bootstrap_per_table
\*     checks it. On libp2p the peer hung up on can still bootstrap later (a
\*     new routing-table peer, the periodic tick), which is the remoteRedial
\*     that Red_Libp2pRemote shows.
\*   * A node that restarts forgets its quiet table, its routing table and
\*     every link at once: the table lives in memory, as Go's peerstore does
\*     (reqs/defra/mutation-net.md says so). That is Init for that one node,
\*     so the model has no restart action; Drop stands for the peer's
\*     restart, which the node sees as a dropped link.
\*   * A hang-up is undone only by the side that chose it, and an explicit
\*     connect undoes only the dialer's own records: when the peer hung up
\*     on dials back explicitly, the node that hung up still keeps it quiet.

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
  Nodes,        \* the three nodes
  LocalQuiet,   \* TRUE as written: a hang-up quiets the peer for discovery
  RemoteQuiet,  \* TRUE when the close tells the peer it was deliberate
  Unquiet,      \* TRUE as written: an explicit connect lifts the quiet
  OnlyDiscoveryChecked, \* TRUE as written: only discovery's dials are refused
  MaxConnects   \* how many explicit connects the model allows, to stay finite

VARIABLES
  up,           \* up[x][y]: x holds a live link to y, as x sees it
  quiet,        \* quiet[x]: the peers x's discovery must not dial
  msgs,         \* the lists and closes on their way
  hungUp,       \* pairs <<x, y>>: x hung up on y, nothing explicit by x since
  hungUpOn,     \* pairs <<y, x>>: x hung up on y, nothing explicit by y since
  dropped,      \* pairs whose link dropped, nothing explicit since
  localRedial,  \* TRUE once a node discovery-dialed a peer it hung up on
  remoteRedial, \* TRUE once a node discovery-dialed a peer that hung up on it
  dropRedial,   \* TRUE once discovery reconnected a dropped pair
  requestRefused, \* TRUE once another protocol's dial to a quiet peer was refused
  connects      \* how many explicit connects and request dials ran

vars == <<up, quiet, msgs, hungUp, hungUpOn, dropped,
          localRedial, remoteRedial, dropRedial, requestRefused, connects>>

\* Every ordered pair of distinct nodes.
Pairs == {p \in Nodes \X Nodes : p[1] # p[2]}

\* Every message the network could carry: a list names peers, a close none.
Msg == [kind : {"list", "close"}, from : Nodes, to : Nodes, names : SUBSET Nodes]

\* The list `f` sends `t` under links `u`: f's linked peers, without t.
List(u, f, t) == [kind |-> "list", from |-> f, to |-> t,
                  names |-> {n \in Nodes : u[f][n]} \ {t}]

\* The lists two newly linked nodes send each other; an empty one is not sent
\* (share_peers returns when it has nothing to share).
ListsOf(u, x, y) == {l \in {List(u, x, y), List(u, y, x)} : l.names # {}}

\* A close from x to y is still on its way.
CloseInFlight(x, y) ==
  \E m \in msgs : m.kind = "close" /\ {m.from, m.to} = {x, y}

TypeOK ==
  /\ up \in [Nodes -> [Nodes -> BOOLEAN]]   \* a view per node per peer
  /\ quiet \in [Nodes -> SUBSET Nodes]      \* a set of peers per node
  /\ msgs \subseteq Msg                     \* only well-formed messages
  /\ hungUp \subseteq Pairs                 \* pairs of distinct nodes
  /\ hungUpOn \subseteq Pairs               \* the same
  /\ dropped \subseteq Pairs                \* the same
  /\ localRedial \in BOOLEAN                \* a flag
  /\ remoteRedial \in BOOLEAN               \* a flag
  /\ dropRedial \in BOOLEAN                 \* a flag
  /\ requestRefused \in BOOLEAN             \* a flag
  /\ connects \in 0..MaxConnects            \* never past the bound

Init ==
  /\ up = [x \in Nodes |-> [y \in Nodes |-> FALSE]]  \* nobody is linked
  /\ quiet = [x \in Nodes |-> {}]                    \* nobody is quiet
  /\ msgs = {}                                       \* nothing on the wire
  /\ hungUp = {}                                     \* no hang-ups yet
  /\ hungUpOn = {}                                   \* so none seen either
  /\ dropped = {}                                    \* no drops yet
  /\ localRedial = FALSE                             \* nothing went wrong
  /\ remoteRedial = FALSE                            \* nothing went wrong
  /\ dropRedial = FALSE                              \* no reconnect yet
  /\ requestRefused = FALSE                          \* no dial refused yet
  /\ connects = 0                                    \* no explicit connect yet

\* x dials y explicitly (ConnectPeers, a replicator). That names y again,
\* so x's discovery may dial it from now on: x's own records of a hang-up
\* between them are done with. y keeps its own; only y can lift those. Each
\* side sends its list.
Connect(x, y) ==
  /\ x # y                                           \* a node does not dial itself
  /\ ~up[x][y] /\ ~up[y][x]                          \* no link either side sees
  /\ ~CloseInFlight(x, y)                            \* the old link is fully closed
  /\ connects < MaxConnects                          \* stay within the bound
  /\ LET u == [up EXCEPT ![x][y] = TRUE, ![y][x] = TRUE] \* the new links
     IN /\ up' = u                                   \* both sides see the link
        /\ msgs' = msgs \cup ListsOf(u, x, y)        \* and send their lists
  /\ quiet' = IF Unquiet                             \* the guard of the lift:
                THEN [quiet EXCEPT ![x] = @ \ {y}]   \* unquiet: y was named
                ELSE quiet                           \* RED: y stays quiet anyway
  /\ hungUp' = hungUp \ {<<x, y>>}                   \* x's hang-up on y is undone
  /\ hungUpOn' = hungUpOn \ {<<x, y>>}               \* as is x's record of y's
  /\ dropped' = dropped \ {<<x, y>>, <<y, x>>}       \* and a drop is behind us
  /\ connects' = connects + 1                        \* one more explicit connect
  /\ UNCHANGED <<localRedial, remoteRedial, dropRedial, requestRefused>> \* no discovery dial here

\* Another protocol on x (a request-response or PushLog request) dials y,
\* which x has no link to. That is not discovery and names nothing, so it
\* lifts no quiet; as written it goes through. With the RED
\* OnlyDiscoveryChecked = FALSE, x's quiet check refuses it.
RequestDial(x, y) ==
  /\ x # y                                           \* a node does not dial itself
  /\ ~up[x][y] /\ ~up[y][x]                          \* no link either side sees
  /\ ~CloseInFlight(x, y)                            \* the old link is fully closed
  /\ connects < MaxConnects                          \* shares the explicit bound
  /\ connects' = connects + 1                        \* one more dial counted
  /\ IF ~OnlyDiscoveryChecked /\ y \in quiet[x]      \* RED: every dial checked
       THEN /\ requestRefused' = TRUE                \* the request never lands
            /\ UNCHANGED <<up, msgs>>                \* and no link comes up
       ELSE LET u == [up EXCEPT ![x][y] = TRUE, ![y][x] = TRUE] \* the new links
            IN /\ up' = u                            \* both sides see the link
               /\ msgs' = msgs \cup ListsOf(u, x, y) \* and send their lists
               /\ UNCHANGED requestRefused           \* nothing was refused
  /\ UNCHANGED <<quiet, hungUp, hungUpOn, dropped, localRedial, remoteRedial,
                 dropRedial>>                        \* no quiet moves

\* x hangs up on y on purpose. With LocalQuiet x remembers it; the close
\* goes to y.
Disconnect(x, y) ==
  /\ up[x][y]                                        \* x has a link to y
  /\ up' = [up EXCEPT ![x][y] = FALSE]               \* x drops it now
  /\ quiet' = IF LocalQuiet                          \* the guard of half 1:
                THEN [quiet EXCEPT ![x] = @ \cup {y}] \* x quiets y
                ELSE quiet                           \* RED: x forgets at once
  /\ msgs' = msgs \cup {[kind |-> "close", from |-> x, to |-> y, names |-> {}]} \* the close leaves
  /\ hungUp' = hungUp \cup {<<x, y>>}                \* x chose this
  /\ hungUpOn' = hungUpOn \cup {<<y, x>>}            \* y had it chosen for it
  /\ UNCHANGED <<dropped, localRedial, remoteRedial, dropRedial, requestRefused,
                 connects>>                          \* nothing else moves

\* The close reaches y: y sees the link gone at the same moment it reads why,
\* and with RemoteQuiet it quiets x for discovery.
DeliverClose(m) ==
  /\ m \in msgs /\ m.kind = "close"                  \* a close on its way
  /\ up' = [up EXCEPT ![m.to][m.from] = FALSE]       \* y's view of the link ends
  /\ quiet' = IF RemoteQuiet                         \* the guard of half 2:
                THEN [quiet EXCEPT ![m.to] = @ \cup {m.from}] \* y quiets x
                ELSE quiet                           \* libp2p, or RED: y forgets
  /\ msgs' = msgs \ {m}                              \* the close is read
  /\ UNCHANGED <<hungUp, hungUpOn, dropped, localRedial, remoteRedial,
                 dropRedial, requestRefused, connects>> \* nothing else moves

\* A link between x and y drops for real: both sides see it at once, nobody
\* chose it, and nobody quiets anybody.
Drop(x, y) ==
  /\ up[x][y] /\ up[y][x]                            \* a live link both sides see
  /\ up' = [up EXCEPT ![x][y] = FALSE, ![y][x] = FALSE] \* it is gone
  /\ dropped' = dropped \cup {<<x, y>>, <<y, x>>}    \* remember it was a drop
  /\ UNCHANGED <<quiet, msgs, hungUp, hungUpOn, localRedial, remoteRedial,
                 dropRedial, requestRefused, connects>> \* nothing else moves

\* A list reaches r: r dials every peer on it it has no link to and has not
\* quieted. Each dial lands, and the new pairs send their lists.
DeliverList(m) ==
  /\ m \in msgs /\ m.kind = "list"                   \* a list on its way
  /\ LET r == m.to                                   \* the reader
         \* the peers r's discovery dials
         targets == {n \in m.names : n # r /\ ~up[r][n] /\ n \notin quiet[r]}
         \* the links after those dials land, both sides
         u == [a \in Nodes |-> [b \in Nodes |->
                 up[a][b] \/ (a = r /\ b \in targets) \/ (b = r /\ a \in targets)]]
         \* the pairs r just dialed
         dialed == {<<r, n>> : n \in targets}
     IN /\ up' = u                                   \* the dials landed
        /\ msgs' = (msgs \ {m}) \cup UNION {ListsOf(u, r, n) : n \in targets} \* new lists leave
        /\ localRedial' = (localRedial \/ dialed \cap hungUp # {})    \* r redialed a peer it hung up on
        /\ remoteRedial' = (remoteRedial \/ dialed \cap hungUpOn # {}) \* r redialed a peer that hung up on it
        /\ dropRedial' = (dropRedial \/ dialed \cap dropped # {})      \* discovery mended a drop
  /\ UNCHANGED <<quiet, hungUp, hungUpOn, dropped, requestRefused, connects>> \* the rest stays

Next ==
  \/ \E x, y \in Nodes : Connect(x, y)               \* an explicit dial
  \/ \E x, y \in Nodes : RequestDial(x, y)           \* another protocol's dial
  \/ \E x, y \in Nodes : Disconnect(x, y)            \* a hang-up on purpose
  \/ \E x, y \in Nodes : Drop(x, y)                  \* a real drop
  \/ \E m \in msgs : DeliverClose(m)                 \* a close arrives
  \/ \E m \in msgs : DeliverList(m)                  \* a list arrives

Spec == Init /\ [][Next]_vars

\* The nodes are interchangeable: renaming them changes no verdict, so TLC
\* checks one state of each renaming.
Symmetry == Permutations(Nodes)

\* NoLocalRedial: a node never discovery-dials a peer it hung up on until
\* something explicit names that peer again.
NoLocalRedial == ~localRedial

\* NoRedialAfterDeliberateClose: neither the node that hung up nor the peer
\* it hung up on dials the other through discovery until something explicit
\* names it again.
NoRedialAfterDeliberateClose == ~localRedial /\ ~remoteRedial

\* NoRedialAfterDrop: discovery never mends a real drop. This must FAIL: a
\* drop is not a deliberate close, and discovery reconnects it as before.
NoRedialAfterDrop == ~dropRedial

\* UnquietOnConnect: a node keeps a peer quiet only while it holds a record
\* of a hang-up between them that it has not undone, so once it dials the
\* peer explicitly, its discovery may reach the peer again. The lift happens
\* in the same step as the connect, so this is checked in every state rather
\* than as an eventually: a state after the connect with the peer still quiet
\* is already the bug.
UnquietOnConnect ==
  \A x \in Nodes : \A y \in quiet[x] :               \* every quiet peer
    <<x, y>> \in hungUp \cup hungUpOn                \* has a live hang-up behind it

\* RequestsStillDial: the quiet table never refuses a dial another protocol
\* starts; it keeps only discovery away, so PushLog and requests still reach
\* the peer.
RequestsStillDial == ~requestRefused
====
