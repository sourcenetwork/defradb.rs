---------------------- MODULE ReplicatorIncarnation ----------------------
\* Story. Two install calls race to install one peer as a replicator. Each takes
\* an identity from a shared counter, installs it as the peer's incarnation,
\* and runs a replay that sends the peer's existing documents. While a replay
\* runs, someone can delete the peer or reconfigure it (removing one collection
\* rewrites the peer through create_replicator, which gives it a new
\* incarnation), and other writers can rewrite the row under the same
\* incarnation (a status writer, an address refresh). The replay writes two
\* kinds of durable state: retry markers for documents it could not finish,
\* and the peer row's status, which it flips to inactive by reading, deciding
\* and writing in one transaction.
\* Never: a stale allocation failing for good, the incarnation moving back, a
\* replay changing a row it does not own or recreating a deleted one, a
\* replay skipping its flip because another writer rewrote its own install's
\* row first, a replay writing a retry marker for an incarnation other than
\* its own, or a replay whose install is still current giving up on its
\* remaining failed documents because a live push already holds one of them
\* with a newer marker.
\* Rust: crates/db/src/merge/push_docs_replay.rs persist_replay_failures, which flips
\* status through crates/storage/src/stores/peerstore.rs update_replicator_for_incarnation
\* (UpdateReplicatorAttempt: one transaction, the bytes rewritten by
\* crates/p2p/src/replicator.rs ReplicatorInfo::rewrite_status; the status helpers in
\* crates/p2p-adapter/src/replicator_status.rs use the same method), and records markers
\* through record_push_failure_for_incarnation; crates/db/src/merge/push_docs.rs
\* push_existing_docs_with_config_and_allowlist, which registers each document's marker
\* through observe_push_head_for_incarnation. Both marker writes check the incarnation
\* inside peerstore.rs RegisterScopeMarkerAttempt.
\* Allocation happens outside the deciding transaction, so callers can arrive out of order.
EXTENDS Naturals
\* Stale: "retry", "error" or "rollback" decides a stale allocation's fate.
\* ReplayMode: "fenced" flips the status in one transaction fenced by the
\* install's identity (the fix); "create" reads the row, then rewrites it through
\* create_replicator; "cas" reads the row, then writes only if the incarnation
\* and the bytes it read are both unchanged.
\* MarkerMode: "replay" checks the replay's own incarnation with each marker
\* write and stops only when that check fails; "current" writes the marker for
\* whatever incarnation is current; "conflated" also stops when a newer marker
\* already holds the document's scope, as if the install were gone.
CONSTANTS Stale, ReplayMode, MarkerMode
\* Two calls share a monotone allocator and one peer's durable incarnation.
Calls == {1, 2}
\* Two documents every replay fails to finish, so one can be held by a newer marker.
Docs == {1, 2}
\* Finite identities and deadline ticks bound the explored execution.
\* sequence: Peerstore allocator; incarnation: ReplicatorIncarnationKey.
\* ids: ReplicatorInstall.id; phase and age: each caller's local install state.
\* pending: the persisted pending install owner; row: ReplicatorInfo status or absent.
\* marks: every retry marker a replay wrote, as <<the replay's own identity,
\*   the incarnation the marker was written under, the document>>.
\* todo[r]: the failed documents replay r still has to record.
\* gaveUp[r]: replay r stopped recording, having seen its install replaced.
\* newer: documents a live push already holds with a newer marker.
\* version: how many same-incarnation rewrites the row's bytes have had.
\* seen[r]: the version replay r read, in the modes that read before writing.
VARIABLES sequence, incarnation, ids, phase, pending, age, row, marks, todo, gaveUp, newer, version, seen
\* Every transition accounts for the entire state.
vars == <<sequence, incarnation, ids, phase, pending, age, row, marks, todo, gaveUp, newer, version, seen>>
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
        \* No replay has failures to record, and none has given up.
        /\ todo = [r \in Calls |-> {}] /\ gaveUp = [r \in Calls |-> FALSE]
        \* No live push holds a document yet.
        /\ newer = {}
        \* The row's bytes have not been rewritten, and no replay has read them.
        /\ version = 0 /\ seen = [r \in Calls |-> 0]
\* An independent allocation can be delayed before its deciding transaction begins.
Allocate(r) == /\ phase[r] = "allocate" /\ sequence < 3
               \* The shared counter hands out the next identity.
               /\ sequence' = sequence + 1
               \* The caller keeps that identity.
               /\ ids' = [ids EXCEPT ![r] = sequence']
               \* It now decides whether the identity may install.
               /\ phase' = [phase EXCEPT ![r] = "decide"]
               \* Nothing durable changes yet.
               /\ UNCHANGED <<incarnation, pending, age, row, marks, todo, gaveUp, newer, version, seen>>
