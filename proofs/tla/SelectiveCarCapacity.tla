---------------------- MODULE SelectiveCarCapacity ----------------------
\* A cache entry is installed before a send gets its guard; concurrent
\* registrations may temporarily exceed capacity before validation returns.
\* A completed send is only a warm entry. Reclaim it under pressure, never
\* an active or still-validating send. Its exact peer/root scope never widens.
\* Rust: sync/coordinator/selective_car_access.rs register and allows_root.
EXTENDS Naturals, FiniteSets, TLC
\* Each switch removes one real guard in a named negative configuration.
CONSTANTS ReclaimCompleted, ProtectActive, BoundPeer, BoundGlobal,
          ScopePeer, ScopeRoot, HonorExpiry, OnlyUnderPressure
\* Two sends share one peer; two other peers exercise the global limit.
Ids == 1..4
Peers == 1..3
Roots == 1..3
Peer(i) == IF i <= 2 THEN 1 ELSE i - 1
Root(i) == IF i % 2 = 0 THEN 2 ELSE 1
\* Small limits expose the same local/global pressure as the native constants.
PeerLimit == 1
GlobalLimit == 2
\* Phase belongs to the send; cache membership belongs to the shared map.
VARIABLES phase, cache, remaining, warmSeen, localSeen, globalSeen, interfered,
          refusedEligible, earlyEviction
vars == <<phase, cache, remaining, warmSeen, localSeen, globalSeen, interfered,
          refusedEligible, earlyEviction>>
\* No send or warm entry exists initially.
Init == /\ phase = [i \in Ids |-> "new"]
        /\ cache = {}
        /\ remaining = [i \in Ids |-> {}]
        /\ warmSeen = [i \in Ids |-> {}]
        /\ localSeen = [i \in Ids |-> FALSE]
        /\ globalSeen = [i \in Ids |-> FALSE]
        /\ interfered = [i \in Ids |-> FALSE]
        /\ refusedEligible = FALSE
        /\ earlyEviction = FALSE
\* Pending sends already have a non-expiring registration, like active sends.
InFlight == {i \in Ids : phase[i] \in {"pending", "active"}}
Warm == {i \in Ids : phase[i] \in {"completed", "expired"}}
\* Counts include provisional registrations until they validate or refuse.
ForPeer(entries, p) == {i \in entries : Peer(i) = p}
PeerOver(i) == Cardinality(ForPeer(cache, Peer(i))) > PeerLimit
GlobalOver == Cardinality(cache) > GlobalLimit
\* Return a guard only when both enabled capacity checks fit.
Fits(i) == (~BoundPeer \/ ~PeerOver(i)) /\ (~BoundGlobal \/ ~GlobalOver)
\* A peer at capacity must reclaim from its own warm entries first.
NeededCandidates(i) == {v \in cache \ {i} :
    (v \in warmSeen[i] \/ (~ProtectActive /\ v \notin remaining[i])) /\
    (~BoundPeer \/ ~PeerOver(i) \/ Peer(v) = Peer(i))}
\* Removal may race with the count read that observed admission pressure.
Candidates(i) == {v \in cache \ {i} :
    (v \in warmSeen[i] \/ (~ProtectActive /\ v \notin remaining[i])) /\
    (~OnlyUnderPressure \/ globalSeen[i] \/ (localSeen[i] /\ Peer(v) = Peer(i)))}
