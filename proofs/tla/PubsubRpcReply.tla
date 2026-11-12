---- MODULE PubsubRpcReply ----
\* The story, in plain words.
\*
\* A node asks its peers a question over gossip: it publishes a request on a
\* shared topic (doc-sync, sync-branchable, encryption) and waits on its own
\* reply topic, `<topic>/<itself>/_response`, for one answer from each peer
\* that holds the topic. Gossip delivers the request to every peer once: the
\* first copy that arrives counts, and later copies are dropped as already
\* seen. That first copy may come straight from the asker or be relayed by a
\* third peer, the relay, that got it first. A peer answers on the reply topic
\* of the peer it takes for the asker. Only the asker listens on its reply
\* topic; an answer on anyone else's reply topic is thrown away, as an answer
\* to a question that peer never asked.
\*
\* A tiny example. The asker A publishes; the relay B gets it and passes it
\* on to C before A's own copy reaches C. If C answers to the peer that handed
\* it the copy, it answers on B's reply topic, B drops it, and A waits until
\* its deadline and goes on without C's heads.
\*
\* An answer takes time to travel. If the asker's deadline passes while an
\* answer is still on its way, the asker has stopped reading and the answer
\* is dropped; that is the deadline doing its job, not a lost answer.
\*
\* Who acts: the asker, its peers (each one answers), gossip (which picks the
\* path of each first copy and carries each answer), and the clock (the
\* asker's deadline).
\*
\* What must never happen:
\*   1. an answer goes to a reply topic the asker does not listen on, and is
\*      lost - INV_NoAnswerLost;
\*   2. the asker stops at its deadline when no answer is missing or still
\*      on its way - INV_AllAnswered;
\*   3. the asker waits forever - LIVE_Done.
\*
\* Correspondence with the Rust code:
\*
\*   | Model             | Rust                                                        |
\*   |-------------------|-------------------------------------------------------------|
\*   | Publish           | SyncCoordinator::pubsub_sync_branchable_collection and the  |
\*   |                   | doc-sync caller (crates/p2p/src/sync/coordinator/           |
\*   |                   | pubsub_client.rs): publish_raw on the base topic            |
\*   | Deliver(p, how)   | gossipsub::Event::Message, emitted for the first copy only  |
\*   |                   | (crates/p2p/src/host/p2p_host/protocols.rs                  |
\*   |                   | handle_gossipsub_event)                                     |
\*   | via[p]            | that event's propagation_source: the asker when "direct",   |
\*   |                   | the relay when "relayed"                                    |
\*   | Answer(p)         | TopicHandler::deliver_gossip_message (crates/p2p/src/       |
\*   |                   | pubsub_rpc/topic_handler.rs): the reply goes to             |
\*   |                   | response_topic(base, from)                                  |
\*   | AnswerPublisher   | `from` is HostEvent::GossipRawMessage's `source`, the       |
\*   |                   | message's signed author (protocols.rs, then                 |
\*   |                   | handle_gossip_raw_message in crates/p2p/src/sync/           |
\*   |                   | coordinator/event_handler/pubsub_raw.rs); RED: the          |
\*   |                   | propagation_source, as before #2773                         |
\*   | inflight          | a reply published but not yet read by the reply loop's      |
\*   |                   | `prep.responses.recv()`                                     |
\*   | Arrive(p)         | that read, on the asker's own `_response` topic (got), or   |
\*   |                   | another peer's dropping the envelope (lost)                 |
\*   | Finish, Deadline  | the reply loop's `expected_responses` count and its         |
\*   |                   | deadline (pubsub_sync_branchable_collection); at the        |
\*   |                   | deadline the loop ends and dropping `prep` cancels the      |
\*   |                   | correlation, so a reply still in flight is never read       |
\*
\* Gaps, explained:
\*   * One request. Each request is answered on its own, so one shows them all.
\*   * Every peer holds the topic and answers; a peer that fails to answer is
\*     the deadline's case, which the model keeps.
\*   * A reply on the asker's topic reaches the asker unless the deadline
\*     comes first: the asker is connected to every peer it counts
\*     (expected_responses is its topic peers).

CONSTANTS Asker, Peers, AnswerPublisher
\* Asker is the node that asks; Peers are the nodes that answer. When
\* AnswerPublisher is TRUE a peer answers the request's publisher (the code
\* as written); a RED configuration sets it FALSE, and a peer answers the
\* neighbour that handed it the request.

ASSUME /\ Asker \notin Peers
       \* the asker is not one of its own answering peers;
       /\ AnswerPublisher \in BOOLEAN
       \* and the guard is on or off.

VARIABLES via, answered, inflight, got, lost, done
\* via: for each peer, who handed it the request ("none" yet, the asker, or
\* the relaying peer). answered: the peers that sent an answer. inflight:
\* the peers whose answer is still on its way. got: the peers whose answer
\* reached the asker. lost: the peers whose answer went to a topic no one
\* waits on. done: how the asker stopped waiting ("no" yet, "all" answered,
\* or the "deadline").

vars == <<via, answered, inflight, got, lost, done>>
\* Every variable, for the stuttering steps.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ via \in [Peers -> {"none", Asker} \cup Peers]
  \* each peer got the request from nobody yet, the asker, or a peer;
  /\ answered \subseteq Peers
  \* the peers that answered are peers;
  /\ inflight \subseteq answered
  \* an answer on its way was sent by a peer that answered;
  /\ got \subseteq Peers /\ lost \subseteq Peers
  \* answers that arrived or were lost are from peers;
  /\ done \in {"no", "all", "deadline"}
  \* and the asker is waiting, or stopped for one of two reasons.

