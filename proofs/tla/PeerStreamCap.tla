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
\*     handlers have all ended.
\*
\* Rust: crates/p2p/src/two_stream/peer_streams.rs PeerStreams::admit and
\* the StreamAdmission guard's drop; crates/p2p/src/two_stream/runner.rs,
\* where every inbound stream is admitted before its handler is spawned.
EXTENDS Naturals

\* "none" keeps every guard (GREEN). "nocheck" admits past the cap
\* (INV_WithinCap fails); "leak" keeps the slot when a handler fails or is
\* cancelled (INV_SlotsFollowStreams fails).
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
VARIABLES running, slots, opened

\* Every variable, for UNCHANGED.
vars == <<running, slots, opened>>

\* Nothing running, nothing counted, nothing opened.
Init ==
  /\ running = [p \in Peers |-> 0]
  /\ slots = [p \in Peers |-> 0]
  /\ opened = 0

\* Peer p opens a stream. It runs if p's counted slots are below the cap
\* (always, with the "nocheck" bug); otherwise it is reset and nothing
\* changes but the count of opens.
Open(p) ==
  /\ opened < Opens
  /\ opened' = opened + 1
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
  /\ UNCHANGED opened

\* Every step the node can take.
Next == \E p \in Peers : Open(p) \/ End(p)

\* No peer ever has more streams running than the cap.
INV_WithinCap == \A p \in Peers : running[p] <= Cap

\* The node counts exactly the streams that are running, so a peer whose
\* handlers have all ended can open a stream again.
INV_SlotsFollowStreams == \A p \in Peers : slots[p] = running[p]

\* Explore every interleaving.
Spec == Init /\ [][Next]_vars
=============================================================================