\* A stale allocation is retried through the existing deadline-bounded business decision.
Reallocate(r) == /\ phase[r] = "decide" /\ pending = 0 /\ ids[r] < incarnation
                 \* The rollback bug never re-decides; it installs instead.
                 /\ Stale # "rollback"
                 \* The error bug gives up for good; the fix allocates again.
                 /\ phase' = [phase EXCEPT ![r] = IF Stale = "error" THEN "failed" ELSE "allocate"]
                 \* Nothing durable changes.
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row, marks, todo, gaveUp, newer, version, seen>>
\* Pending installs block successors; the transaction cannot publish an older identity.
Install(r) == /\ phase[r] = "decide" /\ pending = 0
              \* Only an identity at least as new installs, unless the rollback bug is planted.
              /\ (ids[r] >= incarnation \/ Stale = "rollback")
              \* The identity becomes the peer's incarnation, owns the pending slot, and the row is active.
              /\ incarnation' = ids[r] /\ pending' = r /\ row' = "active"
              \* Its replay now runs.
              /\ phase' = [phase EXCEPT ![r] = "running"]
              \* Every document of its replay fails and must be recorded.
              /\ todo' = [todo EXCEPT ![r] = Docs] /\ gaveUp' = [gaveUp EXCEPT ![r] = FALSE]
              \* The counter, identities, deadlines, markers and row version are untouched.
              /\ UNCHANGED <<sequence, ids, age, marks, newer, version, seen>>
\* Completion releases the peer for the next caller without changing its incarnation.
Finish(r) == /\ phase[r] = "running" /\ pending = r
             \* The call is done and the pending slot is free again.
             /\ phase' = [phase EXCEPT ![r] = "done"] /\ pending' = 0
             \* Nothing else changes.
             /\ UNCHANGED <<sequence, incarnation, ids, age, row, marks, todo, gaveUp, newer, version, seen>>
\* The outer deadline remains authoritative even if repeated allocations become stale.
Tick(r) == /\ phase[r] \in {"allocate", "decide"} /\ age[r] < 2
           \* One more tick of the caller's deadline passes.
           /\ age' = [age EXCEPT ![r] = @ + 1]
           \* At the second tick the caller times out.
           /\ phase' = IF age'[r] = 2 THEN [phase EXCEPT ![r] = "timeout"] ELSE phase
           \* Nothing durable changes.
           /\ UNCHANGED <<sequence, incarnation, ids, pending, row, marks, todo, gaveUp, newer, version, seen>>
\* The "create" and "cas" bugs read the active row in one step, before their
\* status write; the fix has no such read, its flip reads inside its write.
ReplayRead(r) == /\ ReplayMode # "fenced" /\ phase[r] = "running" /\ row = "active"
                 \* The replay is now between its read and its write,
                 /\ phase' = [phase EXCEPT ![r] = "flipping"]
                 \* and remembers which bytes it read.
                 /\ seen' = [seen EXCEPT ![r] = version]
                 \* Reading changes nothing durable.
                 /\ UNCHANGED <<sequence, incarnation, ids, pending, age, row, marks, todo, gaveUp, newer, version>>