\* A mutation by another caller can invalidate the pass's bounded observations.
Disturb(actor) == [i \in Ids |-> interfered[i] \/ (i # actor /\ phase[i] = "pending")]
\* Only observed completed entries are reclaimable in a bounded scan.
\* A send finishing after its entry was scanned can be retried by its existing owner.
Eligible(i) ==
    /\ Cardinality(ForPeer(cache \ warmSeen[i], Peer(i))) <= PeerLimit
    /\ Cardinality(cache \ warmSeen[i]) <= GlobalLimit
\* Expiry can interleave with every map scan, but never revokes an active send.
Prune == /\ cache' = cache \ {i \in Ids : phase[i] = "expired"}
         /\ remaining' = [i \in Ids |-> remaining[i] \ {v \in Ids : phase[v] = "expired"}]
         /\ warmSeen' = [i \in Ids |-> warmSeen[i] \ {v \in Ids : phase[v] = "expired"}]
         /\ interfered' = IF cache' # cache THEN Disturb(0) ELSE interfered
         /\ UNCHANGED <<phase, localSeen, globalSeen,
                        refusedEligible, earlyEviction>>
\* Insert before validation, just as the native registration path does.
Insert(i) == /\ phase[i] = "new"
             \* Two overlapping passes expose the scan races in this bounded exploration.
             /\ Cardinality({j \in Ids : phase[j] = "pending"}) < 2
             /\ phase' = [phase EXCEPT ![i] = "pending"]
             /\ cache' = (cache \ {v \in Ids : phase[v] = "expired"}) \cup {i}
             /\ remaining' = [remaining EXCEPT ![i] =
                   (cache \cup {v \in Ids : phase[v] = "new"}) \ {i}]
             /\ warmSeen' = [warmSeen EXCEPT ![i] = {}]
             /\ localSeen' = [localSeen EXCEPT ![i] = FALSE]
             /\ globalSeen' = [globalSeen EXCEPT ![i] = FALSE]
             /\ interfered' = [Disturb(i) EXCEPT ![i] = FALSE]
             /\ UNCHANGED <<refusedEligible, earlyEviction>>
\* Entry observations interleave with completion, expiry and other registrations.
Observe(i, v) == /\ phase[i] = "pending" /\ v \in remaining[i]
                 /\ remaining' = [remaining EXCEPT ![i] = @ \ {v}]
                 /\ warmSeen' = [warmSeen EXCEPT ![i] =
                       IF v \in cache \cap Warm THEN @ \cup {v} ELSE @]
                 /\ UNCHANGED <<phase, cache, localSeen, globalSeen, interfered,
                                refusedEligible, earlyEviction>>
\* Capacity is an observation, not a lock held until reclamation finishes.
ObservePressure(i) == /\ phase[i] = "pending" /\ ~Fits(i)
                      /\ localSeen' = [localSeen EXCEPT ![i] =
                           (@ \/ (BoundPeer /\ PeerOver(i)))]
                      /\ globalSeen' = [globalSeen EXCEPT ![i] =
                           (@ \/ (BoundGlobal /\ GlobalOver))]
                      /\ UNCHANGED <<phase, cache, remaining, warmSeen, interfered,
                                     refusedEligible, earlyEviction>>
\* One completed observation is reclaimed per step; other sends may finish meanwhile.
Evict(i, v) ==
    /\ phase[i] = "pending" /\ ReclaimCompleted
    /\ (localSeen[i] \/ globalSeen[i] \/ ~OnlyUnderPressure)
    /\ v \in Candidates(i)
    /\ cache' = cache \ {v}
    \* A removed unique ID cannot reappear; other scans may skip its dead slot.
    /\ remaining' = [j \in Ids |-> remaining[j] \ {v}]
    /\ warmSeen' = [j \in Ids |-> warmSeen[j] \ {v}]
    /\ earlyEviction' = (earlyEviction \/ ~(localSeen[i] \/ globalSeen[i]))
    /\ interfered' = Disturb(i)
    /\ UNCHANGED <<phase, localSeen, globalSeen, refusedEligible>>
