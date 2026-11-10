---------------------- MODULE ReplicatorIncarnation ----------------------
\* Story. Two install calls race to install one peer as a replicator. Each takes
\* an identity from a shared counter, installs it as the peer's incarnation,
\* and runs a replay that sends the peer's existing documents. While a replay
\* runs, someone can delete the peer or reconfigure it (removing one collection
\* rewrites the peer through create_replicator, which gives it a new
\* incarnation). The replay writes two kinds of durable state: retry markers
\* for documents it could not finish, and the peer row's status.
\* Never: a stale allocation failing for good, the incarnation moving back, a
\* replay changing a row it does not own or recreating a deleted one, or a
\* replay writing a retry marker for an incarnation other than its own.
\* Rust: crates/db/src/merge/push_docs_replay.rs persist_replay_failures, which updates
\* status through crates/storage/src/stores/peerstore.rs set_replicator_if_incarnation,
\* and records markers through record_push_failure_for_incarnation; crates/db/src/merge/
\* push_docs.rs push_existing_docs_with_config_and_allowlist, which registers each
\* document's marker through observe_push_head_for_incarnation. Both marker writes
\* check the incarnation inside peerstore.rs RegisterScopeMarkerAttempt.
\* Allocation happens outside the deciding transaction, so callers can arrive out of order.
EXTENDS Naturals
\* Stale: "retry", "error" or "rollback" decides a stale allocation's fate.
\* ReplayMode: "fenced" guards the status write by the install's identity;
\* "create" rewrites the row through create_replicator instead.
\* MarkerMode: "replay" checks the replay's own incarnation with each marker
\* write; "current" writes the marker for whatever incarnation is current.
CONSTANTS Stale, ReplayMode, MarkerMode
\* Two calls share a monotone allocator and one peer's durable incarnation.
Calls == {1, 2}
\* Finite identities and deadline ticks bound the explored execution.
\* sequence: Peerstore allocator; incarnation: ReplicatorIncarnationKey.
\* ids: ReplicatorInstall.id; phase and age: each caller's local install state.
\* pending: the persisted pending install owner; row: ReplicatorInfo status or absent.
\* marks: every retry marker a replay wrote, as <<the replay's own identity,
\*   the incarnation the marker was written under>>.
VARIABLES sequence, incarnation, ids, phase, pending, age, row, marks
\* Every transition accounts for the entire state.
vars == <<sequence, incarnation, ids, phase, pending, age, row, marks>>
\* Both calls begin without an allocation or a pending replay.
Init == /\ sequence = 0 /\ incarnation = 0 /\ pending = 0 /\ row = "absent"
        \* Neither call holds an identity yet.
        /\ ids = [r \in Calls |-> 0]
        \* Both calls start by allocating.
        /\ phase = [r \in Calls |-> "allocate"]
        \* Neither call has spent any of its deadline.
        /\ age = [r \in Calls |-> 0]
        \* No replay has written a marker.
        /\ marks = {}
\* An independent allocation can be delayed before its deciding transaction begins.
Allocate(r) == /\ phase[r] = "allocate" /\ sequence < 3
               \* The shared counter hands out the next identity.
               /\ sequence' = sequence + 1
               \* The caller keeps that identity.
               /\ ids' = [ids EXCEPT ![r] = sequence']
               \* It now decides whether the identity may install.
               /\ phase' = [phase EXCEPT ![r] = "decide"]
               \* Nothing durable changes yet.
               /\ UNCHANGED <<incarnation, pending, age, row, marks>>
\* A stale allocation is retried through the existing deadline-bounded business decision.
Reallocate(r) == /\ phase[r] = "decide" /\ pending = 0 /\ ids[r] < incarnation
                 \* The rollback bug never re-decides; it installs instead.
                 /\ Stale # "rollback"
                 \* The error bug gives up for good; the fix allocates again.
                 /\ phase' = [phase EXCEPT ![r] = IF Stale = "error" THEN "failed" ELSE "allocate"]
                 \* Nothing durable changes.
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row, marks>>
\* Pending installs block successors; the transaction cannot publish an older identity.
Install(r) == /\ phase[r] = "decide" /\ pending = 0
              \* Only an identity at least as new installs, unless the rollback bug is planted.
              /\ (ids[r] >= incarnation \/ Stale = "rollback")
              \* The identity becomes the peer's incarnation, owns the pending slot, and the row is active.
              /\ incarnation' = ids[r] /\ pending' = r /\ row' = "active"
              \* Its replay now runs.
              /\ phase' = [phase EXCEPT ![r] = "running"]
              \* The counter, identities, deadlines and markers are untouched.
              /\ UNCHANGED <<sequence, ids, age, marks>>