\* The replay's status write. The fix (update_replicator_for_incarnation) is one
\* step from "running": in a single transaction it reads the row, checks the
\* install's identity and writes inactive. The bugs write in a later step than
\* their read.
ReplayFlip(r) == /\ phase[r] = IF ReplayMode = "fenced" THEN "running" ELSE "flipping"
                \* The create bug allocates, so it is bounded by the counter's limit.
                /\ (ReplayMode # "create" \/ sequence < 4)
                \* The replay is running again after its write.
                /\ phase' = [phase EXCEPT ![r] = "running"]
                \* Identities, the pending owner, deadlines, markers and bytes versions are untouched.
                /\ UNCHANGED <<ids, pending, age, marks, todo, gaveUp, newer, version, seen>>
                \* Which write the replay makes depends on the mode.
                /\ IF ReplayMode = "create"
                      \* The bug: create_replicator takes a new identity from the counter,
                      THEN /\ sequence' = sequence + 1 /\ incarnation' = sequence'
                           \* and marks the row inactive whatever row is there now.
                           /\ row' = "inactive"
                      \* Otherwise the counter and incarnation stay as they are,
                      ELSE /\ UNCHANGED <<sequence, incarnation>>
                           \* and the row turns inactive only if it is still this install's
                           \* active row; the "cas" bug also requires the bytes it read.
                           /\ row' = IF /\ row = "active" /\ incarnation = ids[r]
                                        /\ (ReplayMode # "cas" \/ version = seen[r])
                                     THEN "inactive" ELSE row
\* A running replay records a retry marker for one failed document d. The
\* peerstore transaction has three outcomes. The install is replaced (the row
\* is gone, or the fix sees another incarnation): the replay gives up. A live
\* push already holds d with a newer marker: d is covered, and the replay moves
\* on (the "conflated" bug gives up here too). Otherwise the marker is written,
\* under the incarnation current at the write (the "current" bug skips the check).
ReplayMark(r, d) == /\ phase[r] = "running" /\ ~gaveUp[r] /\ d \in todo[r]
                    \* Whether the transaction finds the replay's own install.
                    /\ LET replaced == row = "absent" \/ (MarkerMode # "current" /\ incarnation # ids[r])
                           \* Whether a newer marker already holds d's scope.
                           held == d \in newer
                       \* A replaced install stops the replay; the conflated bug also stops on a held d.
                       IN /\ gaveUp' = [gaveUp EXCEPT ![r] = replaced \/ (held /\ MarkerMode = "conflated")]
                          \* Otherwise d is done: covered by the newer marker, or written now.
                          /\ todo' = IF replaced THEN todo ELSE [todo EXCEPT ![r] = @ \ {d}]
                          \* A marker is written only when nothing newer holds d.
                          /\ marks' = IF replaced \/ held THEN marks
                                       ELSE marks \cup {<<ids[r], incarnation, d>>}
                    \* Nothing else changes.
                    /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, row, newer, version, seen>>
\* A live push for the current install commits a newer marker for document d
\* between a replay's generation allocation and its transaction.
LivePush(d) == /\ row = "active" /\ d \notin newer
               \* d's scope is now held by the newer marker.
               /\ newer' = newer \cup {d}
               \* Nothing else changes.
               /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, row, marks, todo, gaveUp, version, seen>>
\* Another writer rewrites an existing row under the same incarnation (an
\* address refresh, another status writer): its bytes change, its status and
\* identity do not.
Rewrite == /\ row # "absent" /\ version < 2
           \* The row's bytes are new.
           /\ version' = version + 1
           \* Nothing else changes.
           /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, row, marks, todo, gaveUp, newer, seen>>
\* Removing a peer deletes the row but retains the incarnation key.
Delete == /\ row # "absent" /\ row' = "absent"
          \* Everything else stays.
          /\ UNCHANGED <<sequence, incarnation, ids, phase, pending, age, marks, todo, gaveUp, newer, version, seen>>
\* A separate create can replace the row while a replay is running: removing
\* one of the peer's collections rewrites it through create_replicator.
Replace == /\ sequence < 4 /\ sequence' = sequence + 1
           \* The rewrite takes a new incarnation and leaves the row active.
           /\ incarnation' = sequence' /\ row' = "active"
           \* The calls and their markers are untouched.
           /\ UNCHANGED <<ids, phase, pending, age, marks, todo, gaveUp, newer, version, seen>>
\* Interleavings include replay, deletion and an independent reconfiguration.
Next == \/ \E r \in Calls : \/ Allocate(r) \/ Reallocate(r) \/ Install(r) \/ Finish(r)
                            \/ Tick(r) \/ ReplayRead(r) \/ ReplayFlip(r)
                            \/ \E d \in Docs : ReplayMark(r, d)
        \* Deletion, reconfiguration, same-incarnation rewrites and live pushes can happen at any time.
        \/ Delete \/ Replace \/ Rewrite \/ \E d \in Docs : LivePush(d)
\* The whole behaviour: start from Init and take any Next step.
Spec == Init /\ [][Next]_vars
\* An unchanged install's deferred status update keeps the plan identity.
ACT_DeferredReplaySettlesClean == [][(\E r \in Calls : ReplayFlip(r) /\ row = "active" /\ incarnation = ids[r]) => incarnation' = incarnation]_vars
\* A replay returning after deletion cannot recreate the peer row.
ACT_NoResurrect == [][(\E r \in Calls : ReplayFlip(r) /\ row = "absent") => row' = "absent"]_vars
\* A replay for an older install cannot alter a replacement's status.
ACT_NoStaleFlip == [][(\E r \in Calls : ReplayFlip(r) /\ incarnation # ids[r]) => row' = row]_vars
\* A replay whose own install still holds the active row always flips it to
\* inactive, however often other writers rewrote that row under the same incarnation.
ACT_FlipNotSkipped == [][(\E r \in Calls : ReplayFlip(r) /\ row = "active" /\ incarnation = ids[r]) => row' = "inactive"]_vars
\* Every durable incarnation change moves forward.
ACT_Monotone == [][incarnation' >= incarnation]_vars
\* Allocator ordering cannot become a permanent storage failure.
INV_Redecide == \A r \in Calls : phase[r] # "failed"
\* Every retry marker a replay wrote belongs to that replay's own incarnation.
INV_MarkerOwner == \A m \in marks : m[1] = m[2]
\* A replay gives up on its failed documents only once its install is replaced;
\* while the install is current, every failed document gets a marker or is
\* already held by a newer one.
INV_FailuresRecorded == \A r \in Calls : gaveUp[r] => (row = "absent" \/ incarnation # ids[r])
=============================================================================