Init ==
  \* The asker has just published: no peer has the request yet.
  /\ via = [p \in Peers |-> "none"]
  \* no peer has received a copy;
  /\ answered = {}
  \* no peer has answered;
  /\ inflight = {}
  \* no answer is on its way;
  /\ got = {}
  \* the asker has no answer yet;
  /\ lost = {}
  \* no answer has gone astray;
  /\ done = "no"
  \* and the asker is waiting.

DeliverDirect(p) ==
  \* Peer p gets its first copy straight from the asker.
  /\ via[p] = "none"
  \* p has no copy yet (later copies are dropped as already seen);
  /\ via' = [via EXCEPT ![p] = Asker]
  \* p records that the asker handed it the request;
  /\ UNCHANGED <<answered, inflight, got, lost, done>>
  \* and nothing else changes.

DeliverRelayed(p, r) ==
  \* Peer p gets its first copy from peer r, which already had it and passes
  \* it on before the asker's own copy reaches p.
  /\ via[p] = "none"
  \* p has no copy yet;
  /\ r # p
  \* the relay is some other peer;
  /\ via[r] # "none"
  \* the relay already has the request, so it can pass it on;
  /\ via' = [via EXCEPT ![p] = r]
  \* p records that r handed it the request;
  /\ UNCHANGED <<answered, inflight, got, lost, done>>
  \* and nothing else changes.

ReplyTo(p) == IF AnswerPublisher THEN Asker ELSE via[p]
\* The peer whose reply topic p answers on: the publisher, the asker (RED:
\* whoever handed p the request).

Answer(p) ==
  \* While the asker waits, peer p sends its answer once; it is now on its
  \* way to ReplyTo(p)'s reply topic.
  /\ done = "no"
  \* the asker still waits (a later answer changes nothing);
  /\ via[p] # "none"
  \* p has the request;
  /\ p \notin answered
  \* p has not answered yet;
  /\ answered' = answered \cup {p}
  \* p answers;
  /\ inflight' = inflight \cup {p}
  \* and its answer starts travelling;
  /\ UNCHANGED <<via, got, lost, done>>
  \* nothing else changes.

Arrive(p) ==
  \* While the asker still reads, p's answer lands on ReplyTo(p)'s reply
  \* topic. Only the asker listens on its own; any other peer drops it.
  /\ done = "no"
  \* the asker still reads its topic (after the deadline nothing is read);
  /\ p \in inflight
  \* p's answer is on its way;
  /\ inflight' = inflight \ {p}
  \* and now it has landed;
  /\ IF ReplyTo(p) = Asker
     \* if it went to the asker's topic,
     THEN /\ got' = got \cup {p}
          \* the answer arrives;
          /\ UNCHANGED lost
          \* and nothing is lost;
     ELSE /\ lost' = lost \cup {p}
          \* otherwise that peer drops it, and it is lost;
          /\ UNCHANGED got
          \* and the asker gets nothing from p;
  /\ UNCHANGED <<via, answered, done>>
  \* nothing else changes.

Finish ==
  \* The asker has an answer from every peer, and stops waiting.
  /\ done = "no"
  \* the asker still waits;
  /\ got = Peers
  \* every peer's answer has arrived;
  /\ done' = "all"
  \* so the asker stops, with every answer;
  /\ UNCHANGED <<via, answered, inflight, got, lost>>
  \* and nothing else changes.

Deadline ==
  \* The asker's deadline passes before every answer arrived; it goes on
  \* with what it has. It can pass at any moment while it waits.
  /\ done = "no"
  \* the asker still waits;
  /\ got # Peers
  \* some answer has not arrived;
  /\ done' = "deadline"
  \* so the asker stops at its deadline, and stops reading its topic;
  /\ UNCHANGED <<via, answered, inflight, got, lost>>
  \* an answer still on its way stays unread forever.

Next ==
  \* Each step is gossip's, a peer's, or the asker's.
  \/ \E p \in Peers : DeliverDirect(p)
  \* gossip hands a peer its first copy from the asker;
  \/ \E p, r \in Peers : DeliverRelayed(p, r)
  \* or from a relay;
  \/ \E p \in Peers : Answer(p)
  \* a peer sends its answer;
  \/ \E p \in Peers : Arrive(p)
  \* an answer lands;
  \/ Finish
  \* the asker stops with every answer;
  \/ Deadline
  \* or at its deadline.

Fair ==
  \* Gossip delivers, peers answer, answers land, and the asker stops once
  \* it can.
  /\ \A p \in Peers : WF_vars(DeliverDirect(p))
  \* the asker's own copy eventually reaches every peer that has none;
  /\ \A p \in Peers : WF_vars(Answer(p))
  \* a peer with the request eventually answers;
  /\ \A p \in Peers : WF_vars(Arrive(p))
  \* an answer on its way eventually lands;
  /\ WF_vars(Finish)
  \* and an asker with every answer eventually stops.

Spec == Init /\ [][Next]_vars /\ Fair
\* The run starts at Init, takes Next steps, and is fair as above. The
\* deadline is never forced: a run may wait for every answer.

\* Promise: every answer a peer sends reaches the asker's topic.
INV_NoAnswerLost == lost = {}
\* no answer ever went to a topic no one waits on.

\* Promise: the asker gives up at its deadline only while some answer is
\* missing or still on its way, never with an answer sent elsewhere.
INV_AllAnswered ==
  done = "deadline" => \E p \in Peers : p \notin answered \/ p \in inflight
  \* at the deadline, some peer has not answered yet or its answer has not
  \* landed.

\* Promise: the asker stops waiting.
LIVE_Done == <>(done # "no")
\* eventually the asker is not waiting.

\* Promise: unless the deadline cuts it short, every answer arrives.
LIVE_AllArrive == <>(got = Peers \/ done = "deadline")
\* eventually every answer has arrived, or the deadline has passed.

====
