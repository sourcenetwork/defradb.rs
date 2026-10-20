---- MODULE GossipOwnership ----
\* The story: our node gossips with its peers. Gossip's state machine learns
\* about peers from other peers, so it can want to talk to anyone, including
\* a peer we have revoked. Gossip rides the one mux connection we keep per
\* peer: it never opens or closes a connection itself. When it wants a peer
\* it has no connection to, it asks for the normal dial, which checks that the
\* peer is admitted before and after connecting.
\*
\* Who acts: gossip (learning and forgetting peers), our dialer, peers dialing
\* us, the operator revoking a peer, and the network dropping connections.
\*
\* What must never happen: gossip reaches a revoked peer, gossip runs on a
\* connection that is gone, or gossip closes a connection other protocols
\* share. What must always happen: a healthy peer gossip wants is reached.
\*
\* Code it models (crates/p2p/src/iroh):
\*   gossip/driver.rs         GossipDriver        (one send path per connected peer)
\*   endpoint_rpc.rs          connect_with_cache  (admission before and after dialing)
\*   endpoint_streams.rs      handle_incoming     (admission on accept)
\*   endpoint_commands.rs     handle_deny_peer    (revoke, then close the connection)
EXTENDS Naturals

CONSTANTS
  Peers,          \* the peers in the run
  Revocable,      \* peers the operator may revoke
  Healthy,        \* a peer that is never revoked: the one we promise to reach
  MaxDrops,       \* how many times the network may drop a connection
  PathMode,       \* "SharedMux" (gossip rides our connection) | "OwnConnection" (today: gossip dials its own)
  RecheckMode,    \* "Recheck" (admission checked again once connected) | "None"
  ForgetMode      \* "DropSender" (gossip forgets a peer, connection stays) | "CloseConnection"

\* The healthy peer is a real peer and is never revoked.
ASSUME Healthy \in Peers /\ Healthy \notin Revocable
\* Only real peers can be revoked.
ASSUME Revocable \subseteq Peers
ASSUME MaxDrops \in Nat
ASSUME PathMode \in {"SharedMux", "OwnConnection"}
ASSUME RecheckMode \in {"Recheck", "None"}
ASSUME ForgetMode \in {"DropSender", "CloseConnection"}

VARIABLES
  admitted,        \* [Peers -> BOOLEAN] the peer may talk to us
  conn,            \* [Peers -> BOOLEAN] our mux connection to the peer is open
  dialing,         \* [Peers -> BOOLEAN] a dial passed its first check and is connecting
  gconn,           \* [Peers -> BOOLEAN] gossip's own connection (OwnConnection only)
  path,            \* [Peers -> BOOLEAN] gossip holds a send path to the peer
  wants,           \* [Peers -> BOOLEAN] gossip wants to talk to the peer
  drops,           \* how many connections the network has dropped so far
  closedByGossip   \* gossip once closed a connection other protocols share

vars == <<admitted, conn, dialing, gconn, path, wants, drops, closedByGossip>>

\* The connection gossip sends on: ours when it rides the mux, its own otherwise.
GossipConn(p) == IF PathMode = "SharedMux" THEN conn[p] ELSE gconn[p]

TypeOK ==
  /\ admitted \in [Peers -> BOOLEAN]
  /\ conn \in [Peers -> BOOLEAN]
  /\ dialing \in [Peers -> BOOLEAN]
  /\ gconn \in [Peers -> BOOLEAN]
  /\ path \in [Peers -> BOOLEAN]
  /\ wants \in [Peers -> BOOLEAN]
  /\ drops \in 0..MaxDrops
  /\ closedByGossip \in BOOLEAN

\* Every peer starts admitted and unknown to gossip, with nothing open.
Init ==
  /\ admitted = [p \in Peers |-> TRUE]
  /\ conn = [p \in Peers |-> FALSE]
  /\ dialing = [p \in Peers |-> FALSE]
  /\ gconn = [p \in Peers |-> FALSE]
  /\ path = [p \in Peers |-> FALSE]
  /\ wants = [p \in Peers |-> FALSE]
  /\ drops = 0
  /\ closedByGossip = FALSE

