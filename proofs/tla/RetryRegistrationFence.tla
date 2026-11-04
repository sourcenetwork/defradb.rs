---- MODULE RetryRegistrationFence ----
\* Allocation and registration are separate: tokens can commit out of order.
\* A committed head fans out to every listed replicator one registration at a
\* time; each registration commits or is rejected (fence or storage fault).
\*
\* State and its code (crates/storage/src/stores/peerstore.rs unless named):
\*   present[p]     the ReplicatorKey row (create_replicator, delete_replicator)
\*   epoch[p]       the ReplicatorIncarnationKey value; only equality is read
\*   allocated      the /rep/retry/marker-generation sequence
\*   staged         RegisterScopeMarkerAttempts allocated and not yet decided;
\*                  owner[g], captured[g] are their peer_id, expected_incarnation
\*   marker[p]      the scope's retry marker generation, 0 when absent
\*   phase, listed  admit_replicator_push (p2p/src/sync/coordinator/broadcast.rs)
\*                  before, inside and after its loop over push_jobs
\*   mine[p]        the generation that loop allocated for p's job, 0 for none
\*   outcome[p]     p's job: undecided, marker_generation set, or in unregistered
\*   queued         the jobs passed to enqueue_replicator_push
EXTENDS Naturals, FiniteSets
CONSTANT Peers, Background, MaxEpoch, Faults, CheckEpoch, CheckOrder, FanOut
ASSUME Faults \in BOOLEAN /\ FanOut \in {"PerPeer", "AbortOnFirstFail"}
VARIABLES present, epoch, allocated, owner, staged, captured, marker,
          phase, listed, mine, outcome, queued
vars == <<present, epoch, allocated, owner, staged, captured, marker,
          phase, listed, mine, outcome, queued>>
