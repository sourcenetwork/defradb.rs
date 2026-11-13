---- MODULE ActivePeersIdentify ----
\* The story, in plain words.
\*
\* Peers dial our node. Our dials, and theirs, come from a fresh source
\* port that nothing listens on, so the first address the host records for a
\* peer that dialed in is that source port. The peer's Identify, which comes a
\* moment later, names the address it listens on, and the host records that
\* instead. Meanwhile a caller may ask ActivePeers for the addresses of the
\* connected peers.
\*
\* Who acts: each peer (it connects, its Identify arrives or fails, it may
\* hang up), the caller (it asks ActivePeers), and the host (it answers).
\*
\* What must never happen:
\*   1. ActivePeers lists a peer that was waiting on its Identify when it was
\*      asked at its source port while that Identify may still come -
\*      INV_ListenAddress;
\*   2. the answer is held by a peer that connected after the call: once
\*      every peer waiting at the call has settled, the host can answer -
\*      INV_NotHeldByLater;
\*   3. a caller waits forever for its answer - LIVE_Answered.
\*
\* Correspondence with the Rust code (crates/p2p/src/host):
\*
\*   | Model            | Rust                                                     |
\*   |------------------|----------------------------------------------------------|
\*   | Connect(p)       | p2p_host/swarm.rs handle_swarm_event, ConnectionEstablished|
\*   |                  | on the Listener side: peer_addrs = send_back_addr, the   |
\*   |                  | peer joins `unidentified`                                |
\*   | Identified(p)    | p2p_host/protocols.rs identify Event::Received: the      |
\*   |                  | listen address replaces it, on_identify_settled          |
\*   | IdentifyFails(p) | identify Event::Error (its own timeout included): the    |
\*   |                  | source port stays, on_identify_settled                   |
\*   | HangUp(p)        | swarm.rs handle_connection_closed, last connection:      |
\*   |                  | on_identify_settled                                      |
\*   | Ask              | command_handler/network.rs handle_peer_addresses: the    |
\*   |                  | reply is held with the connected unidentified peers      |
\*   |                  | (waitsOn; RED SnapshotAtAsk = FALSE: waits on every      |
\*   |                  | connected unidentified peer, later ones included)        |
\*   | Answer           | answer_peer_addresses: sent once its waitsOn is empty    |
\*   |                  | (RED WaitForIdentify = FALSE: at once, as before #2774)  |
\*
\* Gaps, explained:
\*   * Two peers and one caller show it: a second peer can connect after
\*     the call, which is how a later peer could hold it. Each call keeps its
\*     own waitsOn, so a second caller is handled the same way.
\*   * A peer that connects after the call is listed at whatever address it
\*     has when the answer goes out, its source port included.
\*   * A peer we dialed is left out: its recorded address is the one we
\*     dialed, its listen address, from the start.
\*   * A peer connects once: a reconnect is a new Connect of the same kind.

CONSTANTS Peers, WaitForIdentify, SnapshotAtAsk
\* Peers: the peers that may dial in (two in every configuration).
\* WaitForIdentify: TRUE as written, the answer waits for Identify; the RED
\*   AnswerAtOnce sets it FALSE and answers at once.
\* SnapshotAtAsk: TRUE as written, the answer waits only on the peers that
\*   were unidentified at the call; the RED WaitOnLater sets it FALSE and
\*   waits on every connected unidentified peer, as the first version did.

ASSUME WaitForIdentify \in BOOLEAN /\ SnapshotAtAsk \in BOOLEAN
\* Both guards are on or off.

VARIABLES peer, addr, asked, waitsOn, atAsk, answer, answeredAt
\* peer[p]: "away", "connected" (no Identify yet), "identified", "failed"
\*   (its Identify failed), or "gone" (hung up).
\* addr[p]: the address recorded for p ("none", the "source" port, or its
\*   "listen" address).
\* asked: the caller has asked.
\* waitsOn: the peers the held answer still waits on.
\* atAsk: the peers that were waiting on their Identify when the caller asked.
\* answer[p]: how the answer listed p ("none" before the answer, "absent" if
\*   not listed, else the address it was listed at).
\* answeredAt[p]: p's state when the host answered ("none" before).

vars == <<peer, addr, asked, waitsOn, atAsk, answer, answeredAt>>
\* Every variable, for the stuttering steps.

States == {"away", "connected", "identified", "failed", "gone"}
\* The states a peer can be in.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ peer \in [Peers -> States]
  /\ addr \in [Peers -> {"none", "source", "listen"}]
  /\ asked \in BOOLEAN
  /\ waitsOn \subseteq Peers
  /\ atAsk \subseteq Peers
  /\ answer \in [Peers -> {"none", "absent", "source", "listen"}]
  /\ answeredAt \in [Peers -> States \cup {"none"}]

Answered == \A p \in Peers : answer[p] # "none"
\* The host has answered (every peer has its entry in the answer).

Init ==
  \* No connection yet, and nobody has asked.
  /\ peer = [p \in Peers |-> "away"]
  /\ addr = [p \in Peers |-> "none"]
  /\ asked = FALSE
  /\ waitsOn = {}
  /\ atAsk = {}
  /\ answer = [p \in Peers |-> "none"]
  /\ answeredAt = [p \in Peers |-> "none"]

Settle(p) ==
  \* p's Identify ended one way or the other, or p hung up: no held answer
  \* waits on it any more.
  waitsOn' = waitsOn \ {p}

Connect(p) ==
  \* Peer p dials in: the host records its source port.
  /\ peer[p] = "away"
  /\ peer' = [peer EXCEPT ![p] = "connected"]
  /\ addr' = [addr EXCEPT ![p] = "source"]
  /\ UNCHANGED <<asked, waitsOn, atAsk, answer, answeredAt>>

Identified(p) ==
  \* p's Identify arrives: its listen address replaces the port.
  /\ peer[p] = "connected"
  /\ peer' = [peer EXCEPT ![p] = "identified"]
  /\ addr' = [addr EXCEPT ![p] = "listen"]
  /\ Settle(p)
  /\ UNCHANGED <<asked, atAsk, answer, answeredAt>>

IdentifyFails(p) ==
  \* p's Identify fails or times out: the port is all there is.
  /\ peer[p] = "connected"
  /\ peer' = [peer EXCEPT ![p] = "failed"]
  /\ Settle(p)
  /\ UNCHANGED <<addr, asked, atAsk, answer, answeredAt>>

HangUp(p) ==
  \* p's last connection closes; the host forgets its address.
  /\ peer[p] \in {"connected", "identified", "failed"}
  /\ peer' = [peer EXCEPT ![p] = "gone"]
  /\ addr' = [addr EXCEPT ![p] = "none"]
  /\ Settle(p)
  /\ UNCHANGED <<asked, atAsk, answer, answeredAt>>

Waiting == {p \in Peers : peer[p] = "connected"}
\* The connected peers still waiting on their Identify.

Ask ==
  \* The caller asks ActivePeers, once; the answer is held with the peers
  \* waiting on their Identify right now.
  /\ ~asked
  /\ asked' = TRUE
  /\ waitsOn' = Waiting
  /\ atAsk' = Waiting
  /\ UNCHANGED <<peer, addr, answer, answeredAt>>

CanAnswer ==
  \* Whether the host lets the held answer go: at once (RED AnswerAtOnce),
  \* once its own peers settled, or (RED WaitOnLater) once no connected peer
  \* at all is waiting on its Identify.
  IF ~WaitForIdentify THEN TRUE
  ELSE IF SnapshotAtAsk THEN waitsOn = {}
  ELSE Waiting = {}

Listed(p) ==
  \* How the host lists p: its recorded address if connected, else absent.
  IF peer[p] \in {"connected", "identified", "failed"} THEN addr[p] ELSE "absent"

Answer ==
  \* The host answers the caller.
  /\ asked
  /\ ~Answered
  /\ CanAnswer
  /\ answer' = [p \in Peers |-> Listed(p)]
  /\ answeredAt' = peer
  /\ UNCHANGED <<peer, addr, asked, waitsOn, atAsk>>

Next ==
  \* Each step is a peer's, the caller's or the host's.
  \/ \E p \in Peers : Connect(p) \/ Identified(p) \/ IdentifyFails(p) \/ HangUp(p)
  \/ Ask
  \/ Answer

Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(Answer)
        /\ \A p \in Peers : WF_vars(Identified(p) \/ IdentifyFails(p))
\* The host answers when it can, and a pending Identify ends, one way or the
\* other (libp2p times it out).

INV_ListenAddress ==
  \* Promise: a peer that was waiting on its Identify at the call is listed at
  \* its source port only once that Identify failed.
  \A p \in atAsk : answer[p] = "source" => answeredAt[p] = "failed"

INV_NotHeldByLater ==
  \* Promise: once every peer that was waiting at the call has settled, the
  \* host can answer; a peer that connected later never holds the answer.
  (asked /\ ~Answered /\ \A p \in atAsk : peer[p] # "connected") => CanAnswer

LIVE_Answered == asked ~> Answered
\* Promise: every caller that asks gets an answer.

====
