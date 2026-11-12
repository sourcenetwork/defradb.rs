---- MODULE VincTopicDelivery ----
\* The story, in plain words.
\*
\* Some nodes run vinc. Each names the same hub. A node joins a topic by
\* telling the hub; the hub then pushes every message published on the topic
\* to every node that joined it, and tells each joined node who else joined.
\* Nodes also hold peer sessions with one another (they dial each other).
\*
\* A gossip mesh, which libp2p and iroh run, only carries a message over peer
\* connections. The hub carries it to every member, connected or not. So the
\* endpoint hands the engine a message only while it holds a peer session:
\* a node with no peer is on no mesh, and takes nothing.
\*
\* A topic can be raw (the KMS key exchange): its messages are opaque bytes
\* and must never be decoded as a PushLog head hint. The endpoint marks a raw
\* topic before it joins it, so no message can arrive in between.
\*
\* The endpoint reports a peer as JOINED on a topic when vinc says a member of
\* the topic it has a session with appeared (vinc's neighbours), not when the
\* hub merely lists the peer as a member.
\*
\* A KMS reply is published on the requester's own topic, which the replier
\* never subscribed to, and it must not go out before the requester is known.
\* Only a node on a topic hears who is on it, so the replier joins the topic
\* for the publish and leaves once it is sent, if the join was its own.
\* Meanwhile the engine may subscribe to that same topic; then the topic is
\* the engine's, and the publish must not leave it. The engine hears only of
\* topics it subscribed to: not of one joined for a publish, nor of one it
\* left while some of its messages were still on their way.
\*
\* A tiny example. Nodes a and b subscribe and dial each other; node c
\* subscribes but dials no one. a publishes. The hub pushes the message to b
\* and c. b holds a session with a, so its engine gets it; c holds none, so
\* its endpoint drops it.
\*
\* What must never happen:
\*   1. a node's engine gets a message while the node holds no peer session
\*      - INV_NoDeliveryOffMesh;
\*   2. a message of a raw topic reaches the engine decoded as a PushLog
\*      - INV_RawNeverDecoded;
\*   3. a peer is reported JOINED that the node holds no session with, or
\*      that never joined - INV_JoinedOnlyLinkedMembers;
\*   4. the engine subscribed to a topic and the node is not on it
\*      - INV_EngineKeepsItsTopic;
\*   5. the engine hears of a topic it is not subscribed to
\*      - INV_OnlyEngineTopics.
\*
\* Correspondence with the Rust code (crates/p2p/src/vinc/):
\*
\*   | Model              | Rust                                                     |
\*   |--------------------|----------------------------------------------------------|
\*   | Subscribe          | Node::subscribe in topics.rs: mark raw, add to the       |
\*   |                    | subscribed set, then ep.join                             |
\*   | Unsubscribe        | Node::unsubscribe: out of the set, then ep.leave         |
\*   | MarkRaw (RED)      | the raw mark made after the join                         |
\*   | PublishJoin,       | Node::publish with wait: ep.join (new or not), the       |
\*   | PublishLeave       | publish, then ep.leave if the join was new and the       |
\*   |                    | subscribed set does not hold the topic                   |
\*   | Link, Unlink       | Node::connected / Node::disconnected in endpoint.rs      |
\*   | Publish            | ep.publish, and vinc's hub pushing to the members        |
\*   | inflight           | what the hub pushed and vinc queued as events            |
\*   | Deliver            | Node::topic_events and Node::delivery: the subscribed    |
\*   |                    | filter, the peer-session gate, then raw or decoded       |
\*   | Neighbor           | Node::topic_events: NeighborUp becomes PeerSubscribed    |
\*   | Gate               | the `self.peers.borrow().is_empty()` check (RED: none)   |
\*   | RawFirst           | raw marked before the join (RED: after)                  |
\*   | SessionFilter      | vinc's neighbours, members with a session (RED: the      |
\*   |                    | hub's member list as is)                                 |
\*   | OwnerCheck         | leave only a join of its own the engine has not taken    |
\*   |                    | (RED: leave always)                                      |
\*   | EngineFilter       | events of a topic not in the subscribed set are skipped  |
\*   |                    | (RED: handed on)                                         |
\*
\* Gaps, explained:
\*   * One topic and one hub: topics never interact, and every node names the
\*     same hub in the tests this models.
\*   * A message is just its publisher and whether its topic is raw; its
\*     signature is vinc's to verify, and verified before Deliver.
\*   * The gate is coarse: two meshes that share a hub hear each other. That
\*     is a reach the code allows, not one this model forbids.
\*   * One waiting publish per node at a time: two at once on one topic share
\*     the first one's join, which only shortens the second's wait.

EXTENDS Naturals, FiniteSets
\* Naturals gives the range 2..3; FiniteSets gives Cardinality.

CONSTANTS Nodes, RawTopic, Gate, RawFirst, SessionFilter, OwnerCheck, EngineFilter
\* Nodes: the nodes. RawTopic: whether the topic is raw. The five other
\* switches are guards in the code: TRUE as written, and each RED turns one
\* off.

ASSUME /\ Cardinality(Nodes) \in 2..3
       \* two or three nodes;
       /\ \A b \in {RawTopic, Gate, RawFirst, SessionFilter, OwnerCheck, EngineFilter} : b \in BOOLEAN
       \* and every switch is on or off.

VARIABLES joined, engine, waiting, owns, raw, links, inflight, delivered, reported
\* joined: the nodes on the topic, as the hub knows them. engine: the nodes
\* whose engine is subscribed. waiting: the nodes with a waiting publish under
\* way. owns: those of them whose publish made the join. raw: the nodes that
\* mark the topic raw. links: the peer sessions, as pairs. inflight: messages
\* the hub pushed that the endpoint has not handled, as <<to, from>>.
\* delivered: what each engine got. reported: the JOINED events each node made.

vars == <<joined, engine, waiting, owns, raw, links, inflight, delivered, reported>>
\* Every variable, for the stuttering steps.

Delivery == [decoded : BOOLEAN, onMesh : BOOLEAN, rawTopic : BOOLEAN, owned : BOOLEAN]
\* A message the engine got: whether it was decoded as a PushLog, whether the
\* node held a peer session then, whether its topic is raw, and whether the
\* engine was subscribed to it then.

Report == [linked : BOOLEAN, member : BOOLEAN, owned : BOOLEAN]
\* A JOINED event: whether, when it was made, the node held a session with the
\* peer, the hub listed the peer as a member, and the engine was subscribed.

Linked(n, p) == {n, p} \in links
\* Whether n and p hold a peer session.

OnMesh(n) == \E p \in Nodes \ {n} : Linked(n, p)
\* Whether n holds any peer session.

Skipped(n) == EngineFilter /\ n \notin engine
\* Whether n's endpoint skips the topic's events: its engine is not on it.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ joined \subseteq Nodes /\ engine \subseteq Nodes
  \* some nodes are on the topic, some engines subscribed;
  /\ owns \subseteq waiting /\ waiting \subseteq Nodes
  \* some nodes have a publish waiting, some of which made their join;
  /\ raw \subseteq Nodes
  \* some nodes mark the topic raw;
  /\ links \subseteq {{x, y} : x, y \in Nodes}
  \* the sessions are pairs of nodes;
  /\ inflight \subseteq Nodes \X Nodes
  \* the pushed messages name a receiver and a publisher;
  /\ delivered \in [Nodes -> SUBSET Delivery]
  \* each engine got some messages;
  /\ reported \in [Nodes -> SUBSET Report]
  \* and each node made some JOINED events.

Init ==
  \* No one joined, no session, nothing sent.
  /\ joined = {} /\ engine = {} /\ waiting = {} /\ owns = {}
  \* no one is on the topic and nothing waits;
  /\ raw = {}
  \* no topic is marked raw;
  /\ links = {}
  \* no session is up;
  /\ inflight = {}
  \* nothing is pushed;
  /\ delivered = [n \in Nodes |-> {}]
  \* no engine got anything;
  /\ reported = [n \in Nodes |-> {}]
  \* and no event was made.

Subscribe(n) ==
  \* n's engine subscribes. A raw topic is marked raw in the same step (RED:
  \* later, by MarkRaw).
  /\ n \notin engine
  \* the engine is not subscribed;
  /\ engine' = engine \cup {n}
  \* now it is;
  /\ joined' = joined \cup {n}
  \* the hub knows n (a second join is harmless);
  /\ raw' = IF RawTopic /\ RawFirst THEN raw \cup {n} ELSE raw
  \* and, for a raw topic, n marked it raw first;
  /\ UNCHANGED <<waiting, owns, links, inflight, delivered, reported>>
  \* The rest stays.

