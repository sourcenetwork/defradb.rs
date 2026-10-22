---- MODULE TwoStoreCounter ----
\* Counter materialization under concurrent local-write vs remote-merge, abstracting
\* the pre-#1021 merge path (reconcile acc := blob -> merge(+delta) -> blob := acc),
\* now crates/db/src/merge/merge_handler/field_ops.rs (apply_field_op_once)
\* and the local-write path in the `db` crate's doc_mutator. Anchors in the
\* TwoStoreCounter_DESIGN.md (and project memory same_doc_merge_storm_undercount).
\*
\* THE BUG (found 2026-06-15, storm probe): a PCounter's value lives in TWO stores —
\* the materialized document `blob` (advanced by LOCAL writes AND by merges) and the
\* CRDT accumulation store `acc` (advanced ONLY by merges). Each merge RECONCILES
\* `acc := blob` then applies its delta then writes `blob := acc`. That is a
\* read-modify-write of `blob`. A concurrent LOCAL increment touches only `blob`,
\* on a DIFFERENT key, so the storage layer's transactional conflict detection
\* never fires between the two — the merge commits a stale-reconciled value and
\* CLOBBERS the local increment. The DAG converges (every delta block is merged-
\* marked) yet the materialized sum is short by the clobbered increments.
\*
\* HOW GO AVOIDS IT (parity): Go keeps the value in ONE key (counter.go
\* incrementValue RMW on key.WithValueFlag()); BOTH local writes and merges RMW
\* that same key, so a concurrent local-write and merge conflict at commit and the
\* loser retries against the fresh value. No two-store split, no clobber.
\*
\* One knob selects the buggy split vs. the Go-parity fix:
\*   StoreMode = "Split"   - reconcile-from-blob with NO cross-store conflict; the
\*                           merge commits its stale snapshot+delta unconditionally,
\*                           clobbering an interleaved local write          [RED]
\*           = "Unified"   - the merge's RMW is conflict-checked: it commits only if
\*                           `blob` is unchanged since reconcile, else it re-reconciles
\*                           against the fresh value and retries (= Go's shared-key
\*                           txn-conflict / a per-doc lock spanning both paths) [GREEN]
\*
\* ORACLE: `doneCount` counts every increment whose effect has been committed
\* (a local apply, or a merge commit). It is independent of the stores. The headline
\* safety property is that the materialized value equals that count — no increment is
\* lost. `acc` is carried to mirror the code but the user-visible value is `blob`.
\*
\* The GREEN "Unified" mode abstracts a conflict-checked RMW. The current code uses
\* engine OCC and counter merge operands, plus init-if-absent reconciliation
\* (PCounter migrate-via-max). This model checks no-loss at the abstract state level;
\* it does not prove the implementation's OCC protocol.
\*
\* THE DEFERRAL AXIS (PR #2192): a counter block can reach this node on its own
\* (a standalone field block) BEFORE its document has a value it can add to: the
\* document is not materialized yet, or its value is on another schema version.
\* Only the composite block that links the counter block writes that value. Who
\* acts: the standalone merge and the composite merge of the same block. What can
\* happen: either arrives first. What must never happen: the standalone merge
\* claims the block's applied key without adding its increment, so the composite
\* finds the key, skips the increment, and the increment is lost for good.
\*   Defer = "On"   - the standalone merge with nothing to add to is a terminal
\*                    skip: no applied key, no value, no "merged" mark. The
\*                    composite applies the increment once                  [GREEN]
\*           = "Off"  - the standalone merge claims the applied key, drops the
\*                    value it computed, and reports the block merged       [RED]
\* Code: crates/db/src/merge/merge_handler/field_ops.rs `apply_field_op_once`
\* (its `standalone` deferral), called standalone by `merge_field_in_txn`
\* (field.rs) from `process_block_in_txn` (batch.rs), and by the composite merge
\* through `merge_field_delta_in_txn` from composite_fields.rs.
EXTENDS Naturals, FiniteSets

CONSTANTS
  LocalOps,   \* finite set of local-write increment ids (db crate doc_mutator path)
  RemoteOps,  \* finite set of remote-merge increment ids (db-merge counter merge path)
  StoreMode,  \* "Split" | "Unified"   — the two-store split (lost-update axis)
  Dedup,      \* "On" | "Off"          — the per-block merged-set guard (double-apply axis)
  LateOps,    \* finite set of remote increments that may arrive before the document has a value
  Defer       \* "On" | "Off"          — the standalone deferral (lost-deferral axis)

\* No increment id belongs to two kinds of increment.
ASSUME LocalOps \cap RemoteOps = {}
\* A late increment is neither a local one nor an ordinary remote one.
ASSUME LateOps \cap (LocalOps \cup RemoteOps) = {}
\* The store knob is one of its two settings.
ASSUME StoreMode \in {"Split", "Unified"}
\* The dedup knob is one of its two settings.
ASSUME Dedup \in {"On", "Off"}
\* The deferral knob is one of its two settings.
ASSUME Defer \in {"On", "Off"}

\* Every increment the model tracks, of all three kinds.
Ops == LocalOps \cup RemoteOps \cup LateOps

VARIABLES
  blob,       \* Nat: materialized document value (queries read this; local writes RMW it)
  acc,        \* Nat: counter accumulation store (value_key); merges RMW it
  pc,         \* [Ops -> phase] per-increment control state
  snap,       \* [RemoteOps -> Nat] value snapshotted by a merge's reconcile step
  doneCount,  \* Nat: ORACLE — increments committed so far (must equal blob)
  materialized, \* BOOLEAN: the document has a value an increment can be added to
  applied     \* [LateOps -> BOOLEAN]: the late increment's applied key is claimed

\* Every variable, for stuttering steps.
vars == <<blob, acc, pc, snap, doneCount, materialized, applied>>

\* local ops: "todo" -> "done"; remote ops: "todo" -> "recon" -> "done".
Phases == {"todo", "recon", "done"}

TypeOK ==
  /\ blob \in Nat
  /\ acc \in Nat
  /\ pc \in [Ops -> Phases]
  /\ snap \in [RemoteOps -> Nat]
  /\ doneCount \in Nat
  /\ materialized \in BOOLEAN
  /\ applied \in [LateOps -> BOOLEAN]

\* The document starts with no committed increments. It already has a value
\* unless some increment can arrive before it does.
Init ==
  /\ blob = 0
  /\ acc = 0
  /\ pc = [o \in Ops |-> "todo"]
  /\ snap = [o \in RemoteOps |-> 0]
  /\ doneCount = 0
  /\ materialized = (LateOps = {})
  /\ applied = [o \in LateOps |-> FALSE]

\* ---- Local write: blob += 1, NOT touching acc (the two-store gap). No lock. ----------
\* A local write needs a document to write to.
LocalApply(o) ==
  /\ o \in LocalOps
  /\ pc[o] = "todo"
  /\ materialized
  /\ blob' = blob + 1
  /\ doneCount' = doneCount + 1
  /\ pc' = [pc EXCEPT ![o] = "done"]
  /\ UNCHANGED <<acc, snap, materialized, applied>>

\* ---- Merge step 1: reconcile the accumulation store up to the current blob. ----------
\* Pre-#1021 split only: field_ops.rs keeps no second store; its operand reads the part at commit.
\* An ordinary remote increment merges into a document that has a value.
MergeReconcile(o) ==
  /\ o \in RemoteOps
  /\ pc[o] = "todo"
  /\ materialized
  /\ snap' = [snap EXCEPT ![o] = blob]
  /\ acc' = blob
  /\ pc' = [pc EXCEPT ![o] = "recon"]
  /\ UNCHANGED <<blob, doneCount, materialized, applied>>

\* ---- Merge step 2: apply the delta and write the value back. -------------------------
\* field_ops.rs: `apply_field_op_once` merges the operation onto the counter's part.
MergeCommit(o) ==
  /\ o \in RemoteOps
  /\ pc[o] = "recon"
  /\ IF StoreMode = "Split"
       \* RED: commit the stale reconciled snapshot + delta unconditionally. If a
       \* local write bumped `blob` since reconcile, this OVERWRITES it (lost update).
       THEN /\ acc' = snap[o] + 1
            /\ blob' = snap[o] + 1
            /\ doneCount' = doneCount + 1
            /\ pc' = [pc EXCEPT ![o] = "done"]
            /\ UNCHANGED <<snap, materialized, applied>>
       \* GREEN: conflict-checked RMW. Commit only if `blob` is unchanged since
       \* reconcile; otherwise re-reconcile against the fresh value and retry. This is
       \* Go's shared-key txn-conflict (or a per-doc lock spanning local+merge).
       ELSE IF blob = snap[o]
              THEN /\ acc' = blob + 1
                   /\ blob' = blob + 1
                   /\ doneCount' = doneCount + 1
                   /\ pc' = [pc EXCEPT ![o] = "done"]
                   /\ UNCHANGED <<snap, materialized, applied>>
              ELSE /\ snap' = [snap EXCEPT ![o] = blob]
                   /\ acc' = blob
                   /\ UNCHANGED <<blob, doneCount, pc, materialized, applied>>

\* ---- Re-delivery (#4935): a remote delta is DELIVERED A SECOND TIME. ----------------
\* A counter field block delivered as its own PushLog head (or re-delivered via two
\* channels) is handed to the merge handler a SECOND time. Go's `coreblock.ProcessBlock`
\* applies the delta unconditionally; idempotency rests entirely on the blockstore
\* merged-set / is_merged guard. This action models the re-delivery itself; the guard is
\* modeled INLINE as a no-op transition (not a disabled action) so BOTH settings exercise
\* the re-delivery path:
\*   Dedup="On"  (GREEN): is_merged(cid) finds the block already in the merged-set and
\*                        SUPPRESSES the re-apply -> UNCHANGED. This no-op is what makes
\*                        GREEN non-vacuous on the double-apply axis (the guard actively
\*                        fires, rather than the re-delivery being structurally absent).
\*   Dedup="Off" (RED):   no guard -> the re-merge re-applies the +1 (blob/acc climb)
\*                        WITHOUT a new increment (doneCount unchanged) -> blob exceeds
\*                        the committed count.
\* The counter's idempotency axis, orthogonal to the two-store split above; the Lean twin
\* is `CounterReconcile.counter_not_idempotent`.
MergeRedeliver(o) ==
  /\ o \in RemoteOps
  /\ pc[o] = "done"      \* its delta was already applied once; now re-delivered
  /\ IF Dedup = "On"
       \* GREEN: the merged-set guard suppresses the re-apply (a no-op). The re-delivery
       \* is exercised; dedup actively prevents the double-count.
       THEN UNCHANGED <<blob, acc>>
       \* RED: no guard -> the delta is applied a second time.
       ELSE /\ blob' = blob + 1
            /\ acc'  = acc + 1
  /\ UNCHANGED <<pc, snap, doneCount, materialized, applied>>

\* ---- Late increment, standalone: its field block arrives on its own. -----------------
\* field_ops.rs `apply_field_op_once` with `standalone` set. With a value to add to,
\* it adds the increment once, guarded by the applied key. Without one:
\*   Defer="On"  (GREEN): a terminal skip that changes nothing — a no-op transition, so
\*                        GREEN actively exercises the early arrival.
\*   Defer="Off" (RED):   the old code claimed the applied key, dropped the value it
\*                        computed and reported the block merged: counted, never added.
StandaloneArrive(o) ==
  \* o is a late increment that no merge has finished yet.
  /\ o \in LateOps
  /\ pc[o] = "todo"
  /\ IF materialized
       \* A value exists: the increment is an operand on it, applied once.
       THEN /\ blob' = blob + 1
            /\ applied' = [applied EXCEPT ![o] = TRUE]
            /\ doneCount' = doneCount + 1
            /\ pc' = [pc EXCEPT ![o] = "done"]
       ELSE IF Defer = "On"
              \* GREEN: nothing to add to, so nothing changes; the composite will apply it.
              THEN UNCHANGED <<blob, applied, doneCount, pc>>
              \* RED: the applied key is claimed and the block counted, but the value is lost.
              ELSE /\ applied' = [applied EXCEPT ![o] = TRUE]
                   /\ doneCount' = doneCount + 1
                   /\ pc' = [pc EXCEPT ![o] = "done"]
                   /\ UNCHANGED blob
  \* Neither the accumulation store nor the snapshots, nor whether a value exists, change.
  /\ UNCHANGED <<acc, snap, materialized>>

\* ---- Late increment, through its composite: the block that writes the value. --------
\* composite_fields.rs merges every linked field block through `merge_field_delta_in_txn`
\* (not standalone): the composite writes the document value, and adds the increment
\* unless its applied key says it was already added.
\* The composite may also arrive after a merge already finished the increment
\* (a redelivery, or after the standalone block applied it): its applied-key check
\* then keeps it from adding the increment twice.
CompositeArrive(o) ==
  \* o is a late increment, finished or not.
  /\ o \in LateOps
  \* After the composite the document has a value.
  /\ materialized' = TRUE
  \* The increment is added only if its applied key is unclaimed.
  /\ blob' = IF applied[o] THEN blob ELSE blob + 1
  \* Either way the composite leaves the key claimed.
  /\ applied' = [applied EXCEPT ![o] = TRUE]
  \* The oracle counts the increment the first time a merge reports it merged.
  /\ doneCount' = IF pc[o] = "todo" THEN doneCount + 1 ELSE doneCount
  /\ pc' = [pc EXCEPT ![o] = "done"]
  \* The accumulation store and snapshots are untouched.
  /\ UNCHANGED <<acc, snap>>

\* Any increment takes any step it is allowed to take.
Next == \E o \in Ops : LocalApply(o) \/ MergeReconcile(o) \/ MergeCommit(o) \/ MergeRedeliver(o)
                       \/ StandaloneArrive(o) \/ CompositeArrive(o)

Spec == Init /\ [][Next]_vars

\* ===================================================================================
\* SAFETY: the materialized value is EXACT — it equals the number of committed
\* increments, neither short nor over. Two independent hazards can break it:
\*   * lost update  (blob < doneCount): RED under StoreMode="Split" — a merge's
\*     reconcile-from-blob clobbers an interleaved local increment.
\*   * double-apply (blob > doneCount): RED under Dedup="Off" — a re-delivered delta
\*     is merged twice (the #4935 field-block-as-head re-merge).
\*   * lost deferral (blob < doneCount): RED under Defer="Off" — a standalone block
\*     that arrived before its document's value claimed the applied key without
\*     adding, so its composite skipped the increment.
\* GREEN (StoreMode="Unified", Dedup="On") holds equality in every reachable state.
\* ===================================================================================
INV_NoLoss == blob >= doneCount        \* no increment dropped (Split and Defer=Off break this)
INV_NoDoubleApply == blob <= doneCount \* no increment counted twice (Dedup=Off breaks this)
INV_Exact == blob = doneCount          \* both directions (the headline)

INV_TypeOK == TypeOK
====
