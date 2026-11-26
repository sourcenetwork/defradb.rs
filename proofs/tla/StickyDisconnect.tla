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
\*   |                   | peer is quiet; the dial stays in DefraKademlia::dials    |
\*   | LandDial(p)       | handle_established_outbound_connection reads the quiet   |
\*   |                   | again and refuses a dial quieted meanwhile (RED          |
\*   |                   | RecheckQuiet = FALSE lets it land)                       |
\*   | Drop(x, y)        | a connection error, or the peer restarting: nothing      |
\*   |                   | quiets                                                   |
\*   | LoseClose(m)      | the same on the side hung up on: it never sees a reason  |
\*
\* On pinc (crates/database/p2p/src/pinc), where both halves hold:
\*
\*   | Model             | Rust                                                     |
\*   |-------------------|----------------------------------------------------------|
\*   | quiet[x]          | endpoint.rs Node::quiet, read by Node::is_quiet          |
\*   | Connect(x, y)     | endpoint.rs Command::Dial and Node::allow lift it        |
\*   | RequestDial(x, y) | any other dial: only exchange.rs learn_peers reads       |
\*   |                   | is_quiet, so nothing else is refused                     |
\*   | Disconnect(x, y)  | endpoint.rs Node::hang_up: quiets y, with a link or not, |
\*   |                   | then closes each link with CLOSE_DISCONNECTED (RED       |
\*   |                   | LocalQuiet = FALSE skips the quiet, RED QuietUnlinked =  |
\*   |                   | FALSE skips it when no link is up)                       |
\*   | DeliverClose(m)   | serve.rs serve_connection ends and hung_up_on reads the  |
\*   |                   | code: quiets x. is_quiet also reads a closed link not    |
\*   |                   | yet forgotten, so the close and its reason are one step  |
\*   | DeliverList(m)    | exchange.rs learn_peers: known_or_barred skips a quiet   |
\*   |                   | peer, and spawns a dial of each other one                |
\*   | LandDial(p)       | the spawned dial: is_quiet before the dial, and after it |
\*   |                   | a dial that landed on a peer quiet by then is hung up    |
\*   |                   | on at once (RED RecheckQuiet = FALSE skips both)         |
\*   | Drop(x, y)        | a timeout, the peer restarting or any other close code:  |
\*   |                   | not quiet                                                |
\*   | LoseClose(m)      | the peer's link times out before CLOSE_DISCONNECTED      |
\*   |                   | arrives: hung_up_on never sees it, not quiet             |
\*
\* Gaps, explained:
\*   * A discovery dial is decided when its list is read (DeliverList) and
\*     lands later (LandDial), so a hang-up can come in between. The
\*     dial that lands on a peer quiet by then is refused in one step: on
\*     pinc the link is up for a moment and hung up on at once, which the
\*     model folds into the refusal.
\*   * A close can be lost (LoseClose): the peer hung up on learns nothing and
\*     may dial back, as after a drop. NoRedialAfterDeliberateClose is about
\*     a close that arrived; NoRedialAfterLostClose is the witness that must
\*     FAIL.
\*   * Links are one truth value per side, not one per connection. An
\*     explicit connect waits until any close between the two has arrived,
\*     so an old close never lands on a new link.
\*   * A Kademlia answer can name a routing-table peer that is no longer
\*     connected. Every routing-table peer was connected once, so a list
\*     taken while it was connected and delivered late stands in for it.
\*   * To stay finite, a hang-up with no link up is only the first hang-up of
\*     a run, and one lost close stands at a time. Each RED that needs one
\*     still fails within that bound.
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
  QuietUnlinked, \* TRUE as written: it does so with no link up as well
  RemoteQuiet,  \* TRUE when the close tells the peer it was deliberate
  Unquiet,      \* TRUE as written: an explicit connect lifts the quiet
  OnlyDiscoveryChecked, \* TRUE as written: only discovery's dials are refused
  RecheckQuiet, \* TRUE as written: a discovery dial reads the quiet as it lands
  MaxConnects   \* how many explicit connects the model allows, to stay finite

VARIABLES
  up,           \* up[x][y]: x holds a live link to y, as x sees it
  quiet,        \* quiet[x]: the peers x's discovery must not dial
  msgs,         \* the lists and closes on their way
  pending,      \* pairs <<r, n>>: r's discovery decided to dial n, not landed
  hungUp,       \* pairs <<x, y>>: x hung up on y, nothing explicit by x since
  hungUpOn,     \* pairs <<y, x>>: x hung up on y, nothing explicit by y since
  dropped,      \* pairs whose link dropped, nothing explicit since
  lostClose,    \* pairs <<y, x>>: x's close to y was lost, nothing explicit since
  localRedial,  \* TRUE once a node discovery-dialed a peer it hung up on
  remoteRedial, \* TRUE once a node discovery-dialed a peer that hung up on it
  dropRedial,   \* TRUE once discovery reconnected a dropped pair
  lostRedial,   \* TRUE once discovery reconnected a pair whose close was lost
  requestRefused, \* TRUE once another protocol's dial to a quiet peer was refused
  connects      \* how many explicit connects and request dials ran

