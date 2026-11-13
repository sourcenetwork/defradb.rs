---- MODULE AnnounceAudience ----
\* The story, in plain words.
\*
\* A node commits a write and announces it over gossip. To save messages it
\* usually holds the announcement for a short window (250 ms), so a burst of
\* writes to one document goes out as one. Meanwhile another peer may connect
\* and subscribe to the collection. Go announces a write when it commits, to
\* the peers subscribed then: a peer that connects later learns of the write
\* only through the DAG of a later update.
\*
\* Who acts: the writer (commits, announces), the other peer (connects and
\* subscribes), and the clock (closes the window).
\*
\* What must never happen:
\*   1. a write committed while no peer was connected is announced to a peer
\*      that connected after it - INV_GoAudience;
\*   2. a write is never announced - LIVE_Announced.
\*
\* Correspondence with the Rust code (crates/p2p/src/sync):
\*
\*   | Model             | Rust                                                        |
\*   |-------------------|-------------------------------------------------------------|
\*   | Commit            | coordinator/broadcast.rs broadcast_local_update_with_creator|
\*   |                   | is called for a committed write                             |
\*   | connectedAtCommit | peer_state has_connected_peer() at that call                |
\*   | Hold              | broadcast_coalescer.rs BroadcastCoalescer::run: the leader  |
\*   |                   | waits out its window (wait_for_quiet)                       |
\*   | Announce          | Broadcaster::broadcast_update publishes to the topics; it   |
\*   |                   | reaches the peer if the peer subscribes by then             |
\*   | SkipWhenAlone     | the has_connected_peer() check that publishes at once       |
\*   |                   | (RED: FALSE, the window is always used, as before #2841)    |
\*   | Join              | the peer connects and subscribes to the collection          |
\*
\* Gaps, explained:
\*   * One write and one other peer: every write is announced on its own path.
\*   * A peer connected at commit but not yet subscribed is the window's
\*     ordinary case and is not what this model checks.
\*   * The db crate's broadcast retry (merge/broadcast_mutator/broadcast.rs
\*     broadcast_with_retry_with_creator) publishes again on InsufficientPeers
\*     while a peer is connected, so it can reach a peer that subscribes after
\*     the commit. Go never retries; the model leaves the retry out.

CONSTANT SkipWhenAlone
\* TRUE as written; a RED configuration sets it FALSE.

ASSUME SkipWhenAlone \in BOOLEAN
\* The guard is on or off.

VARIABLES phase, peer, connectedAtCommit, delivered
\* phase: "idle" before the write, "held" in its window, "announced" after.
\* peer: "away" or "joined" (connected and subscribed). connectedAtCommit:
\* whether the peer was there when the write committed. delivered: whether
\* the announcement reached the peer.

vars == <<phase, peer, connectedAtCommit, delivered>>
\* Every variable, for the stuttering steps.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ phase \in {"idle", "held", "announced"}
  /\ peer \in {"away", "joined"}
  /\ connectedAtCommit \in BOOLEAN
  /\ delivered \in BOOLEAN

Init ==
  \* Nothing written yet; the peer may or may not be there already.
  /\ phase = "idle"
  /\ peer \in {"away", "joined"}
  /\ connectedAtCommit = FALSE
  /\ delivered = FALSE

Publish ==
  \* What an announcement does: it reaches the peer if the peer is there.
  /\ phase' = "announced"
  /\ delivered' = (peer = "joined")

Commit ==
  \* The write commits. With no peer there it is announced at once (RED:
  \* held anyway); otherwise it is held for the window.
  /\ phase = "idle"
  /\ connectedAtCommit' = (peer = "joined")
  /\ IF SkipWhenAlone /\ peer = "away"
     THEN Publish
     ELSE /\ phase' = "held"
          /\ UNCHANGED delivered
  /\ UNCHANGED peer

WindowCloses ==
  \* The window ends and the held write is announced.
  /\ phase = "held"
  /\ Publish
  /\ UNCHANGED <<peer, connectedAtCommit>>

Join ==
  \* The other peer connects and subscribes, at any moment.
  /\ peer = "away"
  /\ peer' = "joined"
  /\ UNCHANGED <<phase, connectedAtCommit, delivered>>

Next == Commit \/ WindowCloses \/ Join
\* Each step is the writer's, the clock's or the peer's.

Spec == Init /\ [][Next]_vars /\ WF_vars(Commit) /\ WF_vars(WindowCloses)
\* The write commits, and its window closes.

INV_GoAudience == delivered => connectedAtCommit
\* Promise: an announcement reaches only a peer that was there when the
\* write committed, as Go's.

LIVE_Announced == <>(phase = "announced")
\* Promise: every write is announced.

====
