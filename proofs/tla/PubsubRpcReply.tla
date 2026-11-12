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
\* Who acts: the asker, its peers (each one answers), gossip (which picks the
\* path of each first copy), and the clock (the asker's deadline).
\*
\* What must never happen:
\*   1. an answer goes to a reply topic the asker does not listen on, and is
\*      lost - INV_NoAnswerLost;
\*   2. the asker stops waiting without an answer from every peer while every
\*      peer has answered - INV_AllAnswered;
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
\*   | got, lost         | the asker's correlator receiving the envelope on its own    |
\*   |                   | `_response` topic, or another peer's dropping it            |
\*   | Finish, Deadline  | the reply loop's `expected_responses` count and its         |
\*   |                   | deadline (pubsub_sync_branchable_collection)                |
\*
\* Gaps, explained:
\*   * One request. Each request is answered on its own, so one shows them all.
\*   * Every peer holds the topic and answers; a peer that fails to answer is
\*     the deadline's case, which the model keeps.
\*   * The reply itself travels straight to its topic's listener: the asker is
\*     connected to every peer it counts (expected_responses is its topic
\*     peers), so a reply on its topic reaches it.

CONSTANTS Asker, Peers, AnswerPublisher
\* Asker is the node that asks; Peers are the nodes that answer. When
\* AnswerPublisher is TRUE a peer answers the request's publisher (the code
\* as written); a RED configuration sets it FALSE, and a peer answers the
\* neighbour that handed it the request.

ASSUME /\ Asker \notin Peers
       \* the asker is not one of its own answering peers;
       /\ AnswerPublisher \in BOOLEAN
       \* and the guard is on or off.

VARIABLES via, answered, got, lost, done
\* via: for each peer, who handed it the request ("none" yet, the asker, or
\* the relaying peer). answered: the peers that answered. got: the peers
\* whose answer reached the asker. lost: the peers whose answer went to a
\* topic no one waits on. done: how the asker stopped waiting ("no" yet,
\* "all" answered, or the "deadline").

vars == <<via, answered, got, lost, done>>
\* Every variable, for the stuttering steps.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ via \in [Peers -> {"none", Asker} \cup Peers]
  \* each peer got the request from nobody yet, the asker, or a peer;
  /\ answered \subseteq Peers
  \* the answered peers are peers;
  /\ got \subseteq Peers /\ lost \subseteq Peers
  \* answers that arrived or were lost are from peers;
  /\ done \in {"no", "all", "deadline"}
  \* and the asker is waiting, or stopped for one of two reasons.

Init ==
  \* The asker has just published: no peer has the request yet.
  /\ via = [p \in Peers |-> "none"]
  /\ answered = {}
  /\ got = {}
  /\ lost = {}
  /\ done = "no"

DeliverDirect(p) ==
  \* Peer p gets its first copy straight from the asker.
  /\ via[p] = "none"
  /\ via' = [via EXCEPT ![p] = Asker]
  /\ UNCHANGED <<answered, got, lost, done>>

DeliverRelayed(p, r) ==
  \* Peer p gets its first copy from peer r, which already had it and passes
  \* it on before the asker's own copy reaches p.
  /\ via[p] = "none"
  /\ r # p
  /\ via[r] # "none"
  /\ via' = [via EXCEPT ![p] = r]
  /\ UNCHANGED <<answered, got, lost, done>>

ReplyTo(p) == IF AnswerPublisher THEN Asker ELSE via[p]
\* The peer whose reply topic p answers on: the publisher, the asker (RED:
\* whoever handed p the request).

Answer(p) ==
  \* While the asker waits, peer p answers once, on ReplyTo(p)'s reply
  \* topic. Only the asker listens on its own; any other peer drops it.
  /\ done = "no"
  \* the asker still waits (a later answer changes nothing);
  /\ via[p] # "none"
  \* p has the request;
  /\ p \notin answered
  \* p has not answered yet;
  /\ answered' = answered \cup {p}
  \* p answers;
  /\ IF ReplyTo(p) = Asker
     THEN /\ got' = got \cup {p}
          \* on the asker's topic: the answer arrives;
          /\ UNCHANGED lost
     ELSE /\ lost' = lost \cup {p}
          \* on another peer's topic: that peer drops it;
          /\ UNCHANGED got
  /\ UNCHANGED <<via, done>>
  \* and nothing else changes.

Finish ==
  \* The asker has an answer from every peer, and stops waiting.
  /\ done = "no"
  /\ got = Peers
  /\ done' = "all"
  /\ UNCHANGED <<via, answered, got, lost>>

Deadline ==
  \* The asker's deadline passes before every answer arrived; it goes on
  \* with what it has. It can pass at any moment while it waits.
  /\ done = "no"
  /\ got # Peers
  /\ done' = "deadline"
  /\ UNCHANGED <<via, answered, got, lost>>

Next ==
  \* Each step is gossip's, a peer's, or the asker's.
  \/ \E p \in Peers : DeliverDirect(p)
  \/ \E p, r \in Peers : DeliverRelayed(p, r)
  \/ \E p \in Peers : Answer(p)
  \/ Finish
  \/ Deadline

Fair ==
  \* Gossip delivers, peers answer, and the asker stops once it can.
  /\ \A p \in Peers : WF_vars(DeliverDirect(p))
  /\ \A p \in Peers : WF_vars(Answer(p))
  /\ WF_vars(Finish)

Spec == Init /\ [][Next]_vars /\ Fair
\* The run starts at Init, takes Next steps, and is fair as above. The
\* deadline is never forced: a run may wait for every answer.

INV_NoAnswerLost == lost = {}
\* Promise: every answer a peer sends reaches the asker.

INV_AllAnswered == done = "deadline" => answered # Peers
\* Promise: the asker gives up at its deadline only while some peer has not
\* answered, never with every peer answered and an answer missing.

LIVE_Done == <>(done # "no")
\* Promise: the asker stops waiting.

LIVE_AllArrive == <>(got = Peers \/ done = "deadline")
\* Promise: unless the deadline cuts it short, every answer arrives.

====
