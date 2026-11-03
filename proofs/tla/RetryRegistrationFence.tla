---- MODULE RetryRegistrationFence ----
\* Allocation and registration are separate: tokens can commit out of order.
\* A committed head fans out to every listed replicator one registration at a
\* time; each registration commits or is rejected (fence or storage fault).
EXTENDS Naturals, FiniteSets
CONSTANT Peers, Background, MaxChurn, Faults, CheckEpoch, CheckOrder, FanOut
ASSUME Faults \in BOOLEAN /\ FanOut \in {"PerPeer", "AbortOnFirstFail"}
VARIABLES present, epoch, churn, allocated, owner, staged, captured, marker,
          unsafeCommit, phase, listed, mine, outcome, queued
vars == <<present, epoch, churn, allocated, owner, staged, captured, marker,
          unsafeCommit, phase, listed, mine, outcome, queued>>
Tokens == 1..(Background + Cardinality(Peers))
NoPeer == "none"
Rejected == {"gone", "stale", "covered", "failed"}
FanTokens == Cardinality({p \in Peers : mine[p] # 0})
fenceVars == <<present, epoch, churn, allocated, owner, staged, captured, marker, unsafeCommit>>
fanVars == <<phase, listed, mine, outcome, queued>>
Init == /\ present = [p \in Peers |-> TRUE] /\ epoch = [p \in Peers |-> 1]
        /\ churn = 0 /\ allocated = 0 /\ owner = [g \in Tokens |-> NoPeer]
        /\ staged = {} /\ captured = [g \in Tokens |-> 0]
        /\ marker = [p \in Peers |-> 0] /\ unsafeCommit = FALSE
        /\ phase = "idle" /\ listed = {} /\ mine = [p \in Peers |-> 0]
        /\ outcome = [p \in Peers |-> "none"] /\ queued = {}
Stage(p) == /\ allocated' = allocated + 1
            /\ owner' = [owner EXCEPT ![allocated + 1] = p]
            /\ staged' = staged \cup {allocated + 1}
            /\ captured' = [captured EXCEPT ![allocated + 1] = epoch[p]]
\* Another writer's registration for the same peer scope.
Allocate(p) == /\ present[p] /\ allocated - FanTokens < Background /\ Stage(p)
               /\ UNCHANGED <<present, epoch, churn, marker, unsafeCommit>> /\ UNCHANGED fanVars
Forget(p) == /\ present[p] /\ churn < MaxChurn
             /\ present' = [present EXCEPT ![p] = FALSE] /\ marker' = [marker EXCEPT ![p] = 0]
             /\ churn' = churn + 1
             /\ UNCHANGED <<epoch, allocated, owner, staged, captured, unsafeCommit>> /\ UNCHANGED fanVars
Readd(p) == /\ ~present[p] /\ churn < MaxChurn
            /\ present' = [present EXCEPT ![p] = TRUE] /\ epoch' = [epoch EXCEPT ![p] = @ + 1]
            /\ churn' = churn + 1
            /\ UNCHANGED <<allocated, owner, staged, captured, marker, unsafeCommit>> /\ UNCHANGED fanVars
\* create_replicator rewrites a present peer's incarnation and keeps its markers.
Reincarnate(p) == /\ present[p] /\ churn < MaxChurn
                  /\ epoch' = [epoch EXCEPT ![p] = @ + 1] /\ churn' = churn + 1
                  /\ UNCHANGED <<present, allocated, owner, staged, captured, marker, unsafeCommit>>
                  /\ UNCHANGED fanVars
Fence(g) == LET p == owner[g] IN
              /\ present[p]
              /\ (~CheckEpoch \/ captured[g] = epoch[p])
              /\ (~CheckOrder \/ g >= marker[p])
Reason(g) == LET p == owner[g] IN
               IF ~present[p] THEN "gone"
               ELSE IF CheckEpoch /\ captured[g] # epoch[p] THEN "stale" ELSE "covered"
Decide(g, result) == outcome' = IF mine[owner[g]] = g
                                  THEN [outcome EXCEPT ![owner[g]] = result] ELSE outcome
Commit(g) == /\ g \in staged /\ Fence(g)
             /\ LET p == owner[g] IN
                  /\ marker' = [marker EXCEPT ![p] = g]
                  /\ unsafeCommit' = (unsafeCommit \/ captured[g] # epoch[p] \/ g < marker[p])
             /\ staged' = staged \ {g} /\ Decide(g, "registered")
             /\ UNCHANGED <<present, epoch, churn, allocated, owner, captured>>
             /\ UNCHANGED <<phase, listed, mine, queued>>
Reject(g) == /\ g \in staged /\ ~Fence(g)
             /\ staged' = staged \ {g} /\ Decide(g, Reason(g))
             /\ UNCHANGED <<present, epoch, churn, allocated, owner, captured, marker, unsafeCommit>>
             /\ UNCHANGED <<phase, listed, mine, queued>>
Fault(g) == /\ Faults /\ g \in staged /\ mine[owner[g]] = g
            /\ staged' = staged \ {g} /\ Decide(g, "failed")
            /\ UNCHANGED <<present, epoch, churn, allocated, owner, captured, marker, unsafeCommit>>
            /\ UNCHANGED <<phase, listed, mine, queued>>
\* broadcast.rs push_jobs: the caller snapshots the replicator list.
List == /\ phase = "idle" /\ phase' = "register"
        /\ listed' = {p \in Peers : present[p]}
        /\ UNCHANGED fenceVars /\ UNCHANGED <<mine, outcome, queued>>
\* admit_replicator_push awaits each registration before starting the next.
FanRegister(p) ==
  /\ phase = "register" /\ p \in listed /\ outcome[p] = "none" /\ mine[p] = 0
  /\ \A q \in listed : mine[q] # 0 => outcome[q] # "none"
  /\ FanOut = "PerPeer" \/ \A q \in listed : outcome[q] \notin Rejected
  /\ IF present[p]
       THEN /\ Stage(p) /\ mine' = [mine EXCEPT ![p] = allocated + 1]
            /\ UNCHANGED outcome
       ELSE /\ outcome' = [outcome EXCEPT ![p] = "gone"]
            /\ UNCHANGED <<allocated, owner, staged, captured, mine>>
  /\ UNCHANGED <<present, epoch, churn, marker, unsafeCommit, phase, listed, queued>>
AbortFanOut == /\ FanOut = "AbortOnFirstFail" /\ phase = "register"
               /\ \E p \in listed : outcome[p] \in Rejected
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
TypeOK == /\ present \in [Peers -> BOOLEAN] /\ epoch \in [Peers -> 1..(1 + MaxChurn)]
          /\ churn \in 0..MaxChurn /\ allocated \in 0..Cardinality(Tokens)
          /\ owner \in [Tokens -> Peers \cup {NoPeer}] /\ staged \subseteq Tokens
          /\ captured \in [Tokens -> 0..(1 + MaxChurn)] /\ marker \in [Peers -> 0..Cardinality(Tokens)]
          /\ unsafeCommit \in BOOLEAN /\ phase \in {"idle", "register", "done"}
          /\ listed \subseteq Peers /\ mine \in [Peers -> 0..Cardinality(Tokens)]
          /\ outcome \in [Peers -> {"none", "registered"} \cup Rejected] /\ queued \subseteq Peers
INV_RegistrationCannotCrossFence == ~unsafeCommit
\* Each listed peer is decided on its own; exactly the registered ones are queued.
INV_FanOutPeerLocal ==
  phase = "done" => \A p \in listed : outcome[p] # "none" /\ (p \in queued <=> outcome[p] = "registered")
LIVE_FanOutDecides == <>(phase = "done")
====