vars == <<up, quiet, msgs, pending, hungUp, hungUpOn, dropped, lostClose,
          localRedial, remoteRedial, dropRedial, lostRedial, requestRefused,
          connects>>

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
  /\ pending \subseteq Pairs                \* pairs of distinct nodes
  /\ hungUp \subseteq Pairs                 \* the same
  /\ hungUpOn \subseteq Pairs               \* the same
  /\ dropped \subseteq Pairs                \* the same
  /\ lostClose \subseteq Pairs              \* the same
  /\ localRedial \in BOOLEAN                \* a flag
  /\ remoteRedial \in BOOLEAN               \* a flag
  /\ dropRedial \in BOOLEAN                 \* a flag
  /\ lostRedial \in BOOLEAN                 \* a flag
  /\ requestRefused \in BOOLEAN             \* a flag
  /\ connects \in 0..MaxConnects            \* never past the bound

Init ==
  /\ up = [x \in Nodes |-> [y \in Nodes |-> FALSE]]  \* nobody is linked
  /\ quiet = [x \in Nodes |-> {}]                    \* nobody is quiet
  /\ msgs = {}                                       \* nothing on the wire
  /\ pending = {}                                    \* no dial on its way
  /\ hungUp = {}                                     \* no hang-ups yet
  /\ hungUpOn = {}                                   \* so none seen either
  /\ dropped = {}                                    \* no drops yet
  /\ lostClose = {}                                  \* no close lost yet
  /\ localRedial = FALSE                             \* nothing went wrong
  /\ remoteRedial = FALSE                            \* nothing went wrong
  /\ dropRedial = FALSE                              \* no reconnect yet
  /\ lostRedial = FALSE                              \* no reconnect yet
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
  /\ lostClose' = lostClose \ {<<x, y>>, <<y, x>>}   \* as is a lost close
  /\ connects' = connects + 1                        \* one more explicit connect
  /\ UNCHANGED <<pending, localRedial, remoteRedial, dropRedial, lostRedial,
                 requestRefused>>                    \* no discovery dial here

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
  /\ UNCHANGED <<quiet, pending, hungUp, hungUpOn, dropped, lostClose,
                 localRedial, remoteRedial, dropRedial, lostRedial>> \* no quiet moves

\* x hangs up on y on purpose, with a link to it or not (DisconnectPeers
\* answers the same either way). With LocalQuiet x remembers it; a close goes
\* to y only over a link x holds.
Disconnect(x, y) ==
  /\ x # y                                           \* a node does not hang up on itself
  /\ up[x][y] \/ hungUp = {}                         \* unlinked: only as the first hang-up
  /\ up' = [up EXCEPT ![x][y] = FALSE]               \* x drops any link now
  /\ quiet' = IF LocalQuiet /\ (QuietUnlinked \/ up[x][y]) \* the guard of half 1:
                THEN [quiet EXCEPT ![x] = @ \cup {y}] \* x quiets y
                ELSE quiet                           \* RED: x forgets at once
  /\ msgs' = IF up[x][y]                             \* a close only over a link
               THEN msgs \cup {[kind |-> "close", from |-> x, to |-> y, names |-> {}]}
               ELSE msgs                            \* nothing to close
  /\ hungUp' = hungUp \cup {<<x, y>>}                \* x chose this
  /\ hungUpOn' = IF up[x][y]                         \* y is told only by a close
                   THEN hungUpOn \cup {<<y, x>>}     \* y had it chosen for it
                   ELSE hungUpOn                     \* y never hears of it
  /\ UNCHANGED <<pending, dropped, lostClose, localRedial, remoteRedial,
                 dropRedial, lostRedial, requestRefused, connects>> \* nothing else moves

\* The close reaches y: y sees the link gone at the same moment it reads why,
\* and with RemoteQuiet it quiets x for discovery.
DeliverClose(m) ==
  /\ m \in msgs /\ m.kind = "close"                  \* a close on its way
  /\ up' = [up EXCEPT ![m.to][m.from] = FALSE]       \* y's view of the link ends
  /\ quiet' = IF RemoteQuiet                         \* the guard of half 2:
                THEN [quiet EXCEPT ![m.to] = @ \cup {m.from}] \* y quiets x
                ELSE quiet                           \* libp2p, or RED: y forgets
  /\ msgs' = msgs \ {m}                              \* the close is read
  /\ UNCHANGED <<pending, hungUp, hungUpOn, dropped, lostClose, localRedial,
                 remoteRedial, dropRedial, lostRedial, requestRefused,
                 connects>>                          \* nothing else moves

