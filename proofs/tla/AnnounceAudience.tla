---- MODULE AnnounceAudience ----
\* The story, in plain words.
\*
\* A node commits a write and announces it over gossip. To save messages it
\* usually holds the announcement for a short window (250 ms), so a burst of
\* writes to one document goes out as one. Meanwhile another peer may connect
\* and, a little later (90 to 130 ms in the run that failed), subscribe to
\* the collection. Go announces a write once, when it commits, to the peers
\* subscribed then, and never publishes it again (internal/db/p2p/p2p.go
\* SendUpdate: one PublishToTopicAsync per topic, "Retries are for
\* replicators only"; go-p2p host.go PublishToTopicAsync publishes without
\* WithRepublishing). A peer that subscribes later learns of the write only
\* through the DAG of a later update.
\*
\* Who acts: the writer (commits, announces, retries), the other peer
\* (connects, then subscribes), and the clock (closes the window).
\*
\* What must never happen:
\*   1. a write reaches a peer that was not subscribed when it committed -
\*      INV_GoAudience;
\*   2. a write is never announced - LIVE_Announced.
\*
\* Correspondence with the Rust code:
\*
\*   | Model              | Rust                                                       |
\*   |--------------------|------------------------------------------------------------|
\*   | Commit             | p2p sync/coordinator/broadcast.rs                          |
\*   |                    | broadcast_local_update_with_creator, called for a write   |
\*   | Alone              | p2p sync/peer_state/tracker/operations.rs has_subscriber,  |
\*   |                    | for the collection and the document topics                 |
\*   | subscribedAtCommit | whether has_subscriber was true at that call               |
\*   | Hold               | p2p sync/broadcast_coalescer.rs BroadcastCoalescer::run:   |
\*   |                    | the leader waits out its window (wait_for_quiet)           |
\*   | Publish            | Broadcaster::broadcast_update; it reaches the peer only if |
\*   |                    | the peer is subscribed by then, else InsufficientPeers     |
\*   | Retry              | db merge/broadcast_mutator/broadcast.rs                    |
\*   |                    | broadcast_with_creator publishes once: no Retry in GREEN;  |
\*   |                    | RED is the retry loop that function replaced               |
\*   | Connect, Subscribe | TransportEvent PeerConnected, then PeerSubscribed          |
\*
\* Gaps, explained:
\*   * One write and one other peer: every write is announced on its own
\*     path. With a subscriber there at commit the write is held, and a
\*     second peer subscribing during the window also gets it; that is the
\*     coalescing window's accepted cost, and this model has no second peer.
\*   * Unsubscribing and disconnecting are left out: they only shrink who
\*     receives, so they cannot break INV_GoAudience.

CONSTANTS SkipWhenAlone, AloneMeansUnsubscribed, RetryWhileConnected
\* The three guards. GREEN: TRUE, TRUE, FALSE. Each RED configuration flips one.

ASSUME SkipWhenAlone \in BOOLEAN
\* TRUE: a write with no subscriber is announced at once, not held.
ASSUME AloneMeansUnsubscribed \in BOOLEAN
\* TRUE: "alone" means no peer subscribed; FALSE: no peer connected (#2848's
\* first cut).
ASSUME RetryWhileConnected \in BOOLEAN
\* TRUE: an announcement that found no subscriber is published again while a
\* peer is connected (the db retry loop before this fix).

VARIABLES phase, peer, subscribedAtCommit, delivered
\* phase: "idle" before the write, "held" in its window, "announced" after.
\* peer: "away", "connected" (no subscription yet) or "subscribed".
\* subscribedAtCommit: whether the peer was subscribed when the write
\* committed. delivered: whether an announcement reached the peer.

vars == <<phase, peer, subscribedAtCommit, delivered>>
\* Every variable, for the stuttering steps.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ phase \in {"idle", "held", "announced"}
  /\ peer \in {"away", "connected", "subscribed"}
  /\ subscribedAtCommit \in BOOLEAN
  /\ delivered \in BOOLEAN

Init ==
  \* Nothing written yet; the peer may be at any stage already.
  /\ phase = "idle"
  /\ peer \in {"away", "connected", "subscribed"}
  /\ subscribedAtCommit = FALSE
  /\ delivered = FALSE

Alone ==
  \* Whether the writer counts itself alone: no subscriber (GREEN), or no
  \* connected peer (RED).
  IF AloneMeansUnsubscribed THEN peer # "subscribed" ELSE peer = "away"

Publish ==
  \* What an announcement does: it reaches the peer only if subscribed now.
  /\ phase' = "announced"
  /\ delivered' = (delivered \/ peer = "subscribed")

Commit ==
  \* The write commits. Alone, it is announced at once (RED: held anyway);
  \* otherwise it is held for the window.
  /\ phase = "idle"
  /\ subscribedAtCommit' = (peer = "subscribed")
  /\ IF SkipWhenAlone /\ Alone
     THEN Publish
     ELSE /\ phase' = "held"
          /\ UNCHANGED delivered
  /\ UNCHANGED peer

WindowCloses ==
  \* The window ends and the held write is announced.
  /\ phase = "held"
  /\ Publish
  /\ UNCHANGED <<peer, subscribedAtCommit>>

Retry ==
  \* RED only: an announcement that reached no one is published again while
  \* a peer is connected, and reaches it once it has subscribed.
  /\ RetryWhileConnected
  /\ phase = "announced"
  /\ ~delivered
  /\ peer # "away"
  /\ Publish
  /\ UNCHANGED <<peer, subscribedAtCommit>>

Connect ==
  \* The other peer connects, at any moment.
  /\ peer = "away"
  /\ peer' = "connected"
  /\ UNCHANGED <<phase, subscribedAtCommit, delivered>>

Subscribe ==
  \* The connected peer subscribes to the collection, at any later moment.
  /\ peer = "connected"
  /\ peer' = "subscribed"
  /\ UNCHANGED <<phase, subscribedAtCommit, delivered>>

Next == Commit \/ WindowCloses \/ Retry \/ Connect \/ Subscribe
\* Each step is the writer's, the clock's or the peer's.

Spec == Init /\ [][Next]_vars /\ WF_vars(Commit) /\ WF_vars(WindowCloses)
\* The write commits, and its window closes.

INV_GoAudience == delivered => subscribedAtCommit
\* Promise: an announcement reaches only a peer that was subscribed when the
\* write committed, as Go's.

LIVE_Announced == <>(phase = "announced")
\* Promise: every write is announced.

====
