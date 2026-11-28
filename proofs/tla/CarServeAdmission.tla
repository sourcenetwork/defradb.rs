---- MODULE CarServeAdmission ----
\* The story. A node serves CAR files to peers that are catching up on its
\* history. Serving is expensive, so only `Capacity` serves run at once: that
\* is the serve lane. Peers ask for more than that. A request that finds the
\* lane full does not get turned away at once: it waits in a small waiting
\* room for a short time. When a serve finishes, its slot goes straight to the
\* waiting request whose peer is running the fewest serves, so one hungry peer
\* cannot keep the lane from everyone else. A request whose wait runs out, or
\* that finds the waiting room (or its own peer's share of the room) full, is
\* answered "busy" when a rejection worker is free, or has its stream closed
\* when none is; either way the asking peer moves on to another provider.
\*
\* Who acts: requests arrive, serves finish (or fail), waits run out.
\* What must never happen: more serves than slots; a slot that is counted but
\* not running, or running but not counted; a free slot beside a waiting
\* request; a waiting room larger than its bound; a "busy" answer to a request
\* that could have waited; a freed slot handed to a peer that already runs
\* more serves than another waiting peer.
\* What must happen: every request is eventually served, answered busy, or
\* ended with its stream closed.
\*
\* Rust owners:
\*   crates/database/p2p/src/sync/car_serve_admission.rs  ServeAdmission::offer, release, pop_expired
\*   crates/database/p2p/src/sync/event_dispatcher.rs     run_event_dispatcher, spawn_serve, ServeSlot::drop, refuse
EXTENDS Naturals, FiniteSets

\* Each guard below is TRUE in the GREEN configuration; a RED configuration
\* turns one off to show the invariant that guard protects.
CONSTANTS
    CheckCapacity,   \* admit at once only while a slot is free
    WaitWhenFull,    \* a request that finds the lane full waits rather than being shed
    BoundRoom,       \* the waiting room and each peer's share of it are bounded
    GrantOnRelease,  \* a finished serve hands its slot to a waiting request
    FairGrant,       \* that slot goes to the peer running the fewest serves
    ReleaseOnFail,   \* a serve that fails (panics, is cancelled) still returns its slot
    Capacity,        \* the number of serves the lane runs at once
    ShareDivisor     \* the divisor that gives one peer its waiting-room share

\* The lane and share must have meaningful positive sizes.
ASSUME Capacity > 0 /\ ShareDivisor > 0

\* Two peers; request 1 summarizes the serves already running in the full lane.
\* Peer "a" sends four new requests, peer "b" sends one.
Peers == {"a", "b"}
\* The requests, numbered.
Requests == 1..6
\* Which peer sent each request.
Owner(r) == IF r \in 1..5 THEN "a" ELSE "b"
\* The waiting room holds as many requests as the lane has slots.
Room == Capacity
\* One peer may fill one `ShareDivisor` of the room, and at least one place.
Share == IF Room \div ShareDivisor > 0 THEN Room \div ShareDivisor ELSE 1

\* st[r]: where request r is. "new" not yet arrived; "waiting" in the room;
\* "active" being served; "served" done; "busy" answered busy; "closed" ended
\* by closing the stream when no rejection worker was free.
\* held[p]: the slots the lane counts against peer p (the Rust `active` map).
\* slots: the slots the lane counts in all (the Rust `active_total`).
\* badShed: some request was shed at once although it could have waited.
\* unfair: some freed slot went to a peer running more serves than another waiting peer.
VARIABLES st, held, slots, badShed, unfair
\* All the variables, for stuttering steps.
vars == <<st, held, slots, badShed, unfair>>

\* The requests waiting in the room.
WaitingSet == {r \in Requests : st[r] = "waiting"}
\* The requests of peer p waiting in the room.
WaitingOf(p) == {r \in WaitingSet : Owner(r) = p}
\* Whether a newly arrived request of peer p would fit in the room.
RoomHasSpace(p) == Cardinality(WaitingSet) < Room /\ Cardinality(WaitingOf(p)) < Share

\* Start at the moment the lane is full. Request 1 stands for the serves
\* already running under peer "a"; the other requests have not arrived yet.
Init == /\ st = [r \in Requests |-> IF r = 1 THEN "active" ELSE "new"]
        /\ held = [p \in Peers |-> IF p = "a" THEN Capacity ELSE 0]
        /\ slots = Capacity
        /\ badShed = FALSE
        /\ unfair = FALSE

\* A request arrives (`ServeAdmission::offer`). With a slot free it is served
\* at once. With the lane full it waits if the room has space for it, and is
\* answered busy or closed otherwise.
Arrive(r) ==
    LET p == Owner(r) IN
    /\ st[r] = "new"
    /\ IF slots < Capacity \/ ~CheckCapacity
       \* A free slot (or, with the guard off, no check at all): serve now.
       THEN /\ st' = [st EXCEPT ![r] = "active"]
            /\ held' = [held EXCEPT ![p] = @ + 1]
            /\ slots' = slots + 1
            /\ UNCHANGED <<badShed, unfair>>
       \* The lane is full: wait if the room (and this peer's share) has space.
       ELSE IF WaitWhenFull /\ (RoomHasSpace(p) \/ ~BoundRoom)
            THEN /\ st' = [st EXCEPT ![r] = "waiting"]
                 /\ UNCHANGED <<held, slots, badShed, unfair>>
            \* Otherwise shed it; it was a wrong shed if the room had space.
            ELSE \/ /\ st' = [st EXCEPT ![r] = "busy"]
                    /\ badShed' = (badShed \/ RoomHasSpace(p))
                    /\ UNCHANGED <<held, slots, unfair>>
                 \/ /\ st' = [st EXCEPT ![r] = "closed"]
                    /\ badShed' = (badShed \/ RoomHasSpace(p))
                    /\ UNCHANGED <<held, slots, unfair>>

\* The slots held per peer once peer p has given one back.
Returned(p) == [held EXCEPT ![p] = @ - 1]

\* Hand a freed slot to waiting request w (`ServeAdmission::release`). `h` is
\* the per-peer count after the release. It is unfair if another waiting
\* request's peer runs fewer serves than w's peer.
Grant(w, h, done, r) ==
    /\ st' = [st EXCEPT ![r] = done, ![w] = "active"]
    /\ held' = [h EXCEPT ![Owner(w)] = @ + 1]
    /\ UNCHANGED slots
    /\ unfair' = (unfair \/ \E v \in WaitingSet : h[Owner(v)] < h[Owner(w)])
    /\ UNCHANGED badShed

\* Serve r ends and ends as `done`, returning its slot (`ServeSlot::drop`). If
\* requests wait, the slot goes to one of them: with FairGrant, one whose peer
\* runs the fewest serves; without it, any of them. Without GrantOnRelease the
\* slot is just returned and the waiting requests stay waiting.
ReleaseAndGrant(r, done) ==
    LET p == Owner(r)
        h == Returned(p)
    IN
    IF GrantOnRelease /\ WaitingSet # {}
    THEN \E w \in WaitingSet :
            /\ (FairGrant => \A v \in WaitingSet : h[Owner(w)] <= h[Owner(v)])
            /\ Grant(w, h, done, r)
    ELSE /\ st' = [st EXCEPT ![r] = done]
         /\ held' = h
         /\ slots' = slots - 1
         /\ UNCHANGED <<badShed, unfair>>

\* Serve r finishes normally and gives its slot back.
Finish(r) ==
    /\ st[r] = "active"
    /\ ReleaseAndGrant(r, "served")

\* Serve r fails (a panic or a cancellation). The slot guard still runs on
\* drop; with ReleaseOnFail off it is lost and stays counted.
Fail(r) ==
    /\ st[r] = "active"
    /\ IF ReleaseOnFail
       THEN ReleaseAndGrant(r, "served")
       ELSE /\ st' = [st EXCEPT ![r] = "served"]
            /\ UNCHANGED <<held, slots, badShed, unfair>>

\* A waiting request's deadline passes (`ServeAdmission::pop_expired`): it
\* leaves the room and is answered busy, or its stream closes if no rejection
\* worker is free.
Expire(r) ==
    /\ st[r] = "waiting"
    /\ \/ /\ st' = [st EXCEPT ![r] = "busy"]
          /\ UNCHANGED <<held, slots, badShed, unfair>>
       \/ /\ st' = [st EXCEPT ![r] = "closed"]
          /\ UNCHANGED <<held, slots, badShed, unfair>>

\* Any one of these steps may happen next.
Next == \E r \in Requests : Arrive(r) \/ Finish(r) \/ Fail(r) \/ Expire(r)

\* Serves eventually finish and waits eventually run out.
Spec == Init /\ [][Next]_vars
        /\ \A r \in Requests : WF_vars(Finish(r)) /\ WF_vars(Expire(r))

\* Every variable holds the kind of value it should.
TypeOK == /\ st \in [Requests -> {"new", "waiting", "active", "served", "busy", "closed"}]
          /\ held \in [Peers -> 0..Capacity]
          /\ slots \in 0..Capacity
          /\ badShed \in BOOLEAN
          /\ unfair \in BOOLEAN

\* Never more serves counted than the lane has slots.
INV_Capacity == slots <= Capacity

\* The total and per-peer slot counts agree, and request 1 plus the
\* summarized background serves account for the lane. A failed serve that is
\* not released therefore leaves a counted slot with no active request.
INV_SlotsAccounted ==
    /\ held["a"] = (Capacity - 1) + Cardinality({r \in Requests : st[r] = "active" /\ Owner(r) = "a"})
    /\ held["b"] = Cardinality({r \in Requests : st[r] = "active" /\ Owner(r) = "b"})
    /\ slots = held["a"] + held["b"]
    /\ slots = (Capacity - 1) + Cardinality({r \in Requests : st[r] = "active"})

\* A request only waits while every slot is taken.
INV_WorkConserving == WaitingSet # {} => slots = Capacity

\* The waiting room, and each peer's share of it, stays within its bound.
INV_RoomBounded == /\ Cardinality(WaitingSet) <= Room
                   /\ \A p \in Peers : Cardinality(WaitingOf(p)) <= Share

\* No request is shed at once while the room had space for it.
INV_BusyOnlyWhenFull == ~badShed

\* No freed slot goes to a peer running more serves than another waiting peer.
INV_FairGrant == ~unfair

\* Every request that arrives is eventually served, answered busy, or closed.
LIVE_Answered == \A r \in Requests :
    (st[r] \in {"waiting", "active"}) ~> (st[r] \in {"served", "busy", "closed"})
====
