---- MODULE PeerDisconnect ----
\* The story, in plain words.
\*
\* A node holds some connections to one peer. A caller asks the node's swarm
\* task to disconnect that peer and waits for the answer. The swarm task asks
\* libp2p to close every connection to the peer, and each one closes later,
\* one event at a time. Once the caller has its answer it may ask which
\* peers are connected (ActivePeers).
\*
\* What must never happen: the caller is told the disconnect is done while a
\* connection to the peer is still open, so an ActivePeers right after it
\* still lists the peer. Go's Disconnect closes the connections before it
\* returns, and net_info_disconnect_multiple_peers checks exactly that.
\*
\* The Rust code this models:
\*   - the Disconnect command, which holds the reply   crates/p2p/src/host/command_handler/network.rs handle_disconnect
\*   - the last connection closing, which sends it      crates/p2p/src/host/p2p_host/swarm.rs handle_connection_closed
\*   - the held replies                                 crates/p2p/src/host/p2p_host/mod.rs P2PHost::pending_disconnects
EXTENDS Naturals

CONSTANTS
  Conns,     \* how many connections to the peer are open at the start
  Bug        \* "None" for the real code; "EarlyAck" answers before they close

VARIABLES
  open,      \* connections to the peer still open
  closing,   \* TRUE once libp2p was asked to close them all
  waiting,   \* TRUE while the caller waits for its answer
  answered   \* TRUE once the caller has been told the disconnect is done

vars == <<open, closing, waiting, answered>>

TypeOK ==
  /\ open \in 0..Conns                                   \* never more connections than at the start
  /\ closing \in BOOLEAN                                 \* the close request is a truth value
  /\ waiting \in BOOLEAN                                 \* so is the caller waiting
  /\ answered \in BOOLEAN                                \* and so is the answer

Init ==
  /\ open = Conns                                        \* every connection is open
  /\ closing = FALSE                                     \* nothing asked yet
  /\ waiting = FALSE                                     \* no caller yet
  /\ answered = FALSE                                    \* no answer yet

\* The caller sends Disconnect. With nothing open the swarm answers at once;
\* otherwise it asks libp2p to close everything and keeps the reply, unless
\* RED EarlyAck answers straight away as the old code did.
Disconnect ==
  /\ ~waiting /\ ~answered                               \* one request
  /\ IF open = 0 \/ Bug = "EarlyAck"
       THEN /\ answered' = TRUE                          \* answered now
            /\ waiting' = FALSE                          \* nobody left waiting
       ELSE /\ waiting' = TRUE                           \* the reply is held
            /\ answered' = FALSE                         \* not answered yet
  /\ closing' = (open > 0)                               \* libp2p closes what is open
  /\ UNCHANGED open                                      \* nothing has closed yet

\* One connection closes. When it was the last one, a held reply is sent.
Close ==
  /\ closing /\ open > 0                                 \* only what libp2p was asked to close
  /\ open' = open - 1                                    \* one fewer open
  /\ IF open' = 0 /\ waiting
       THEN /\ answered' = TRUE                          \* the last close sends the reply
            /\ waiting' = FALSE                          \* the caller stops waiting
       ELSE UNCHANGED <<answered, waiting>>              \* still some open, or nobody waits
  /\ UNCHANGED closing                                   \* the close request stands

Next == Disconnect \/ Close

Spec == Init /\ [][Next]_vars /\ WF_vars(Close)

\* INV_AnsweredMeansClosed: once the caller is told the disconnect is done,
\* no connection to the peer is open, so ActivePeers does not list it.
INV_AnsweredMeansClosed == answered => open = 0

\* PROP_CallerAnswered: a caller that waits is answered in the end.
PROP_CallerAnswered == waiting ~> answered
====
