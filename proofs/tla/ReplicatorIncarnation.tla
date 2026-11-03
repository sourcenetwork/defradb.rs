---------------------- MODULE ReplicatorIncarnation ----------------------
\* Allocation happens outside the deciding transaction, so callers can arrive out of order.
EXTENDS Naturals
\* Error and rollback reproduce the two invalid responses to a stale allocation.
CONSTANT Stale
\* Two calls share a monotone allocator and one peer's durable incarnation.
Calls == {1, 2}
\* Finite identities and deadline ticks bound the explored execution.
VARIABLES sequence, incarnation, previous, ids, phase, pending, age
\* Every transition accounts for the entire state.
vars == <<sequence, incarnation, previous, ids, phase, pending, age>>
\* Both calls begin without an allocation or a pending replay.
Init == /\ sequence = 0 /\ incarnation = 0 /\ previous = 0 /\ pending = 0
        /\ ids = [r \in Calls |-> 0]
        /\ phase = [r \in Calls |-> "allocate"]
        /\ age = [r \in Calls |-> 0]
\* An independent allocation can be delayed before its deciding transaction begins.
Allocate(r) == /\ phase[r] = "allocate" /\ sequence < 3
               /\ sequence' = sequence + 1
               /\ ids' = [ids EXCEPT ![r] = sequence']
               /\ phase' = [phase EXCEPT ![r] = "decide"]
               /\ UNCHANGED <<incarnation, previous, pending, age>>
\* A stale allocation is retried through the existing deadline-bounded business decision.
Reallocate(r) == /\ phase[r] = "decide" /\ pending = 0 /\ ids[r] < incarnation
                 /\ Stale # "rollback"
                 /\ phase' = [phase EXCEPT ![r] = IF Stale = "error" THEN "failed" ELSE "allocate"]
                 /\ UNCHANGED <<sequence, incarnation, previous, ids, pending, age>>
\* Pending installs block successors; the transaction cannot publish an older identity.
Install(r) == /\ phase[r] = "decide" /\ pending = 0
              /\ (ids[r] >= incarnation \/ Stale = "rollback")
              /\ previous' = incarnation /\ incarnation' = ids[r] /\ pending' = r
              /\ phase' = [phase EXCEPT ![r] = "running"]
              /\ UNCHANGED <<sequence, ids, age>>
\* Completion releases the peer for the next caller without changing its incarnation.
Finish(r) == /\ phase[r] = "running" /\ pending = r
             /\ phase' = [phase EXCEPT ![r] = "done"] /\ pending' = 0
             /\ UNCHANGED <<sequence, incarnation, previous, ids, age>>
\* The outer deadline remains authoritative even if repeated allocations become stale.
Tick(r) == /\ phase[r] \in {"allocate", "decide"} /\ age[r] < 2
           /\ age' = [age EXCEPT ![r] = @ + 1]
           /\ phase' = IF age'[r] = 2 THEN [phase EXCEPT ![r] = "timeout"] ELSE phase
           /\ UNCHANGED <<sequence, incarnation, previous, ids, pending>>
\* Interleavings include the larger identity finishing before the smaller identity decides.
Next == \E r \in Calls : Allocate(r) \/ Reallocate(r) \/ Install(r) \/ Finish(r) \/ Tick(r)
\* Allocator ordering cannot become a permanent storage failure.
INV_Redecide == \A r \in Calls : phase[r] # "failed"
\* A completed newer install cannot be overwritten with a smaller identity.
INV_Monotone == incarnation >= previous
=============================================================================
