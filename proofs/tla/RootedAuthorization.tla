---------------------- MODULE RootedAuthorization ----------------------
\* Two callers ask for the same rooted walk; a third asks for another one.
\* A caller takes saved state, owns it while reading, then may save it again.
\* Duplicates join its completion event, not a lock around the blockstore.
\* Saving is optional: a full cache must never change the reached grants.
\* Rust: car_authorization.rs walk_with_limits and car_authorization_registry.rs.
EXTENDS Naturals, FiniteSets, TLC
\* Each switch removes one guard in a RED configuration.
CONSTANTS SingleFlight, ResumeCursor, FenceEviction, AtomicReserve,
          BudgetOnlyRetention, WakeOnCancel, ReachedOnly, BoundSlots
\* Three requests and two exact root/want-list keys keep all races small.
Requests == 1..3
Keys == 1..2
\* Requests one and two are duplicates; request three competes for cache space.
Key(r) == IF r = 3 THEN 2 ELSE 1
\* A saved cursor has an identity, traversal depth and reservation charge.
Empty == [generation |-> 0, depth |-> 0, charge |-> 0]
\* Two CID units and one entry also exercise a cursor too large to cache.
Limit == 2
SlotLimit == 1
\* The root is reached first, then its available child. CID three is unrelated.
Reached(d) == IF d = 0 THEN {} ELSE IF d = 1 THEN {1} ELSE {1, 2}
\* Phase and traversal state belong to the request; idle state belongs to the cache.
VARIABLES phase, depth, idle, flight, token, nextToken, reserved, used,
          eventReady, ownerToken, answer, grants, observed, staleEviction, lostResume
\* This is the full state changed by a step.
vars == <<phase, depth, idle, flight, token, nextToken, reserved, used,
          eventReady, ownerToken, answer, grants, observed, staleEviction, lostResume>>
\* Every request begins before checkout, with no saved or active state.
Init == /\ phase = [r \in Requests |-> "ready"]
        /\ depth = [r \in Requests |-> 0]
        /\ idle = [k \in Keys |-> Empty]
        /\ flight = [k \in Keys |-> 0]
        /\ token = [r \in Requests |-> 0]
        /\ nextToken = 1
        /\ reserved = [r \in Requests |-> 0]
        /\ used = 0
        /\ eventReady = [t \in 1..3 |-> FALSE]
        /\ ownerToken = [r \in Requests |-> 0]
        /\ answer = [r \in Requests |-> "none"]
        /\ grants = [r \in Requests |-> {}]
        /\ observed = [k \in Keys |-> 0]
        /\ staleEviction = FALSE
        /\ lostResume = FALSE
\* The owner takes a cursor out of the cache and releases its cache charge.
Start(r) == /\ phase[r] = "ready" /\ nextToken <= 3
            /\ (flight[Key(r)] = 0 \/ ~SingleFlight)
            /\ phase' = [phase EXCEPT ![r] = "walking"]
            /\ depth' = [depth EXCEPT ![r] = IF ResumeCursor THEN idle[Key(r)].depth ELSE 0]
            /\ lostResume' = (lostResume \/ (~ResumeCursor /\ idle[Key(r)].depth > 0))
            /\ used' = used - idle[Key(r)].charge
            /\ idle' = [idle EXCEPT ![Key(r)] = Empty]
            /\ flight' = [flight EXCEPT ![Key(r)] = nextToken]
            /\ token' = [token EXCEPT ![r] = nextToken]
            /\ ownerToken' = [ownerToken EXCEPT ![r] = nextToken]
            /\ nextToken' = nextToken + 1
            /\ UNCHANGED <<reserved, eventReady, answer, grants, observed, staleEviction>>
\* A duplicate observes one completion event without taking the owner's state.
Join(r) == /\ phase[r] = "ready" /\ flight[Key(r)] # 0
           /\ phase' = [phase EXCEPT ![r] = "waiting"]
           /\ token' = [token EXCEPT ![r] = flight[Key(r)]]
           /\ UNCHANGED <<depth, idle, flight, nextToken, reserved, used, eventReady,
                          ownerToken, answer, grants, observed, staleEviction, lostResume>>
\* Each read reaches one more available block, never the unrelated CID.
Read(r) == /\ phase[r] = "walking" /\ depth[r] < 2
           /\ depth' = [depth EXCEPT ![r] = @ + 1]
           /\ UNCHANGED <<phase, idle, flight, token, nextToken, reserved, used,
                          eventReady, ownerToken, answer, grants, observed, staleEviction, lostResume>>
\* The same save path runs on normal completion, a retryable stop, and cancellation.
End(r, cancelled) ==
    /\ phase[r] = "walking"
    /\ phase' = [phase EXCEPT ![r] = "reserving"]
    /\ answer' = [answer EXCEPT ![r] = IF cancelled THEN "cancelled"
                                      ELSE IF depth[r] = 2 THEN "ok" ELSE "retry"]
    /\ grants' = [grants EXCEPT ![r] = IF ReachedOnly THEN Reached(depth[r]) ELSE {1, 2, 3}]
    /\ UNCHANGED <<depth, idle, flight, token, nextToken, reserved, used,
                   eventReady, ownerToken, observed, staleEviction, lostResume>>