\* Gossip hears about a peer from another peer. It may be any peer, revoked
\* or not: membership comes from outside and we cannot filter it.
Learn(p) ==
  /\ ~wants[p]
  /\ wants' = [wants EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, path, drops, closedByGossip>>

\* Gossip forgets a peer (iroh-gossip's DisconnectPeer). Our driver only
\* drops its sender; the CloseConnection bug also closes the shared
\* connection. The healthy peer is never forgotten, so the promise to reach
\* it is not undone by gossip itself.
Forget(p) ==
  /\ p # Healthy
  /\ wants[p]
  /\ wants' = [wants EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = FALSE]
  /\ IF ForgetMode = "CloseConnection" /\ conn[p]
     THEN /\ conn' = [conn EXCEPT ![p] = FALSE]
          /\ closedByGossip' = TRUE
     ELSE UNCHANGED <<conn, closedByGossip>>
  /\ UNCHANGED <<admitted, dialing, gconn, drops>>

\* Our dialer starts a dial (because gossip asked, or for sync). It checks
\* admission before connecting.
DialStart(p) ==
  /\ ~conn[p] /\ ~dialing[p]
  /\ admitted[p]
  /\ dialing' = [dialing EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, gconn, path, wants, drops, closedByGossip>>

\* The dial connects. With Recheck the connection is kept only if the peer is
\* still admitted: a revoke may have landed while we were connecting.
DialDone(p) ==
  /\ dialing[p]
  /\ dialing' = [dialing EXCEPT ![p] = FALSE]
  /\ conn' = [conn EXCEPT ![p] = IF RecheckMode = "Recheck" THEN admitted[p] ELSE TRUE]
  /\ UNCHANGED <<admitted, gconn, path, wants, drops, closedByGossip>>

\* The peer dials us; we accept only an admitted peer.
Accept(p) ==
  /\ ~conn[p]
  /\ admitted[p]
  /\ conn' = [conn EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, dialing, gconn, path, wants, drops, closedByGossip>>

\* Today's design only: iroh-gossip dials a peer it wants on its own
\* connection, with no admission check of ours.
GossipDial(p) ==
  /\ PathMode = "OwnConnection"
  /\ wants[p] /\ ~gconn[p]
  /\ gconn' = [gconn EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, path, wants, drops, closedByGossip>>

\* Gossip opens its send path to a peer it wants, on the connection it uses.
OpenPath(p) ==
  /\ wants[p] /\ ~path[p]
  /\ GossipConn(p)
  /\ path' = [path EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<admitted, conn, dialing, gconn, wants, drops, closedByGossip>>

\* The operator revokes a peer: it is barred, and every connection we hold to
\* it is closed in the same step (handle_deny_peer). Closing the connection
\* ends gossip's path on it.
Revoke(p) ==
  /\ p \in Revocable /\ admitted[p]
  /\ admitted' = [admitted EXCEPT ![p] = FALSE]
  /\ conn' = [conn EXCEPT ![p] = FALSE]
  /\ gconn' = [gconn EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = FALSE]
  /\ UNCHANGED <<dialing, wants, drops, closedByGossip>>

\* The network drops a connection. Gossip's path on it ends with it.
Drop(p) ==
  /\ drops < MaxDrops
  /\ conn[p]
  /\ conn' = [conn EXCEPT ![p] = FALSE]
  /\ path' = [path EXCEPT ![p] = IF PathMode = "SharedMux" THEN FALSE ELSE @]
  /\ drops' = drops + 1
  /\ UNCHANGED <<admitted, dialing, gconn, wants, closedByGossip>>

Next ==
  \E p \in Peers :
    \/ Learn(p) \/ Forget(p) \/ DialStart(p) \/ DialDone(p) \/ Accept(p)
    \/ GossipDial(p) \/ OpenPath(p) \/ Revoke(p) \/ Drop(p)

\* The scheduler is fair to the steps that move gossip toward a peer: if one
\* stays possible, it is eventually taken.
Fairness ==
  \A p \in Peers :
    /\ WF_vars(Learn(p))
    /\ WF_vars(DialStart(p))
    /\ WF_vars(DialDone(p))
    /\ WF_vars(GossipDial(p))
    /\ WF_vars(OpenPath(p))

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
\* and the network do.
LIVE_HealthyReached == <>(path[Healthy])
====