Unsubscribe(n) ==
  \* n's engine unsubscribes: the node leaves the topic. Messages the hub
  \* already pushed stay on their way.
  /\ n \in engine
  \* the engine is subscribed;
  /\ engine' = engine \ {n}
  \* now it is not;
  /\ joined' = joined \ {n}
  \* and the node is off the topic;
  /\ UNCHANGED <<waiting, owns, raw, links, inflight, delivered, reported>>
  \* The rest stays.

MarkRaw(n) ==
  \* RED only: n marks the raw topic raw after it joined.
  /\ RawTopic /\ ~RawFirst
  \* the topic is raw and marked late;
  /\ n \in engine /\ n \notin raw
  \* n subscribed and has not marked it;
  /\ raw' = raw \cup {n}
  \* now it has;
  /\ UNCHANGED <<joined, engine, waiting, owns, links, inflight, delivered, reported>>
  \* The rest stays.

PublishJoin(n) ==
  \* A waiting publish starts on n: it joins the topic, and the join is its
  \* own if n was not on it.
  /\ n \notin waiting
  \* one waiting publish at a time;
  /\ waiting' = waiting \cup {n}
  \* this one is under way;
  /\ owns' = IF n \in joined THEN owns ELSE owns \cup {n}
  \* it owns the join only if it made it;
  /\ joined' = joined \cup {n}
  \* and n is on the topic;
  /\ UNCHANGED <<engine, raw, links, inflight, delivered, reported>>
  \* The rest stays.

