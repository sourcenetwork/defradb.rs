---------------------- MODULE ReplicatorIncarnation ----------------------
\* Rust: crates/db/src/merge/push_docs_replay.rs persist_replay_failures, which updates
\* status through crates/storage/src/stores/peerstore.rs set_replicator_if_incarnation.
\* Allocation happens outside the deciding transaction, so callers can arrive out of order.
EXTENDS Naturals
\* Error and rollback reproduce invalid stale allocations; create reproduces F48.
CONSTANTS Stale, ReplayMode
\* Two calls share a monotone allocator and one peer's durable incarnation.
Calls == {1, 2}
\* Finite identities and deadline ticks bound the explored execution.
\* sequence: Peerstore allocator; incarnation: ReplicatorIncarnationKey.
\* ids: ReplicatorInstall.id; phase and age: each caller's local install state.
\* pending: the persisted pending install owner; row: ReplicatorInfo status or absent.
VARIABLES sequence, incarnation, ids, phase, pending, age, row
\* Every transition accounts for the entire state.
vars == <<sequence, incarnation, ids, phase, pending, age, row>>
\* Both calls begin without an allocation or a pending replay.
Init == /\ sequence = 0 /\ incarnation = 0 /\ pending = 0 /\ row = "absent"
        /\ ids = [r \in Calls |-> 0]
        /\ phase = [r \in Calls |-> "allocate"]
        /\ age = [r \in Calls |-> 0]
\* An independent allocation can be delayed before its deciding transaction begins.
Allocate(r) == /\ phase[r] = "allocate" /\ sequence < 3
               /\ sequence' = sequence + 1
               /\ ids' = [ids EXCEPT ![r] = sequence']
               /\ phase' = [phase EXCEPT ![r] = "decide"]
               /\ UNCHANGED <<incarnation, pending, age, row>>
\* A stale allocation is retried through the existing deadline-bounded business decision.
Reallocate(r) == /\ phase[r] = "decide" /\ pending = 0 /\ ids[r] < incarnation
                 /\ Stale # "rollback"
                 /\ phase' = [phase EXCEPT ![r] = IF Stale = "error" THEN "failed" ELSE "allocate"]
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row>>
\* Pending installs block successors; the transaction cannot publish an older identity.
Install(r) == /\ phase[r] = "decide" /\ pending = 0
              /\ (ids[r] >= incarnation \/ Stale = "rollback")
              /\ incarnation' = ids[r] /\ pending' = r /\ row' = "active"
              /\ phase' = [phase EXCEPT ![r] = "running"]
              /\ UNCHANGED <<sequence, ids, age>>
\* Completion releases the peer for the next caller without changing its incarnation.
Finish(r) == /\ phase[r] = "running" /\ pending = r
             /\ phase' = [phase EXCEPT ![r] = "done"] /\ pending' = 0
             /\ UNCHANGED <<sequence, incarnation, ids, age, row>>
\* The outer deadline remains authoritative even if repeated allocations become stale.
Tick(r) == /\ phase[r] \in {"allocate", "decide"} /\ age[r] < 2
           /\ age' = [age EXCEPT ![r] = @ + 1]
           /\ phase' = IF age'[r] = 2 THEN [phase EXCEPT ![r] = "timeout"] ELSE phase
           /\ UNCHANGED <<sequence, incarnation, ids, pending, row>>
\* A failed replay reads its active row before attempting the status update.
ReplayRead(r) == /\ phase[r] = "running" /\ row = "active"
                 /\ phase' = [phase EXCEPT ![r] = "flipping"]
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row>>
\* The old create allocates a new identity; the guarded update uses the plan id.
ReplayFlip(r) == /\ phase[r] = "flipping"
                /\ (ReplayMode # "create" \/ sequence < 4)
                /\ phase' = [phase EXCEPT ![r] = "running"]
                /\ UNCHANGED <<ids, pending, age>>
                /\ IF ReplayMode = "create"
                      THEN /\ sequence' = sequence + 1 /\ incarnation' = sequence'
                           /\ row' = "inactive"
                      ELSE /\ UNCHANGED <<sequence, incarnation>>
                           /\ row' = IF row = "active" /\ incarnation = ids[r]
                                      THEN "inactive" ELSE row
\* Removing a peer deletes the row but retains the incarnation key.
Delete == /\ row # "absent" /\ row' = "absent"
          /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age>>
\* A separate create can replace the row while a replay is running.
Replace == /\ sequence < 4 /\ sequence' = sequence + 1
           /\ incarnation' = sequence' /\ row' = "active"
           /\ UNCHANGED <<ids, phase, pending, age>>
\* Interleavings include replay, deletion and an independent reconfiguration.
Next == (\E r \in Calls : Allocate(r) \/ Reallocate(r) \/ Install(r) \/ Finish(r) \/ Tick(r) \/ ReplayRead(r) \/ ReplayFlip(r)) \/ Delete \/ Replace
Spec == Init /\ [][Next]_vars
\* An unchanged install's deferred status update keeps the plan identity.
ACT_DeferredReplaySettlesClean == [][(\E r \in Calls : ReplayFlip(r) /\ row = "active" /\ incarnation = ids[r]) => incarnation' = incarnation]_vars
\* A replay returning after deletion cannot recreate the peer row.
ACT_NoResurrect == [][(\E r \in Calls : ReplayFlip(r) /\ row = "absent") => row' = "absent"]_vars
\* A replay for an older install cannot alter a replacement's status.
ACT_NoStaleFlip == [][(\E r \in Calls : ReplayFlip(r) /\ incarnation # ids[r]) => row' = row]_vars
\* Every durable incarnation change moves forward.
ACT_Monotone == [][incarnation' >= incarnation]_vars
\* Allocator ordering cannot become a permanent storage failure.
INV_Redecide == \A r \in Calls : phase[r] # "failed"
=============================================================================
