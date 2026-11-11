/-!
# RequestDrain: the request and worker lifetimes

A signed query task holds a request-scope permit while its worker executes.
Dropping the caller's completion does not drop the task. Once started, worker
pool work runs on even if its waiter is dropped; aborting the task on a
separate deadline would therefore let the drain finish before the write.
An owning transaction guard must likewise move to an edge-admitted commit task:
otherwise cancellation of its caller abandons the transaction under the task.

Rust anchors: `execute_with_signing_context` in
`crates/defra-node/src/signed_query/mod.rs`; `NodeTransaction::commit` in
`crates/defra-node/src/transaction.rs`; `run_on` in
`crates/workers/src/pool/detached.rs`; and `Scope::drain` in
`crates/edge/src/handle.rs`. Interleavings are checked by
`SignedRequestDrain.tla` and `GuardedCommit.tla`.
-/

namespace RequestDrain

-- The edge task's permit and whether its worker has started and still runs.
structure Request where
  permit : Bool
  worker : Bool
  deriving DecidableEq

-- The worker starts under the task that was already admitted.
def start : Request := ⟨true, true⟩

-- The worker finishes and then its task returns, releasing admission.
def finish : Request := ⟨false, false⟩

-- Dropping the caller's waiter leaves the detached task and worker alone.
def dropWaiter (request : Request) : Request := request

-- The old task deadline ended the edge task without stopping its worker.
def expireTask (request : Request) : Request := ⟨false, request.worker⟩

-- A running worker always retains its admitting task's permit.
def keepsPermit (request : Request) : Prop :=
  request.worker = true → request.permit = true

-- The admission of a started worker satisfies the contract.
theorem start_keeps_permit : keepsPermit start := by
  simp [keepsPermit, start]

-- Finishing the worker and task preserves the contract.
theorem finish_keeps_permit : keepsPermit finish := by
  simp [keepsPermit, finish]

-- Dropping the caller's waiter preserves the contract.
theorem dropped_waiter_keeps_permit (request : Request)
    (held : keepsPermit request) : keepsPermit (dropWaiter request) := by
  exact held

-- The former task deadline can lose the permit while its worker runs.
theorem expired_task_loses_permit : ¬ keepsPermit (expireTask start) := by
  simp [keepsPermit, expireTask, start]

-- Who holds an owning transaction guard after a commit starts.
inductive Owner where
  | caller
  | task
  deriving DecidableEq

-- A cancelled waiter abandons the transaction only if it kept the guard.
def abandonedOnCancel : Owner → Bool
  | .caller => true
  | .task => false

-- A task-owned guard survives cancellation of the caller's waiter.
theorem task_guard_survives_cancellation : abandonedOnCancel .task = false := by
  rfl

-- Keeping the guard at the caller reproduces the unwanted abandon.
theorem caller_guard_is_abandoned : abandonedOnCancel .caller = true := by
  rfl

-- A completed edge-admitted begin returns an open transaction only while its result is held.
-- If its waiter left, dropping the guarded result abandons that transaction.
def finishBegin (waiter : Bool) : Bool × Bool :=
  if waiter then (true, true) else (false, false)

-- Every open result of a guarded begin has an owner.
theorem begin_open_has_owner (waiter : Bool) :
    (finishBegin waiter).1 = true → (finishBegin waiter).2 = true := by
  cases waiter <;> simp [finishBegin]

-- A begin completing after its waiter left abandons the unreturned handle.
theorem abandoned_begin_is_closed : finishBegin false = (false, false) := by
  rfl

end RequestDrain