PublishLeave(n) ==
  \* The waiting publish is sent, and leaves the topic if the join was its own
  \* and the engine has not subscribed since (RED: it leaves whatever happened).
  /\ n \in waiting
  \* a waiting publish is under way;
  /\ waiting' = waiting \ {n}
  \* it ends;
  /\ owns' = owns \ {n}
  \* and owns nothing any more;
  /\ joined' = IF (n \in owns /\ n \notin engine) \/ ~OwnerCheck
               THEN joined \ {n}
               ELSE joined
  \* n leaves only a join of the publish's own the engine has not taken;
  /\ UNCHANGED <<engine, raw, links, inflight, delivered, reported>>
  \* The rest stays.

Link(n, p) ==
  \* n and p bring up a peer session.
  /\ n # p /\ ~Linked(n, p)
  \* two different nodes with no session yet;
  /\ links' = links \cup {{n, p}}
  \* now they hold one;
  /\ UNCHANGED <<joined, engine, waiting, owns, raw, inflight, delivered, reported>>
  \* The rest stays.

Unlink(n, p) ==
  \* The session between n and p ends.
  /\ Linked(n, p)
  \* they hold one;
  /\ links' = links \ {{n, p}}
  \* now they do not;
  /\ UNCHANGED <<joined, engine, waiting, owns, raw, inflight, delivered, reported>>
  \* The rest stays.

Publish(n) ==
  \* n publishes once, and the hub pushes it to every other node on the topic.
  /\ ~\E m \in inflight : m[2] = n
  \* one message per publisher in flight, to keep the states small;
  /\ inflight' = inflight \cup {<<m, n>> : m \in joined \ {n}}
  \* the hub pushes to every member but the publisher, linked or not;
  /\ UNCHANGED <<joined, engine, waiting, owns, raw, links, delivered, reported>>
  \* The rest stays.

Deliver(n, p) ==
  \* n's endpoint handles the message p published: skipped when the engine
  \* is not subscribed (RED: not skipped), dropped off the mesh (RED: not
  \* dropped), else handed on raw or decoded as the raw set says.
  /\ <<n, p>> \in inflight
  \* the hub pushed it to n;
  /\ inflight' = inflight \ {<<n, p>>}
  \* it is handled once;
  /\ delivered' = IF ~Skipped(n) /\ (OnMesh(n) \/ ~Gate)
                  \* neither skipped nor off the mesh,
                  THEN [delivered EXCEPT ![n] = @ \cup
                         {[decoded |-> n \notin raw, onMesh |-> OnMesh(n),
                           rawTopic |-> RawTopic, owned |-> n \in engine]}]
                  \* so its engine gets it, decoded unless marked raw;
                  ELSE delivered
                  \* else it is dropped;
  /\ UNCHANGED <<joined, engine, waiting, owns, raw, links, reported>>
  \* The rest stays.

Neighbor(n, p) ==
  \* n reports p JOINED: vinc said a member with a session appeared (RED:
  \* any member the hub lists, session or not). Skipped when the engine is
  \* not subscribed.
  /\ n # p /\ n \in joined /\ p \in joined
  \* both are on the topic;
  /\ Linked(n, p) \/ ~SessionFilter
  \* and they hold a session (RED: not needed);
  /\ ~Skipped(n)
  \* and the engine is subscribed (RED: whatever);
  /\ reported' = [reported EXCEPT ![n] = @ \cup
                   {[linked |-> Linked(n, p), member |-> p \in joined, owned |-> n \in engine]}]
  \* the event is made;
  /\ UNCHANGED <<joined, engine, waiting, owns, raw, links, inflight, delivered>>
  \* The rest stays.

Next ==
  \* Any node may act; any message may be handled next.
  \/ \E n \in Nodes : \/ Subscribe(n) \/ Unsubscribe(n) \/ MarkRaw(n)
                      \/ PublishJoin(n) \/ PublishLeave(n) \/ Publish(n)
  \/ \E n, p \in Nodes : Link(n, p) \/ Unlink(n, p) \/ Deliver(n, p) \/ Neighbor(n, p)

Spec == Init /\ [][Next]_vars
\* Start empty and take any step.

INV_NoDeliveryOffMesh ==
  \* An engine only gets a message while its node holds a peer session.
  \A n \in Nodes : \A d \in delivered[n] : d.onMesh

INV_RawNeverDecoded ==
  \* No message of a raw topic reaches an engine decoded as a PushLog.
  \A n \in Nodes : \A d \in delivered[n] : d.rawTopic => ~d.decoded

INV_JoinedOnlyLinkedMembers ==
  \* Every JOINED event names a member the node held a session with.
  \A n \in Nodes : \A r \in reported[n] : r.linked /\ r.member

INV_EngineKeepsItsTopic ==
  \* A node whose engine is subscribed is on the topic.
  engine \subseteq joined

INV_OnlyEngineTopics ==
  \* An engine gets messages and JOINED events only while it is subscribed.
  \A n \in Nodes : /\ \A d \in delivered[n] : d.owned
                   /\ \A r \in reported[n] : r.owned

====
