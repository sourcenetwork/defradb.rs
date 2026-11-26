---- MODULE PeerIdentityCache ----
\* Several resolver tasks share one Iroh peer identity cache and insert into
\* it at the same time, with no lock. The cache is a map of peers plus a
\* bounded line (a FIFO) that says which peer leaves first. An insert first
\* writes the peer into the map; only a peer that was not there yet then
\* takes a place at the back of the line. When the line is full, the inserter
\* pops the front place and removes that peer from the map, then tries again.
\* Steps of different tasks interleave in any order.
\*
\* What must never happen: the map keeps a peer that has no place in the line
\* (it would never be evicted, so the cache would grow without bound), a peer
\* holds two places, the line names a peer the map no longer has, or the map
\* grows past the line's capacity once every insert has finished. A refresh
\* that lands while its own peer is being evicted may be removed with it; that
\* is allowed, and costs the next lookup a re-resolve.
\*
\* Code: crates/database/p2p/src/iroh/identity_cache.rs
\* IrohPeerIdentityCache::insert and IrohPeerIdentityCache::enqueue.
\* Naturals gives numbers, Sequences gives the line, FiniteSets counts sets.
EXTENDS Naturals, Sequences, FiniteSets

\* Peers: the peers that can be inserted. Tasks: the inserting tasks.
\* Cap: the line's capacity. Ops: how many inserts each task makes.
\* Mode: the real code, or one of three planted bugs.
CONSTANTS Peers, Tasks, Cap, Ops, Mode

\* "correct" is the code. "noEvict" pops the front place but leaves its peer
\* in the map. "refreshEnqueues" gives a refreshed peer a second place.
\* "enqueueFirst" takes the place before the peer is written into the map.
ASSUME Mode \in {"correct", "noEvict", "refreshEnqueues", "enqueueFirst"}
\* The line holds at least one place, and each task inserts at least once.
ASSUME Cap \in Nat \ {0} /\ Ops \in Nat \ {0}

\* cached: the peers in the map. line: the places, front first.
\* pc: where each task is in its insert. cur: the peer each task inserts.
\* victim: the peer a task popped and is about to remove.
\* left: the inserts each task has still to make.
VARIABLES cached, line, pc, cur, victim, left

\* All variables, for the "nothing else changes" part of the spec.
vars == <<cached, line, pc, cur, victim, left>>

\* The set of peers that have a place in the line.
Placed == {line[i] : i \in 1..Len(line)}

\* A task with no insert in progress.
Idle(t) == pc[t] = "idle"

\* Every task has finished its current insert.
Settled == \A t \in Tasks : Idle(t)

\* Start: an empty cache, every task idle with all its inserts to make.
Init == /\ cached = {}
\* The line is empty.
        /\ line = <<>>
\* Every task is idle.
        /\ pc = [t \in Tasks |-> "idle"]
\* No task has a peer in hand yet; the first peer is a placeholder.
        /\ cur = [t \in Tasks |-> CHOOSE p \in Peers : TRUE]
\* No task has popped anything yet.
        /\ victim = [t \in Tasks |-> CHOOSE p \in Peers : TRUE]
\* Every task has all of its inserts left.
        /\ left = [t \in Tasks |-> Ops]

\* insert's compute: write peer p into the map in one atomic step, and learn
\* whether p was already there. A new peer goes on to take a place; a refresh
\* is done ("refreshEnqueues" sends a refresh to take a place too). The
\* "enqueueFirst" bug takes the place before writing the map: see Link.
Compute(t, p) ==
\* The task is idle and has inserts left.
    /\ Idle(t) /\ left[t] > 0
\* It now holds peer p and has one insert fewer left.
    /\ cur' = [cur EXCEPT ![t] = p]
    /\ left' = [left EXCEPT ![t] = @ - 1]
\* Under "enqueueFirst" a new peer is not written yet; otherwise it is.
    /\ cached' = IF Mode = "enqueueFirst" THEN cached ELSE cached \cup {p}
\* A new peer goes on to take a place; a refresh is finished.
    /\ pc' = [pc EXCEPT ![t] =
                 IF p \notin cached \/ Mode = "refreshEnqueues" THEN "push" ELSE "idle"]
\* The line and the popped peers are untouched.
    /\ UNCHANGED <<line, victim>>