\* The close never reaches y: y's link times out first, so y sees a drop and
\* learns nothing of the hang-up. A quiet y already holds from an earlier
\* close that arrived stays, with its record.
LoseClose(m) ==
  /\ m \in msgs /\ m.kind = "close"                  \* a close on its way
  /\ lostClose = {}                                  \* one lost close at a time
  /\ up' = [up EXCEPT ![m.to][m.from] = FALSE]       \* y's view of the link ends
  /\ msgs' = msgs \ {m}                              \* the close is gone
  /\ IF m.from \in quiet[m.to]                       \* y quiet for an earlier close
       THEN UNCHANGED <<hungUpOn, lostClose>>        \* that one still stands
       ELSE /\ hungUpOn' = hungUpOn \ {<<m.to, m.from>>} \* y was never told
            /\ lostClose' = lostClose \cup {<<m.to, m.from>>} \* it was lost
  /\ UNCHANGED <<quiet, pending, hungUp, dropped, localRedial, remoteRedial,
                 dropRedial, lostRedial, requestRefused, connects>> \* nothing else moves

\* A link between x and y drops for real: both sides see it at once, nobody
\* chose it, and nobody quiets anybody.
Drop(x, y) ==
  /\ up[x][y] /\ up[y][x]                            \* a live link both sides see
  /\ up' = [up EXCEPT ![x][y] = FALSE, ![y][x] = FALSE] \* it is gone
  /\ dropped' = dropped \cup {<<x, y>>, <<y, x>>}    \* remember it was a drop
  /\ UNCHANGED <<quiet, msgs, pending, hungUp, hungUpOn, lostClose, localRedial,
                 remoteRedial, dropRedial, lostRedial, requestRefused,
                 connects>>                          \* nothing else moves

\* A list reaches r: r decides to dial every peer on it it has no link to
\* and has not quieted. The dials are on their way; LandDial lands them.
DeliverList(m) ==
  /\ m \in msgs /\ m.kind = "list"                   \* a list on its way
  /\ LET r == m.to                                   \* the reader
         \* the peers r's discovery dials
         targets == {n \in m.names : n # r /\ ~up[r][n] /\ n \notin quiet[r]}
     IN pending' = pending \cup {<<r, n>> : n \in targets} \* the dials leave
  /\ msgs' = msgs \ {m}                              \* the list is read
  /\ UNCHANGED <<up, quiet, hungUp, hungUpOn, dropped, lostClose, localRedial,
                 remoteRedial, dropRedial, lostRedial, requestRefused,
                 connects>>                          \* the rest stays

\* r's dial of n lands. A link r already holds is reused (connection returns
\* it); with RecheckQuiet a peer quiet by now is refused. Otherwise the link
\* comes up and the pair sends its lists.
LandDial(p) ==
  /\ p \in pending                                   \* a dial on its way
  /\ pending' = pending \ {p}                        \* it lands now
  /\ LET r == p[1]                                   \* the dialer
         n == p[2]                                   \* the peer
     IN IF up[r][n] \/ (RecheckQuiet /\ n \in quiet[r]) \* reused, or refused
          THEN UNCHANGED <<up, msgs, localRedial, remoteRedial, dropRedial,
                           lostRedial>>              \* nothing new comes up
          ELSE LET u == [up EXCEPT ![r][n] = TRUE, ![n][r] = TRUE] \* the new links
               IN /\ up' = u                         \* both sides see the link
                  /\ msgs' = msgs \cup ListsOf(u, r, n) \* and send their lists
                  /\ localRedial' = (localRedial \/ p \in hungUp)    \* r redialed a peer it hung up on
                  /\ remoteRedial' = (remoteRedial \/ p \in hungUpOn) \* r redialed a peer that hung up on it
                  /\ dropRedial' = (dropRedial \/ p \in dropped)      \* discovery mended a drop
                  /\ lostRedial' = (lostRedial \/ p \in lostClose)    \* or a lost close
  /\ UNCHANGED <<quiet, hungUp, hungUpOn, dropped, lostClose, requestRefused,
                 connects>>                          \* the rest stays

Next ==
  \/ \E x, y \in Nodes : Connect(x, y)               \* an explicit dial
  \/ \E x, y \in Nodes : RequestDial(x, y)           \* another protocol's dial
  \/ \E x, y \in Nodes : Disconnect(x, y)            \* a hang-up on purpose
  \/ \E x, y \in Nodes : Drop(x, y)                  \* a real drop
  \/ \E m \in msgs : DeliverClose(m)                 \* a close arrives
  \/ \E m \in msgs : LoseClose(m)                    \* a close is lost
  \/ \E m \in msgs : DeliverList(m)                  \* a list arrives
  \/ \E p \in pending : LandDial(p)                  \* a discovery dial lands

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

\* NoRedialAfterLostClose: discovery never mends a pair whose close was
\* lost. This must FAIL: the peer was never told, so it is a drop to it.
NoRedialAfterLostClose == ~lostRedial

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
