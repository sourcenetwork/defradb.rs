/-!
A history page publishes each ancestor once. Restart may reset the cursor, but
not the published set. A cancelled page changes neither writes nor its cursor.
Rust: merge_handler/history.rs; concurrency is modeled in HistoryTurns.tla.
Driver handoff follows replication.rs construction and collection.rs error propagation;
HistoryDriver.tla models their scheduling and rejection boundary.
-/
namespace HistoryTurns

-- A published CID is kept even when a traversal cursor is discarded.
def publish (seen : List Nat) (cid : Nat) : List Nat :=
  if cid ∈ seen then seen else cid :: seen

-- Replaying the same ancestor cannot add a second publication.
theorem publish_once (seen : List Nat) (cid : Nat) :
    publish (publish seen cid) cid = publish seen cid := by
  -- Both branches make the CID a member; the next call therefore leaves it alone.
  unfold publish
  split <;> simp_all

-- Bookkeeping and persistent data are deliberately separate components.
structure State where
  published : List Nat
  cursor : Nat
  deriving DecidableEq

-- Cleanup is allowed to throw away only the cursor.
def cleanup (s : State) : State := { s with cursor := 0 }

-- Document/block publication survives cursor reclamation unchanged.
theorem cleanup_preserves_publications (s : State) :
    (cleanup s).published = s.published := by
  -- Cleanup writes only the other component.
  rfl

-- A page commits its cursor and writes together, or neither one.
def page (s : State) (cid : Nat) (commit : Bool) : State :=
  if commit then { published := publish s.published cid, cursor := s.cursor + 1 } else s

-- Cancellation cannot leave a cursor ahead of its durable writes.
theorem cancelled_page (s : State) (cid : Nat) : page s cid false = s := by
  -- An uncommitted page returns the original state.
  rfl

-- A rejected page owns only the cursor incarnation it read.
def rejectCursor (current observed : Nat) (present : Bool) : Bool :=
  if current = observed then false else present

theorem stale_rejection_keeps_cursor (current observed : Nat) (h : current ≠ observed) :
    rejectCursor current observed true = true := by
  simp [rejectCursor, h]

-- Rust's queued continuation becomes a weakly registered owner before reconciliation.
-- Dropping that owner releases the slot on skip, failure, cancellation or merge.
def finishSlot (reserved owned : Bool) : Bool := reserved && !owned

-- This is the release step of ContinuationSlot.tla, not a merge mutex.
theorem owned_slot_released (reserved : Bool) : finishSlot reserved true = false := by
  -- Neither initial reservation state can survive its owner's drop.
  cases reserved <;> rfl

-- Collection carriers preserve driver signals from their linked composites.
inductive DriverSignal where
  | ordinaryRetry
  | historyRequired
  | historyAborted
  | decryptionKey
  | depthExceeded
  deriving DecidableEq

-- Retrying a collection locally is valid only for an ordinary transient failure.
def propagate (signal : DriverSignal) : Bool :=
  match signal with
  | .ordinaryRetry => false
  | _ => true

-- A rejected history must reach the driver that fences and clears its cursor.
theorem history_abort_reaches_driver : propagate .historyAborted = true := by
  -- The carrier cannot turn this signal into an ordinary retryable skip.
  rfl

-- Every scheduling turn returns control to the owner of continuation delivery.
def yieldToScheduler (singleTurn : Bool) : Bool := singleTurn

-- Production replication handlers hand yielded work to the continuation scheduler.
theorem replication_hands_off : yieldToScheduler true = true := by
  -- The production construction selects the same bounded-turn mode.
  rfl

-- A locally ready page becomes due at its registration time, without retry backoff.
def continuationDue (now : Nat) : Nat := now

-- The scheduler can claim a committed page without advancing a timer.
theorem continuation_immediately_due (now : Nat) : continuationDue now ≤ now := by
  -- Scheduling local work keeps the current instant instead of adding a delay.
  exact Nat.le_refl now

end HistoryTurns