\* enqueue's push: if the line has room, take a place at its back and finish
\* ("enqueueFirst" still has to write the map); if it is full, go pop.
Push(t) ==
\* The task is about to push.
    /\ pc[t] = "push"
\* Room left: append the peer. Full: the line is unchanged.
    /\ line' = IF Len(line) < Cap THEN Append(line, cur[t]) ELSE line
\* After a successful push the insert is done (or goes on to Link); a full
\* line sends the task to pop.
    /\ pc' = [pc EXCEPT ![t] =
                 IF Len(line) < Cap
                 THEN IF Mode = "enqueueFirst" THEN "link" ELSE "idle"
                 ELSE "pop"]
\* The map, the peers in hand and the inserts left are untouched.
    /\ UNCHANGED <<cached, cur, victim, left>>

\* enqueue's pop: take the front place. Another task may have emptied the line
\* first; then there is nothing to evict and the task tries the push again.
Pop(t) ==
\* The task is about to pop.
    /\ pc[t] = "pop"
\* Front place taken: remember its peer and go remove it.
    /\ IF Len(line) > 0
       THEN /\ victim' = [victim EXCEPT ![t] = Head(line)]
            /\ line' = Tail(line)
\* "noEvict" skips the removal and goes straight back to push.
            /\ pc' = [pc EXCEPT ![t] = IF Mode = "noEvict" THEN "push" ELSE "remove"]
\* Line empty: nothing popped, try the push again.
       ELSE /\ pc' = [pc EXCEPT ![t] = "push"]
            /\ UNCHANGED <<victim, line>>
\* The map, the peers in hand and the inserts left are untouched.
    /\ UNCHANGED <<cached, cur, left>>

\* enqueue's remove: drop the popped peer from the map, whatever its DID is
\* now (a refresh since the pop goes too), then try the push again.
Remove(t) ==
\* The task holds a popped peer.
    /\ pc[t] = "remove"
\* That peer leaves the map.
    /\ cached' = cached \ {victim[t]}
\* Back to pushing its own peer.
    /\ pc' = [pc EXCEPT ![t] = "push"]
\* The line, the peers in hand and the inserts left are untouched.
    /\ UNCHANGED <<line, cur, victim, left>>

\* Only under "enqueueFirst": the map write that comes after the place.
Link(t) ==
\* The task has its place and still has to write the map.
    /\ pc[t] = "link"
\* The peer is written into the map.
    /\ cached' = cached \cup {cur[t]}
\* The insert is finished.
    /\ pc' = [pc EXCEPT ![t] = "idle"]
\* The line, the peers in hand and the inserts left are untouched.
    /\ UNCHANGED <<line, cur, victim, left>>

\* Any task takes any one of its steps.
Next == \E t \in Tasks :
          \/ \E p \in Peers : Compute(t, p)
          \/ Push(t) \/ Pop(t) \/ Remove(t) \/ Link(t)

\* Start in Init, then take Next steps or stand still.
Spec == Init /\ [][Next]_vars

\* Every variable stays one of its allowed values.
TypeOK == /\ cached \subseteq Peers
          /\ line \in Seq(Peers) /\ Len(line) <= Cap
          /\ pc \in [Tasks -> {"idle", "push", "pop", "remove", "link"}]
          /\ cur \in [Tasks -> Peers] /\ victim \in [Tasks -> Peers]
          /\ left \in [Tasks -> 0..Ops]

\* No peer ever holds two places in the line.
NoDoublePlace == \A i, j \in 1..Len(line) : i # j => line[i] # line[j]

\* Every place in the line names a peer the map still has, so popping a
\* place always evicts a live entry.
PlacedAreCached == Placed \subseteq cached

\* Once every insert has finished, the map holds exactly the placed peers:
\* no cached peer is left without a place, so every one can be evicted.
SettledMatch == Settled => cached = Placed

\* Once every insert has finished, the map is within the line's capacity.
SettledBounded == Settled => Cardinality(cached) <= Cap

\* Even mid-insert, the map exceeds capacity by at most two peers per task:
\* its own new peer, not placed yet, and the peer it popped but has yet to
\* remove.
Bounded == Cardinality(cached) <= Cap + 2 * Cardinality(Tasks)
====
