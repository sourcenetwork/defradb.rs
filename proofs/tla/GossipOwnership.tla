---- MODULE GossipOwnership ----
\* The story: our node gossips with its peers. Gossip's state machine learns
\* about peers from other peers, so it can want to talk to anyone, including
\* a peer we have revoked. Gossip rides the one mux connection we keep per
\* peer: it never opens or closes a connection itself. When it wants a peer
\* it has no connection to, it asks for the normal dial, which checks that the
\* peer is admitted before and after connecting. Some peers still run the
\* older mux, which carries every protocol but gossip: a connection to one
\* is open, but gossip cannot use it.
\*
\* Who acts: gossip (learning and forgetting peers), our dialer, peers dialing
\* us, the operator revoking a peer, and the network dropping connections.
\*
\* What must never happen: gossip reaches a revoked peer, gossip runs on a
\* connection that is gone, or gossip closes a connection other protocols
\* share. What must always happen: a healthy peer gossip wants is reached,
\* once gossip stops dropping it. Gossip may drop and relearn the healthy
\* peer a bounded number of times; iroh-gossip makes no promise to relearn a
\* peer it keeps dropping, so neither does this model. And every dial gossip
\* asks for is answered, even one that lands on an older mux: an unanswered
\* ask leaves the peer looking reachable to gossip forever, holding a slot
\* in its view, and gossip never asks for it again.
\*
\* Code it models (crates/p2p/src/iroh):
\*   gossip/driver.rs         Driver::send        (a send path only on a live connection)
\*   gossip/driver.rs         Driver::connected   (a connection already closed is ignored)
\*   gossip/driver.rs         Driver::forget      (drops the send path, never the connection)
\*   gossip/driver.rs         Driver::send        (asks for a dial; `dialing` is `asked` here)
\*   gossip/mod.rs            GossipHandle::connected (an older mux answers the ask as a failure)
\*   endpoint_rpc.rs          connect_with_cache  (admission before and after dialing)
\*   endpoint_streams.rs      handle_incoming     (admission on accept)
\*   endpoint_commands.rs     handle_deny_peer    (revoke, then close the connection)
EXTENDS Naturals