Tokens == 1..(Background + Cardinality(Peers))
NoPeer == "none"
FanTokens == Cardinality({p \in Peers : mine[p] # 0})
fenceVars == <<present, epoch, allocated, owner, staged, captured, marker>>
fanVars == <<phase, listed, mine, outcome, queued>>
Init == /\ present = [p \in Peers |-> TRUE] /\ epoch = [p \in Peers |-> 1]
        /\ allocated = 0 /\ owner = [g \in Tokens |-> NoPeer]
        /\ staged = {} /\ captured = [g \in Tokens |-> 0]
        /\ marker = [p \in Peers |-> 0]
        /\ phase = "idle" /\ listed = {} /\ mine = [p \in Peers |-> 0]
        /\ outcome = [p \in Peers |-> "none"] /\ queued = {}
Stage(p) == /\ allocated' = allocated + 1
            /\ owner' = [owner EXCEPT ![allocated + 1] = p]
            /\ staged' = staged \cup {allocated + 1}
            /\ captured' = [captured EXCEPT ![allocated + 1] = epoch[p]]
\* Another writer's registration for the same peer scope.
Allocate(p) == /\ present[p] /\ allocated - FanTokens < Background /\ Stage(p)
               /\ UNCHANGED <<present, epoch, marker>> /\ UNCHANGED fanVars
Forget(p) == /\ present[p]
             /\ present' = [present EXCEPT ![p] = FALSE] /\ marker' = [marker EXCEPT ![p] = 0]
             /\ UNCHANGED <<epoch, allocated, owner, staged, captured>> /\ UNCHANGED fanVars
Readd(p) == /\ ~present[p] /\ epoch[p] < MaxEpoch
            /\ present' = [present EXCEPT ![p] = TRUE] /\ epoch' = [epoch EXCEPT ![p] = @ + 1]
            /\ UNCHANGED <<allocated, owner, staged, captured, marker>> /\ UNCHANGED fanVars
\* create_replicator rewrites a present peer's incarnation and keeps its markers.
Reincarnate(p) == /\ present[p] /\ epoch[p] < MaxEpoch
                  /\ epoch' = [epoch EXCEPT ![p] = @ + 1]
                  /\ UNCHANGED <<present, allocated, owner, staged, captured, marker>>
                  /\ UNCHANGED fanVars
Fence(g) == LET p == owner[g] IN
              /\ present[p]
              /\ (~CheckEpoch \/ captured[g] = epoch[p])
              /\ (~CheckOrder \/ g >= marker[p])
Decide(g, result) == outcome' = IF mine[owner[g]] = g
                                  THEN [outcome EXCEPT ![owner[g]] = result] ELSE outcome
Commit(g) == /\ g \in staged /\ Fence(g)
             /\ marker' = [marker EXCEPT ![owner[g]] = g]
             /\ staged' = staged \ {g} /\ Decide(g, "registered")
             /\ UNCHANGED <<present, epoch, allocated, owner, captured>>
             /\ UNCHANGED <<phase, listed, mine, queued>>
Reject(g) == /\ g \in staged /\ ~Fence(g)
             /\ staged' = staged \ {g} /\ Decide(g, "unregistered")
             /\ UNCHANGED <<present, epoch, allocated, owner, captured, marker>>
             /\ UNCHANGED <<phase, listed, mine, queued>>
Fault(g) == /\ Faults /\ g \in staged /\ mine[owner[g]] = g
            /\ staged' = staged \ {g} /\ Decide(g, "unregistered")
            /\ UNCHANGED <<present, epoch, allocated, owner, captured, marker>>
            /\ UNCHANGED <<phase, listed, mine, queued>>
\* broadcast.rs push_jobs: the caller snapshots the replicator list.
List == /\ phase = "idle" /\ phase' = "register"
        /\ listed' = {p \in Peers : present[p]}
        /\ UNCHANGED fenceVars /\ UNCHANGED <<mine, outcome, queued>>
\* admit_replicator_push awaits each registration before starting the next.
FanRegister(p) ==
  /\ phase = "register" /\ p \in listed /\ outcome[p] = "none" /\ mine[p] = 0
  /\ \A q \in listed : mine[q] # 0 => outcome[q] # "none"
  /\ FanOut = "PerPeer" \/ \A q \in listed : outcome[q] # "unregistered"
  /\ IF present[p]
       THEN /\ Stage(p) /\ mine' = [mine EXCEPT ![p] = allocated + 1]
            /\ UNCHANGED outcome
       ELSE /\ outcome' = [outcome EXCEPT ![p] = "unregistered"]
            /\ UNCHANGED <<allocated, owner, staged, captured, mine>>
  /\ UNCHANGED <<present, epoch, marker, phase, listed, queued>>
AbortFanOut == /\ FanOut = "AbortOnFirstFail" /\ phase = "register"
               /\ \E p \in listed : outcome[p] = "unregistered"
               /\ phase' = "done"
               /\ UNCHANGED fenceVars /\ UNCHANGED <<listed, mine, outcome, queued>>
Enqueue == /\ phase = "register"
           /\ \A p \in listed : outcome[p] # "none"
           /\ FanOut = "PerPeer" \/ \A p \in listed : outcome[p] = "registered"
           /\ queued' = {p \in listed : outcome[p] = "registered"}
           /\ phase' = "done"
           /\ UNCHANGED fenceVars /\ UNCHANGED <<listed, mine, outcome>>
FanStep == \/ List \/ (\E p \in Peers : FanRegister(p))
           \/ (\E p \in Peers : mine[p] \in staged /\ (Commit(mine[p]) \/ Reject(mine[p])))
           \/ AbortFanOut \/ Enqueue
Next == \/ FanStep
        \/ \E p \in Peers : Allocate(p) \/ Forget(p) \/ Readd(p) \/ Reincarnate(p)
        \/ \E g \in Tokens : Commit(g) \/ Reject(g) \/ Fault(g)
Spec == Init /\ [][Next]_vars /\ WF_vars(FanStep)
TypeOK == /\ present \in [Peers -> BOOLEAN] /\ epoch \in [Peers -> 1..MaxEpoch]
          /\ allocated \in 0..Cardinality(Tokens)
          /\ owner \in [Tokens -> Peers \cup {NoPeer}] /\ staged \subseteq Tokens
          /\ captured \in [Tokens -> 0..MaxEpoch] /\ marker \in [Peers -> 0..Cardinality(Tokens)]
          /\ phase \in {"idle", "register", "done"}
          /\ listed \subseteq Peers /\ mine \in [Peers -> 0..Cardinality(Tokens)]
          /\ outcome \in [Peers -> {"none", "registered", "unregistered"}] /\ queued \subseteq Peers
\* A registration lands only on the incarnation it captured, never below the marker.
ACT_RegistrationCannotCrossFence ==
  [][\A g \in staged : marker'[owner[g]] = g =>
       /\ captured[g] = epoch[owner[g]]
       /\ g >= marker[owner[g]]]_vars
\* Each listed peer is decided on its own; exactly the registered ones are queued.
INV_FanOutPeerLocal ==
  phase = "done" => \A p \in listed : outcome[p] # "none" /\ (p \in queued <=> outcome[p] = "registered")
LIVE_FanOutDecides == <>(phase = "done")
====
