---- MODULE ActivePeersIdentify ----
\* The story, in plain words.
\*
\* A peer dials our node. Our dials, and theirs, come from a fresh source
\* port that nothing listens on, so the first address the host records for a
\* peer that dialed in is that source port. The peer's Identify, which comes a
\* moment later, names the address it listens on, and the host records that
\* instead. Meanwhile a caller may ask ActivePeers for the addresses of the
\* connected peers.
\*
\* Who acts: the peer (it connects, its Identify arrives or fails, it may
\* hang up), the caller (it asks ActivePeers), and the host (it answers).
\*
\* What must never happen:
\*   1. ActivePeers lists a peer at its source port while the peer's Identify
\*      may still come - INV_ListenAddress;
\*   2. a caller waits forever for its answer - LIVE_Answered.
\*
\* Correspondence with the Rust code (crates/p2p/src/host):
\*
\*   | Model            | Rust                                                     |
\*   |------------------|----------------------------------------------------------|
\*   | Connect          | p2p_host/swarm.rs handle_swarm_event, ConnectionEstablished|
\*   |                  | on the Listener side: peer_addrs = send_back_addr, the   |
\*   |                  | peer joins `unidentified`                                |
\*   | Identified       | p2p_host/protocols.rs identify Event::Received: the      |
\*   |                  | listen address replaces it, the peer leaves              |
\*   |                  | `unidentified`, answer_peer_addresses                    |
\*   | IdentifyFails    | identify Event::Error (its own timeout included): the    |
\*   |                  | source port stays, the peer leaves `unidentified`        |
\*   | HangUp           | swarm.rs handle_connection_closed, last connection       |
\*   | Ask              | command_handler/network.rs handle_peer_addresses: the    |
\*   |                  | reply joins pending_peer_addresses                       |
\*   | Answer           | answer_peer_addresses: sent once no connected peer is in |
\*   |                  | `unidentified` (RED WaitForIdentify = FALSE: at once, as |
\*   |                  | before #2774)                                            |
\*
\* Gaps, explained:
\*   * One peer and one caller show it: each peer and each call is handled
\*     the same way and apart from the others.
\*   * A peer we dialed is left out: its recorded address is the one we
\*     dialed, its listen address, from the start.

CONSTANT WaitForIdentify
\* TRUE as written: the answer waits for the peer's Identify. A RED
\* configuration sets it FALSE and answers at once.

ASSUME WaitForIdentify \in BOOLEAN
\* The guard is on or off.

VARIABLES peer, addr, asked, answer, answeredAt
\* peer: "away", "connected" (no Identify yet), "identified", "failed"
\* (its Identify failed), or "gone" (hung up). addr: the address recorded
\* for it ("none", the "source" port, or its "listen" address). asked: the
\* caller has asked. answer: what the caller got ("none" yet, an empty
\* list, or the address it was listed at). answeredAt: the peer's state when
\* the host answered ("none" before).

vars == <<peer, addr, asked, answer, answeredAt>>
\* Every variable, for the stuttering steps.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ peer \in {"away", "connected", "identified", "failed", "gone"}
  /\ addr \in {"none", "source", "listen"}
  /\ asked \in BOOLEAN
  /\ answer \in {"none", "empty", "source", "listen"}
  /\ answeredAt \in {"none", "away", "connected", "identified", "failed", "gone"}

Init ==
  \* No connection yet, and nobody has asked.
  /\ peer = "away"
  /\ addr = "none"
  /\ asked = FALSE
  /\ answer = "none"
  /\ answeredAt = "none"

Connect ==
  \* The peer dials in: the host records its source port.
  /\ peer = "away"
  /\ peer' = "connected"
  /\ addr' = "source"
  /\ UNCHANGED <<asked, answer, answeredAt>>

Identified ==
  \* The peer's Identify arrives: its listen address replaces the port.
  /\ peer = "connected"
  /\ peer' = "identified"
  /\ addr' = "listen"
  /\ UNCHANGED <<asked, answer, answeredAt>>

IdentifyFails ==
  \* The peer's Identify fails or times out: the port is all there is.
  /\ peer = "connected"
  /\ peer' = "failed"
  /\ UNCHANGED <<addr, asked, answer, answeredAt>>

HangUp ==
  \* The peer's last connection closes; the host forgets its address.
  /\ peer \in {"connected", "identified", "failed"}
  /\ peer' = "gone"
  /\ addr' = "none"
  /\ UNCHANGED <<asked, answer, answeredAt>>

Ask ==
  \* The caller asks ActivePeers, once.
  /\ ~asked
  /\ asked' = TRUE
  /\ UNCHANGED <<peer, addr, answer, answeredAt>>

Listed ==
  \* What the host lists: the recorded address of a connected peer.
  IF peer \in {"connected", "identified", "failed"} THEN addr ELSE "empty"

Answer ==
  \* The host answers, unless it waits on the peer's Identify (RED: never).
  /\ asked
  /\ answer = "none"
  /\ WaitForIdentify => peer # "connected"
  /\ answer' = Listed
  /\ answeredAt' = peer
  /\ UNCHANGED <<peer, addr, asked>>

Next == Connect \/ Identified \/ IdentifyFails \/ HangUp \/ Ask \/ Answer
\* Each step is the peer's, the caller's or the host's.

Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(Answer)
        /\ WF_vars(Identified \/ IdentifyFails)
\* The host answers when it can, and a pending Identify ends, one way or the
\* other (libp2p times it out).

INV_ListenAddress == answer = "source" => answeredAt = "failed"
\* Promise: a peer is listed at its source port only once its Identify
\* failed, never while the Identify that names its listen address may come.

LIVE_Answered == asked ~> answer # "none"
\* Promise: every caller that asks gets an answer.

====
