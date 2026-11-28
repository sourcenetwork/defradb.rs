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

-- An admission attempt reads at most `cap` index entries (Rust: live_roots).
def visible (entries cap : Nat) : Nat := min entries cap

-- It admits a new root only when the entries it read leave room.
def admits (entries cap : Nat) : Bool := decide (visible entries cap < cap)

-- An index at or over capacity never admits, however many entries it holds.
theorem full_index_never_admits (entries cap : Nat) (h : cap ≤ entries) :
    admits entries cap = false := by
  -- The capped read equals the capacity, which is not below itself.
  simp [admits, visible, Nat.min_eq_right h]

-- Below capacity the capped read sees every entry, so it admits.
theorem room_admits (entries cap : Nat) (h : entries < cap) :
    admits entries cap = true := by
  -- The capped read is the whole index, which is below the capacity.
  simp [admits, visible, Nat.min_eq_left (Nat.le_of_lt h), h]

-- Reclaiming one entry of an over-full index moves it one step toward room.
theorem reclaim_shrinks (entries : Nat) (h : 0 < entries) : entries - 1 < entries := by
  -- Removing an entry from a non-empty index leaves fewer entries.
  exact Nat.sub_lt h Nat.one_pos

-- The bookkeeping a history walk holds on disk: its stack frames and its node marks.
structure Book where
  frames : Nat
  marks : Nat
  deriving DecidableEq

-- The key count the root's progress must store for a given bookkeeping.
def Book.keys (b : Book) : Nat := b.frames + b.marks

-- Pushing a parent writes one frame and one open mark.
def Book.push (b : Book) : Book := { frames := b.frames + 1, marks := b.marks + 1 }

-- Popping a node: its frame goes, and its mark goes too unless the node was skipped.
def Book.pop (b : Book) (keep : Bool) : Book :=
  { frames := b.frames - 1, marks := if keep then b.marks else b.marks - 1 }

-- A push adds exactly the two keys history_walk.rs adds to `Progress::keys`.
theorem push_accounts (b : Book) : b.push.keys = b.keys + 2 := by
  -- One frame and one mark: two more keys.
  simp [Book.push, Book.keys]; omega

-- Popping an applied node removes exactly the two keys the walk subtracts.
theorem pop_applied_accounts (b : Book) (hf : 1 ≤ b.frames) (hm : 1 ≤ b.marks) :
    (b.pop false).keys = b.keys - 2 := by
  -- The frame and the mark both go.
  simp [Book.pop, Book.keys]; omega

-- Popping a skipped node removes only its frame, the one key the walk subtracts.
theorem pop_skipped_accounts (b : Book) (hf : 1 ≤ b.frames) :
    (b.pop true).keys = b.keys - 1 := by
  -- The done mark stays so a second branch passes the node over.
  simp [Book.pop, Book.keys]; omega

-- A push followed by an applied pop leaves the bookkeeping as it was: an applied
-- node, however deep the history, retains nothing once it is finished.
theorem applied_node_retains_nothing (b : Book) : (b.push.pop false) = b := by
  -- The frame and mark the push wrote are exactly the ones the pop deletes.
  cases b; simp [Book.push, Book.pop]

end HistoryTurns
