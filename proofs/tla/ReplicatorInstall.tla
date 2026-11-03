------------------------ MODULE ReplicatorInstall ------------------------
\* Two callers install one peer. The row and its replay plan commit together.
\* Only a settled replay may finish; recovery settles interrupted work as an error.
\* Rust: p2p-adapter/src/replicator_install_store.rs and replicator_installs.rs.
EXTENDS Naturals, FiniteSets
\* Each switch plants one missing guard for a named RED witness.
CONSTANT Bug
\* Requests also serve as unique, preallocated install identities.
Requests == {1, 2}
\* Version and pending are durable; snapshots and running tasks are volatile.
VARIABLES version, pending, snapshots, stage, settled, completed, events, badSkip, stale, age, expired
\* The complete state tuple lets actions say which fields they leave alone.
vars == <<version, pending, snapshots, stage, settled, completed, events, badSkip, stale, age, expired>>
\* Start without a row or install, and with both callers ready.
Init == /\ version = 0 /\ pending = 0
        /\ snapshots = [r \in Requests |-> 0]
        /\ stage = [r \in Requests |-> "ready"]
        /\ settled = {} /\ completed = {}
        /\ events = [r \in Requests |-> 0]
        /\ badSkip = FALSE /\ stale = FALSE
        /\ age = [r \in Requests |-> 0] /\ expired = {}
\* One engine attempt reads the row's version and the active replay together.
Read(r) == /\ stage[r] = "ready"
           /\ stage' = [stage EXCEPT ![r] = IF pending = 0 \/ Bug = "active" THEN "read" ELSE "wait"]
           /\ snapshots' = [snapshots EXCEPT ![r] = version]
           /\ UNCHANGED <<version, pending, settled, completed, events, badSkip, stale, age, expired>>
\* A stale deciding read retries in the engine, not in application code.
Retry(r) == /\ stage[r] = "read" /\ snapshots[r] # version
            /\ stage' = [stage EXCEPT ![r] = "ready"]
            /\ UNCHANGED <<version, pending, snapshots, settled, completed, events, badSkip, stale, age, expired>>
\* Committing installs both the row version and its pending replay plan.
Install(r) == /\ stage[r] = "read"
              /\ (Bug = "validation" \/ snapshots[r] = version)
              /\ (Bug = "active" \/ pending = 0)
              /\ version' = version + 1 /\ pending' = r
              /\ stage' = [stage EXCEPT ![r] = "run"]
              /\ stale' = (stale \/ snapshots[r] # version)
              /\ badSkip' = (badSkip \/ pending # 0)
              /\ UNCHANGED <<snapshots, settled, completed, events, age, expired>>
\* A successful replay has joined every push before it can finish.
Settle(r) == /\ stage[r] = "run"
             /\ settled' = settled \cup {r}
             /\ stage' = [stage EXCEPT ![r] = "settled"]
             /\ UNCHANGED <<version, pending, snapshots, completed, events, badSkip, stale, age, expired>>
\* The finish commit fences the identity, records the outcome, then emits once.
Finish(r) == /\ stage[r] \in {"settled", "crashed", "done"}
             /\ (Bug = "fence" \/ pending = r \/ (Bug = "duplicate" /\ r \in completed))
             /\ (Bug \in {"duplicate", "fence"} \/ r \notin completed)
             /\ (Bug = "settle" \/ r \in settled)
             /\ pending' = 0 /\ completed' = completed \cup {r}
             /\ events' = [events EXCEPT ![r] = @ + 1]
             /\ stage' = [stage EXCEPT ![r] = "done"]
             /\ badSkip' = (badSkip \/ (pending # 0 /\ pending # r))
             /\ UNCHANGED <<version, snapshots, settled, stale, age, expired>>
\* A restart loses the task, never the durable pending install.
Crash(r) == /\ stage[r] = "run"
            /\ stage' = [stage EXCEPT ![r] = "crashed"]
            /\ pending' = IF Bug = "durable" THEN 0 ELSE pending
            /\ UNCHANGED <<version, snapshots, settled, completed, events, badSkip, stale, age, expired>>
\* Startup records the interruption as a failure, not a successful backfill.
Recover(r) == /\ (stage[r] = "crashed" \/ (Bug = "recovery" /\ stage[r] = "run")) /\ pending = r
              /\ settled' = settled \cup {r}
              /\ stage' = [stage EXCEPT ![r] = "settled"]
              /\ badSkip' = (badSkip \/ stage[r] = "run")
              /\ UNCHANGED <<version, pending, snapshots, completed, events, stale, age, expired>>
\* A waiter rechecks storage after a notification; notification is only a hint.
Wake(r) == /\ stage[r] = "wait" /\ pending = 0
           /\ stage' = [stage EXCEPT ![r] = "ready"]
           /\ UNCHANGED <<version, pending, snapshots, settled, completed, events, badSkip, stale, age, expired>>
\* Model the caller's deadline with two ticks; expiry never clears the owner's row.
Tick(r) == /\ stage[r] = "wait" /\ age[r] < 2
           /\ age' = [age EXCEPT ![r] = @ + 1]
           /\ stage' = IF age'[r] = 2 /\ Bug # "deadline"
                        THEN [stage EXCEPT ![r] = "timeout"] ELSE stage
           /\ expired' = IF age'[r] = 2 THEN expired \cup {r} ELSE expired
           /\ UNCHANGED <<version, pending, snapshots, settled, completed, events, badSkip, stale>>
\* A failed finish commit must not emit an event or change durable state.
Abort(r) == /\ stage[r] = "settled" /\ Bug = "ordered"
            /\ events' = [events EXCEPT ![r] = @ + 1]
            /\ UNCHANGED <<version, pending, snapshots, stage, settled, completed, badSkip, stale, age, expired>>
\* Every finite interleaving is explored, including restart and caller timeout.
Next == \E r \in Requests : Read(r) \/ Retry(r) \/ Install(r) \/ Settle(r) \/ Finish(r)
                          \/ Crash(r) \/ Recover(r) \/ Wake(r) \/ Tick(r) \/ Abort(r)
\* No stale observation is allowed to choose a replay for a committed install.
INV_Validated == ~stale
\* A second call cannot finish or skip over an earlier active replay.
INV_NoEarlySkip == ~badSkip
\* Each identity emits at most one terminal result.
INV_Once == \A r \in Requests : events[r] <= 1
\* Only the successful finish commit emits an event.
INV_Ordered == \A r \in Requests : events[r] > 0 => r \in completed
\* No result claims that a replay settled while its task is still running.
INV_Settled == completed \subseteq settled
\* A crashed replay remains recoverable until its terminal result is committed.
INV_Durable == \A r \in Requests : stage[r] = "crashed" => pending = r
\* Reaching the deadline ends the waiting call, not the replay it observed.
INV_Deadline == \A r \in expired : stage[r] # "wait"
=============================================================================
