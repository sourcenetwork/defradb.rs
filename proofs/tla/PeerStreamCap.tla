---------------------------- MODULE PeerStreamCap ----------------------------
\* The story. Peers open inbound streams on a libp2p node. With
\* `max_streams_per_peer` set, the node runs at most that many streams from
\* one peer at a time: a stream past the cap is reset at once, as Go's
\* resource manager resets it with StreamResourceLimitExceeded. A stream that
\* is admitted holds a slot until its handler ends, however it ends: done,
\* failed, or cancelled.
\*
\* What must never happen:
\*   - a peer has more streams running than the cap;
\*   - a slot outlives its stream, so a peer is refused forever once its
\*     handlers have all ended;
\*   - the node forgets a peer that still runs a stream, so its slots are
\*     counted from zero again and it can run past the cap.
\*
\* The node keeps one counter per peer it has seen, and drops a peer whose
\* counter is at zero, so peers that come and go do not pile up. Only the
\* runner admits, so a counter it finds at zero stays there until it admits
\* again.
\*
\* Rust: crates/p2p/src/two_stream/peer_streams.rs PeerStreams::admit (and
\* its prune of idle counters) and the StreamAdmission guard's drop; crates/p2p/src/two_stream/runner.rs,
\* where every inbound stream is admitted before its handler is spawned.
EXTENDS Naturals

\* "none" keeps every guard (GREEN). "nocheck" admits past the cap
\* (INV_WithinCap fails); "leak" keeps the slot when a handler fails or is
\* cancelled (INV_SlotsFollowStreams fails); "evictlive" drops a peer's
\* counter while it still runs a stream (INV_SlotsFollowStreams fails).
CONSTANT Bug

\* Two peers and a cap of one are enough to show both failures.
Peers == {"a", "b"}
\* The cap on one peer's running streams.
Cap == 1
\* At most this many streams are ever opened, to keep the model finite.
Opens == 3

\* running: per peer, streams whose handler has not ended.
\* slots: per peer, slots the node counts as taken.
\* opened: streams opened so far, by everyone.
\* counted: the peers the node keeps a counter for.
VARIABLES running, slots, opened, counted

\* Every variable, for UNCHANGED.
vars == <<running, slots, opened, counted>>

\* Nothing running, nothing counted, nothing opened.
Init ==
  /\ running = [p \in Peers |-> 0]
  /\ slots = [p \in Peers |-> 0]
  /\ opened = 0
  \* No peer has a counter yet.
  /\ counted = {}

\* Peer p opens a stream. It runs if p's counted slots are below the cap
\* (always, with the "nocheck" bug); otherwise it is reset and nothing
\* changes but the count of opens.
Open(p) ==
  /\ opened < Opens
  /\ opened' = opened + 1
  \* Admitting looks the peer up, so it has a counter from now on.
  /\ counted' = counted \cup {p}
  /\ IF slots[p] < Cap \/ Bug = "nocheck"
     THEN /\ running' = [running EXCEPT ![p] = @ + 1]
          /\ slots' = [slots EXCEPT ![p] = @ + 1]
     ELSE UNCHANGED <<running, slots>>

\* One of p's handlers ends: done, failed or cancelled. Its slot is given
\* back, except a failed or cancelled one under the "leak" bug.
End(p) ==
  /\ running[p] > 0
  /\ running' = [running EXCEPT ![p] = @ - 1]
  /\ \E how \in {"done", "failed", "cancelled"} :
       slots' = [slots EXCEPT ![p] =
                   IF how # "done" /\ Bug = "leak" THEN @ ELSE @ - 1]
  /\ UNCHANGED <<opened, counted>>

\* The runner drops p's counter, which it does only at zero (at any count,
\* with the "evictlive" bug). A dropped counter starts again from zero.
Prune(p) ==
  /\ p \in counted
  /\ slots[p] = 0 \/ Bug = "evictlive"
  /\ counted' = counted \ {p}
  /\ slots' = [slots EXCEPT ![p] = 0]
  /\ UNCHANGED <<running, opened>>

\* Every step the node can take.
Next == \E p \in Peers : Open(p) \/ End(p) \/ Prune(p)

\* No peer ever has more streams running than the cap.
INV_WithinCap == \A p \in Peers : running[p] <= Cap

\* The node counts exactly the streams that are running, so a peer whose
\* handlers have all ended can open a stream again.
INV_SlotsFollowStreams == \A p \in Peers : slots[p] = running[p]

\* A peer the node keeps no counter for runs no stream.
INV_ForgottenAreIdle == \A p \in Peers \ counted : running[p] = 0

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