CONSTANTS
  Peers,          \* the peers in the run
  Revocable,      \* peers the operator may revoke
  Healthy,        \* a peer that is never revoked: the one we promise to reach
  Legacy,         \* peers on the older mux, whose connections cannot carry gossip
  MaxDrops,       \* how many times the network may drop a connection
  MaxForgets,     \* how many times gossip may drop the healthy peer
  ForgetBound,    \* "Bounded" (MaxForgets holds) | "Unbounded" (gossip may drop it forever)
  PathMode,       \* "SharedMux" (gossip rides our connection) | "OwnConnection" (today: gossip dials its own)
  RecheckMode,    \* "Recheck" (admission checked again once connected) | "None"
  ForgetMode,     \* "DropSender" (gossip forgets a peer, connection stays) | "CloseConnection"
  LegacyMode      \* "Answer" (an older-mux connection answers gossip's ask) | "Silent"

\* The healthy peer is a real peer and is never revoked.
ASSUME Healthy \in Peers /\ Healthy \notin Revocable
\* Only real peers can be revoked.
ASSUME Revocable \subseteq Peers
\* Only real peers run the older mux, and the healthy peer is not one of them.
ASSUME Legacy \subseteq Peers /\ Healthy \notin Legacy
ASSUME MaxDrops \in Nat
ASSUME MaxForgets \in Nat
ASSUME ForgetBound \in {"Bounded", "Unbounded"}
ASSUME PathMode \in {"SharedMux", "OwnConnection"}
ASSUME RecheckMode \in {"Recheck", "None"}
ASSUME ForgetMode \in {"DropSender", "CloseConnection"}
ASSUME LegacyMode \in {"Answer", "Silent"}

VARIABLES
  admitted,        \* [Peers -> BOOLEAN] the peer may talk to us
  conn,            \* [Peers -> BOOLEAN] our mux connection to the peer is open
  dialing,         \* [Peers -> BOOLEAN] a dial passed its first check and is connecting
  gconn,           \* [Peers -> BOOLEAN] gossip's own connection (OwnConnection only)
  path,            \* [Peers -> BOOLEAN] gossip holds a send path to the peer
  wants,           \* [Peers -> BOOLEAN] gossip wants to talk to the peer
  asked,           \* [Peers -> BOOLEAN] gossip asked for a dial and awaits the answer
  drops,           \* how many connections the network has dropped so far
  forgets,         \* how many times gossip has dropped the healthy peer
  closedByGossip   \* gossip once closed a connection other protocols share

vars == <<admitted, conn, dialing, gconn, path, wants, asked, drops, forgets, closedByGossip>>

\* The connection gossip sends on: ours when it rides the mux (and the peer
\* runs the current one), its own otherwise.
GossipConn(p) == IF PathMode = "SharedMux" THEN conn[p] /\ p \notin Legacy ELSE gconn[p]

\* What becomes of gossip's ask once a connection to p opens: a usable
\* connection answers it, and so does an older mux unless LegacyMode is
\* "Silent", which drops that answer.
AskAfterOpen(p) == asked[p] /\ p \in Legacy /\ LegacyMode = "Silent"

TypeOK ==
  /\ admitted \in [Peers -> BOOLEAN]
  /\ conn \in [Peers -> BOOLEAN]
  /\ dialing \in [Peers -> BOOLEAN]
  /\ gconn \in [Peers -> BOOLEAN]
  /\ path \in [Peers -> BOOLEAN]
  /\ wants \in [Peers -> BOOLEAN]
  /\ asked \in [Peers -> BOOLEAN]
  /\ drops \in 0..MaxDrops
  /\ forgets \in 0..MaxForgets
  /\ closedByGossip \in BOOLEAN

\* Every peer starts admitted and unknown to gossip, with nothing open.
Init ==
  /\ admitted = [p \in Peers |-> TRUE]
  /\ conn = [p \in Peers |-> FALSE]
  /\ dialing = [p \in Peers |-> FALSE]
  /\ gconn = [p \in Peers |-> FALSE]
  /\ path = [p \in Peers |-> FALSE]
  /\ wants = [p \in Peers |-> FALSE]
  /\ asked = [p \in Peers |-> FALSE]
  /\ drops = 0
  /\ forgets = 0
  /\ closedByGossip = FALSE

\* Gossip hears about a peer from another peer. It may be any peer, revoked
\* or not: membership comes from outside and we cannot filter it.
Learn(p) ==
  /\ ~wants[p]
  /\ wants' = [wants EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, path, asked, drops, forgets, closedByGossip>>

\* Gossip forgets a peer (iroh-gossip's DisconnectPeer). Our driver only
\* drops its sender; the CloseConnection bug also closes the shared
\* connection. The healthy peer may be dropped too, at most MaxForgets
\* times; with ForgetBound "Unbounded" the count never grows, so gossip may
\* drop it forever.
Forget(p) ==
  /\ wants[p]
  \* The healthy peer is dropped only while the bound allows it.
  /\ p = Healthy => forgets < MaxForgets
  \* Count a drop of the healthy peer, unless the bound is switched off.
  /\ forgets' = IF p = Healthy /\ ForgetBound = "Bounded" THEN forgets + 1 ELSE forgets
  /\ wants' = [wants EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = FALSE]
  /\ IF ForgetMode = "CloseConnection" /\ conn[p]
     THEN /\ conn' = [conn EXCEPT ![p] = FALSE]
          /\ closedByGossip' = TRUE
     ELSE UNCHANGED <<conn, closedByGossip>>
  /\ UNCHANGED <<admitted, dialing, gconn, asked, drops>>

\* Gossip has something for a peer it holds no connection to, so it asks the
\* endpoint for a dial, once, and waits for the answer.
Ask(p) ==
  /\ PathMode = "SharedMux"
  /\ wants[p] /\ ~conn[p] /\ ~asked[p]
  /\ asked' = [asked EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, path, wants, drops, forgets, closedByGossip>>

\* The endpoint refuses to dial a revoked peer, and tells gossip so.
Refuse(p) ==
  /\ asked[p] /\ ~admitted[p] /\ ~dialing[p]
  /\ asked' = [asked EXCEPT ![p] = FALSE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, path, wants, drops, forgets, closedByGossip>>

\* Our dialer starts a dial (because gossip asked, or for sync). It checks
\* admission before connecting.
DialStart(p) ==
  /\ ~conn[p] /\ ~dialing[p]
  /\ admitted[p]
  /\ dialing' = [dialing EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, gconn, path, wants, asked, drops, forgets, closedByGossip>>

\* The dial connects. With Recheck the connection is kept only if the peer is
\* still admitted: a revoke may have landed while we were connecting. Either
\* way gossip hears how it went: a refused dial answers its ask, and so does
\* an open connection (see AskAfterOpen).
DialDone(p) ==
  /\ dialing[p]
  /\ dialing' = [dialing EXCEPT ![p] = FALSE]
  /\ conn' = [conn EXCEPT ![p] = IF RecheckMode = "Recheck" THEN admitted[p] ELSE TRUE]
  /\ asked' = [asked EXCEPT ![p] = conn'[p] /\ AskAfterOpen(p)]
  /\ UNCHANGED <<admitted, gconn, path, wants, drops, forgets, closedByGossip>>

\* The peer dials us; we accept only an admitted peer. Gossip hears of the
\* connection as it would of a dial it asked for.
Accept(p) ==
  /\ ~conn[p]
  /\ admitted[p]
  /\ conn' = [conn EXCEPT ![p] = TRUE]
  /\ asked' = [asked EXCEPT ![p] = AskAfterOpen(p)]
  /\ UNCHANGED <<admitted, dialing, gconn, path, wants, drops, forgets, closedByGossip>>

\* Today's design only: iroh-gossip dials a peer it wants on its own
\* connection, with no admission check of ours.
GossipDial(p) ==
  /\ PathMode = "OwnConnection"
  /\ wants[p] /\ ~gconn[p]
  /\ gconn' = [gconn EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, path, wants, asked, drops, forgets, closedByGossip>>

\* Gossip opens its send path to a peer it wants, on the connection it uses.
OpenPath(p) ==
  /\ wants[p] /\ ~path[p]
  /\ GossipConn(p)
  /\ path' = [path EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, wants, asked, drops, forgets, closedByGossip>>

\* The operator revokes a peer: it is barred, and every connection we hold to
\* it is closed in the same step (handle_deny_peer). Closing the connection
\* ends gossip's path on it.
Revoke(p) ==
  /\ p \in Revocable /\ admitted[p]
  /\ admitted' = [admitted EXCEPT ![p] = FALSE]
  /\ conn' = [conn EXCEPT ![p] = FALSE]
  /\ gconn' = [gconn EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = FALSE]
  /\ UNCHANGED <<dialing, wants, asked, drops, forgets, closedByGossip>>

\* The network drops a connection. Gossip's path on it ends with it.
Drop(p) ==
  /\ drops < MaxDrops
  /\ conn[p]
  /\ conn' = [conn EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = IF PathMode = "SharedMux" THEN FALSE ELSE @]
  /\ drops' = drops + 1
  /\ UNCHANGED <<admitted, dialing, gconn, wants, asked, forgets, closedByGossip>>

Next ==
  \E p \in Peers :
    \/ Learn(p) \/ Forget(p) \/ DialStart(p) \/ DialDone(p) \/ Accept(p)
    \/ GossipDial(p) \/ OpenPath(p) \/ Revoke(p) \/ Drop(p)
    \/ Ask(p) \/ Refuse(p)

\* The scheduler is fair to the steps that move gossip toward a peer: if one
\* stays possible, it is eventually taken.
Fairness ==
  \A p \in Peers :
    /\ WF_vars(Learn(p))
    /\ WF_vars(DialStart(p))
    /\ WF_vars(DialDone(p))
    /\ WF_vars(GossipDial(p))
    /\ WF_vars(OpenPath(p))
    /\ WF_vars(Refuse(p))

Spec == Init /\ [][Next]_vars /\ Fairness

INV_TypeOK == TypeOK

\* A revoked peer has no gossip: no send path and no gossip connection.
INV_RevokedHasNoGossip ==
  \A p \in Peers : ~admitted[p] => (~path[p] /\ ~gconn[p])

\* Gossip only holds a send path on a connection that is still open, so it
\* can never look connected to a peer it cannot reach.
INV_PathOnLiveConnection ==
  \A p \in Peers : path[p] => GossipConn(p)

\* Gossip never closes a connection other protocols share.
INV_GossipNeverClosesShared == ~closedByGossip

\* Gossip eventually reaches the healthy peer, whatever the revoked peers
\* and the network do, once it has stopped dropping that peer.
LIVE_HealthyReached == <>(path[Healthy])

\* Every dial gossip asks for is answered, so gossip never waits on a peer
\* for good.
LIVE_AskAnswered == \A p \in Peers : asked[p] ~> ~asked[p]
====