\* Reservation is one atomic decision, made only when saving idle state.
Reserve(r) ==
    /\ phase[r] = "reserving"
    /\ LET charge == depth[r] + 1
           slots == Cardinality({k \in Keys : idle[k].charge > 0})
                    + Cardinality({s \in Requests : reserved[s] > 0})
           fits == used + charge <= Limit /\ slots < SlotLimit
           take == (used + charge <= Limit \/ ~AtomicReserve)
                   /\ (slots < SlotLimit \/ ~BoundSlots)
       IN /\ reserved' = [reserved EXCEPT ![r] = IF take THEN charge ELSE 0]
          /\ used' = IF take THEN used + charge ELSE used
          /\ answer' = [answer EXCEPT ![r] =
                IF ~BudgetOnlyRetention /\ ~fits /\ answer[r] = "ok" THEN "budget-failed" ELSE @]
    /\ phase' = [phase EXCEPT ![r] = "publishing"]
    /\ UNCHANGED <<depth, idle, flight, token, nextToken, eventReady,
                   ownerToken, grants, observed, staleEviction, lostResume>>
\* Publication transfers, rather than duplicates, a reservation into the idle map.
Publish(r) ==
    /\ phase[r] = "publishing"
    /\ idle' = [idle EXCEPT ![Key(r)] = IF reserved[r] = 0 THEN @ ELSE
          [generation |-> token[r], depth |-> depth[r], charge |-> reserved[r]]]
    /\ reserved' = [reserved EXCEPT ![r] = 0]
    /\ phase' = [phase EXCEPT ![r] = "finishing"]
    /\ UNCHANGED <<depth, flight, token, nextToken, used, eventReady,
                   ownerToken, answer, grants, observed, staleEviction, lostResume>>
\* Letting go always wakes joined callers, including when the owner was cancelled.
Finish(r) ==
    /\ phase[r] = "finishing"
    /\ flight' = [flight EXCEPT ![Key(r)] = 0]
    /\ eventReady' = [eventReady EXCEPT ![token[r]] = WakeOnCancel \/ answer[r] # "cancelled"]
    /\ phase' = [phase EXCEPT ![r] = "done"]
    /\ UNCHANGED <<depth, idle, token, nextToken, reserved, used,
                   ownerToken, answer, grants, observed, staleEviction, lostResume>>
\* A waiter settles on the event or its own deadline, never by owning the cursor.
Wait(r) == /\ phase[r] = "waiting"
           /\ phase' = [phase EXCEPT ![r] = "done"]
           /\ answer' = [answer EXCEPT ![r] = IF eventReady[token[r]] THEN "joined" ELSE "deadline"]
           /\ UNCHANGED <<depth, idle, flight, token, nextToken, reserved, used, eventReady,
                          ownerToken, grants, observed, staleEviction, lostResume>>
\* An expiry or oldest-entry scan can hold a stale observation while a new cursor arrives.
Observe(k) == /\ idle[k].generation # 0
              /\ observed' = [observed EXCEPT ![k] = idle[k].generation]
              /\ UNCHANGED <<phase, depth, idle, flight, token, nextToken, reserved, used,
                             eventReady, ownerToken, answer, grants, staleEviction, lostResume>>
\* Conditional removal refuses to erase a new cursor seen through an old observation.
Evict(k) == /\ observed[k] # 0 /\ idle[k].generation # 0
            /\ (~FenceEviction \/ observed[k] = idle[k].generation)
            /\ staleEviction' = (staleEviction \/ observed[k] # idle[k].generation)
            /\ used' = used - idle[k].charge
            /\ idle' = [idle EXCEPT ![k] = Empty]
            /\ UNCHANGED <<phase, depth, flight, token, nextToken, reserved, eventReady,
                           ownerToken, answer, grants, observed, lostResume>>
\* A request advances through a finite pass, even when it is cancelled or times out.
RequestStep(r) == Start(r) \/ Join(r) \/ Read(r) \/ End(r, FALSE) \/ End(r, TRUE)
                  \/ Reserve(r) \/ Publish(r) \/ Finish(r) \/ Wait(r)
\* Cache maintenance interleaves freely with requests.
Next == (\E r \in Requests : RequestStep(r)) \/ (\E k \in Keys : Observe(k) \/ Evict(k))
\* A runnable request is eventually polled; no fairness is assumed of optional eviction.
Spec == Init /\ [][Next]_vars /\ (\A r \in Requests : WF_vars(RequestStep(r)))
\* Exactly one producer runs for each key; duplicates only wait for its event.
INV_OneOwner == \A a, b \in Requests :
    (phase[a] \in {"walking", "reserving", "publishing", "finishing"}
     /\ phase[b] \in {"walking", "reserving", "publishing", "finishing"}
     /\ Key(a) = Key(b)) => a = b
\* The shared reservation count never passes its limit.
INV_Bounded == used <= Limit
\* Reservation also bounds the number of entries before they are published.
INV_SlotsBounded == Cardinality({k \in Keys : idle[k].charge > 0})
                   + Cardinality({r \in Requests : reserved[r] > 0}) <= SlotLimit
\* Every charge belongs either to an idle cursor or to an unpublished reservation.
INV_ExactCharge == used = idle[1].charge + idle[2].charge + reserved[1] + reserved[2] + reserved[3]
\* A full cache cannot turn a successful, fully-read pass into an error.
INV_BudgetDoesNotReject == \A r \in Requests : answer[r] # "budget-failed"
\* The result contains no CID the walk has not reached.
INV_ReachedOnly == \A r \in Requests : grants[r] \subseteq Reached(depth[r])
\* A stale maintenance observation never removes a replacement cursor.
INV_NewCursorSurvives == ~staleEviction
\* Taking saved state never starts behind the cursor that was taken.
INV_ResumePreserved == ~lostResume
\* Every completed producer signals the event its duplicates may be awaiting.
INV_CompletionWakes == \A r \in Requests :
    (phase[r] = "done" /\ ownerToken[r] # 0) => eventReady[ownerToken[r]]
\* With polling and a deadline, each of these three requests eventually settles.
LIVE_Settles == \A r \in Requests : <>(phase[r] = "done")
=============================================================================