\* Completion releases the peer for the next caller without changing its incarnation.
Finish(r) == /\ phase[r] = "running" /\ pending = r
             \* The call is done and the pending slot is free again.
             /\ phase' = [phase EXCEPT ![r] = "done"] /\ pending' = 0
             \* Nothing else changes.
             /\ UNCHANGED <<sequence, incarnation, ids, age, row, marks>>
\* The outer deadline remains authoritative even if repeated allocations become stale.
Tick(r) == /\ phase[r] \in {"allocate", "decide"} /\ age[r] < 2
           \* One more tick of the caller's deadline passes.
           /\ age' = [age EXCEPT ![r] = @ + 1]
           \* At the second tick the caller times out.
           /\ phase' = IF age'[r] = 2 THEN [phase EXCEPT ![r] = "timeout"] ELSE phase
           \* Nothing durable changes.
           /\ UNCHANGED <<sequence, incarnation, ids, pending, row, marks>>
\* A failed replay reads its active row before attempting the status update.
ReplayRead(r) == /\ phase[r] = "running" /\ row = "active"
                 \* The replay is now between its read and its write.
                 /\ phase' = [phase EXCEPT ![r] = "flipping"]
                 \* Reading changes nothing durable.
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row, marks>>
\* The old create allocates a new identity; the guarded update uses the plan id.
ReplayFlip(r) == /\ phase[r] = "flipping"
                \* The create bug allocates, so it is bounded by the counter's limit.
                /\ (ReplayMode # "create" \/ sequence < 4)
                \* The replay goes back to running after its write.
                /\ phase' = [phase EXCEPT ![r] = "running"]
                \* Identities, the pending owner, deadlines and markers are untouched.
                /\ UNCHANGED <<ids, pending, age, marks>>
                \* Which write the replay makes depends on the mode.
                /\ IF ReplayMode = "create"
                      \* The bug: create_replicator takes a new identity from the counter,
                      THEN /\ sequence' = sequence + 1 /\ incarnation' = sequence'
                           \* and marks the row inactive whatever row is there now.
                           /\ row' = "inactive"
                      \* The fix: the counter and incarnation stay as they are,
                      ELSE /\ UNCHANGED <<sequence, incarnation>>
                           \* and the row turns inactive only if it is still this install's active row.
                           /\ row' = IF row = "active" /\ incarnation = ids[r]
                                      THEN "inactive" ELSE row
\* A running replay records a retry marker for a document it did not finish.
\* The peerstore transaction refuses the write when the peer row is gone. The
\* fix also refuses it unless the incarnation is still the replay's own; the
\* "current" bug writes the marker for whichever install is current.
ReplayMark(r) == /\ phase[r] = "running"
                 \* The transaction finds the peer row.
                 /\ row # "absent"
                 \* The fixed write needs the replay's own incarnation still installed.
                 /\ (MarkerMode = "current" \/ incarnation = ids[r])
                 \* The marker is written under the incarnation current at the write.
                 /\ marks' = marks \cup {<<ids[r], incarnation>>}
                 \* Nothing else changes.
                 /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, row>>
\* Removing a peer deletes the row but retains the incarnation key.
Delete == /\ row # "absent" /\ row' = "absent"
          \* Everything else stays.
          /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, marks>>
\* A separate create can replace the row while a replay is running: removing
\* one of the peer's collections rewrites it through create_replicator.
Replace == /\ sequence < 4 /\ sequence' = sequence + 1
           \* The rewrite takes a new incarnation and leaves the row active.
           /\ incarnation' = sequence' /\ row' = "active"
           \* The calls and their markers are untouched.
           /\ UNCHANGED <<ids, phase, pending, age, marks>>
\* Interleavings include replay, deletion and an independent reconfiguration.
Next == \/ \E r \in Calls : \/ Allocate(r) \/ Reallocate(r) \/ Install(r) \/ Finish(r)
                            \/ Tick(r) \/ ReplayRead(r) \/ ReplayFlip(r) \/ ReplayMark(r)
        \* Deletion and reconfiguration can happen at any time.
        \/ Delete \/ Replace
\* The whole behaviour: start from Init and take any Next step.
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
\* Every retry marker a replay wrote belongs to that replay's own incarnation.
INV_MarkerOwner == \A m \in marks : m[1] = m[2]
=============================================================================