\* A successful validator hands its registered authority to the caller's guard.
Accept(i) == /\ phase[i] = "pending" /\ remaining[i] = {} /\ i \in cache /\ Fits(i)
             /\ phase' = [phase EXCEPT ![i] = "active"]
             \* Returning drops the caller's scan-local observations.
             /\ warmSeen' = [warmSeen EXCEPT ![i] = {}]
             /\ localSeen' = [localSeen EXCEPT ![i] = FALSE]
             /\ globalSeen' = [globalSeen EXCEPT ![i] = FALSE]
             /\ interfered' = [interfered EXCEPT ![i] = FALSE]
             /\ UNCHANGED <<cache, remaining, refusedEligible, earlyEviction>>
\* A refusal removes only its own provisional entry, after reclamation is exhausted.
Reject(i) == /\ phase[i] = "pending" /\ remaining[i] = {} /\ ~Fits(i)
             /\ (~ReclaimCompleted \/ interfered[i] \/ NeededCandidates(i) = {})
             /\ phase' = [phase EXCEPT ![i] = "rejected"]
             /\ cache' = cache \ {i}
             /\ refusedEligible' = (refusedEligible \/ (Eligible(i) /\ ~interfered[i]))
             /\ remaining' = [j \in Ids |-> remaining[j] \ {i}]
             /\ warmSeen' = [warmSeen EXCEPT ![i] = {}]
             /\ localSeen' = [localSeen EXCEPT ![i] = FALSE]
             /\ globalSeen' = [globalSeen EXCEPT ![i] = FALSE]
             /\ interfered' = [Disturb(i) EXCEPT ![i] = FALSE]
             /\ UNCHANGED earlyEviction
\* Dropping a guard starts the existing completed recovery window.
Complete(i) == /\ phase[i] = "active"
               /\ phase' = [phase EXCEPT ![i] = "completed"]
               /\ interfered' = Disturb(i)
               /\ UNCHANGED <<cache, remaining, warmSeen, localSeen, globalSeen,
                              refusedEligible, earlyEviction>>
\* Time ends a completed window without affecting any in-flight registration.
Expire(i) == /\ phase[i] = "completed"
             /\ phase' = [phase EXCEPT ![i] = "expired"]
             /\ interfered' = Disturb(i)
             /\ UNCHANGED <<cache, remaining, warmSeen, localSeen, globalSeen,
                            refusedEligible, earlyEviction>>
\* Every send can advance independently; map maintenance can interleave.
Next == Prune \/ (\E i \in Ids : Insert(i) \/ Accept(i) \/ Reject(i) \/
            Complete(i) \/ Expire(i) \/ ObservePressure(i) \/
            (\E v \in Ids : Observe(i, v) \/ Evict(i, v)))
Spec == Init /\ [][Next]_vars
\* The cache hit retains the native peer, exact-root and live-window checks.
Allows(p, r) == \E i \in cache :
    (~ScopePeer \/ Peer(i) = p) /\ (~ScopeRoot \/ Root(i) = r) /\
    (~HonorExpiry \/ phase[i] \in {"pending", "active", "completed"})
\* The independent exact-scope expectation names the granted authority only.
Expected(p, r) == \E i \in cache : Peer(i) = p /\ Root(i) = r /\
    phase[i] \in {"pending", "active", "completed"}
\* Maintenance never erases an in-flight send, including a provisional one.
INV_ActivePreserved == InFlight \subseteq cache
\* Capacity is stable when all concurrent validators have returned.
Quiescent == ~\E i \in Ids : phase[i] = "pending"
INV_PeerBound == Quiescent => \A p \in Peers :
    Cardinality(ForPeer(cache, p)) <= PeerLimit
INV_GlobalBound == Quiescent => Cardinality(cache) <= GlobalLimit
\* An uncontended pass cannot refuse space occupied only by observed warm entries.
INV_WarmDoesNotRefuse == ~refusedEligible
\* A warm entry is retained until expiry or observed admission pressure.
INV_WarmWithoutPressure == ~earlyEviction
\* A cache hit grants exactly the admitted peer/root pair while it is live.
INV_ExactScope == \A p \in Peers, r \in Roots : Allows(p, r) = Expected(p, r)
=============================================================================
