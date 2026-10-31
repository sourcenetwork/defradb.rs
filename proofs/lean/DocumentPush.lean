/-!
Explicit selected-document transfer is independent of continuous replication.
It owns only its selected heads and reports failure unless they are all
acknowledged, even when a subscription exists or disappears during the transfer.
Initial replay instead requires an installed replicator; removal
cannot turn it into an independent transfer or manufacture durable retry work.
The existing rooted grant and retry-registration owners retain their contracts.

A selected call also stops early. Each PushLog ends in one of five ways. Two
of them (the receiver acknowledged the head, or refused just that head) leave
the peer able to take the rest. The other three (a receiver pause hint, the
send's own deadline, a dropped connection) prove the transfer cannot progress
now, so the call raises its stop flag and admits no more sends. A stopped call
never reports success, and success means every selected document was
acknowledged in this call. The concurrent bounds (late sends and timeouts at
most the send slots, cancellation through the pool) are SelectedPushIsolation.tla.
Rust: crates/db/src/merge/push_docs.rs (the selected task's outcome handling),
crates/db/src/merge/push_docs/stop.rs SelectedPushStop.
-/
namespace DocumentPush

inductive Ownership where
  | selected
  | replicator
  deriving DecidableEq, Repr

/-- An explicit call owns its acknowledgements; a subscription cannot defer them. -/
def ownership (explicit present : Bool) : Option Ownership :=
  if explicit then some .selected else if present then some .replicator else none

/-- Every dispatched head remains inside the selection and owns a serving grant. -/
def canSend (owner : Ownership) (present insideSelection hasGrant : Bool) : Bool :=
  insideSelection && hasGrant && (owner == .selected || present)

inductive Completion where
  | acknowledged
  | retainedRetry
  | failed
  deriving DecidableEq, Repr

/-- A retained retry is an outcome only when the durable retry owner kept it. -/
def completion (owner : Ownership) (allHeadsAcknowledged retryRetained : Bool) : Completion :=
  if allHeadsAcknowledged then .acknowledged
  else if owner == .replicator && retryRetained then .retainedRetry else .failed

/-- Retained observation belongs to the installed peer, independently of transfer ownership.
Removing the peer discards its observations; an independent transfer still reports
its actual acknowledgement without creating a subscription. -/
def retainedAcknowledgement (explicit present acknowledged : Bool) : Bool :=
  (ownership explicit present).isSome && present && acknowledged

/-- A selected transfer retains accepted heads while the peer remains installed. -/
theorem selected_retains_heads (present acknowledged : Bool) :
    retainedAcknowledgement true present acknowledged = (present && acknowledged) := by
  simp [retainedAcknowledgement, ownership]

/-- Successful standalone transfers do not invent retained peer state. -/
theorem absent_peer_has_no_observation (explicit acknowledged : Bool) :
    retainedAcknowledgement explicit false acknowledged = false := by
  cases explicit <;> simp [retainedAcknowledgement, ownership]

theorem enrollment_without_subscription : ownership true false = some .selected := rfl

theorem absent_replay_is_refused : ownership false false = none := rfl

/-- Subscription presence cannot change an explicitly selected transfer's owner. -/
theorem selection_owns_acknowledgements (present : Bool) :
    ownership true present = some .selected := rfl

/-- Continuous replay still requires and retains its installed subscription. -/
theorem installed_replay_is_owned :
    ownership false true = some .replicator := rfl

theorem removed_replay_cannot_send (insideSelection hasGrant : Bool) :
    canSend .replicator false insideSelection hasGrant = false := by
  simp [canSend]

theorem unselected_head_cannot_send (owner : Ownership) (present hasGrant : Bool) :
    canSend owner present false hasGrant = false := by
  simp [canSend]

theorem ungranted_head_cannot_send (owner : Ownership) (present insideSelection : Bool) :
    canSend owner present insideSelection false = false := by
  simp [canSend]

theorem selected_failure_is_not_deferred (retryRetained : Bool) :
    completion .selected false retryRetained = .failed := rfl

theorem replay_without_retry_cannot_defer :
    completion .replicator false false = .failed := rfl

/-- How one selected PushLog ended. -/
inductive SendOutcome where
  -- The receiver accepted the head.
  | acknowledged
  -- The receiver refused this one head and gave no pause hint.
  | refused
  -- The receiver asked the sender to pause (a retry-after hint).
  | hinted
  -- The send's own deadline passed with no reply.
  | timedOut
  -- The connection to the peer dropped under the send.
  | disconnected
  deriving DecidableEq, Repr

/-- Whether an outcome proves the transfer cannot progress now, so the call
raises its stop flag. -/
def stopsTransfer : SendOutcome → Bool
  -- An acknowledged head is progress.
  | .acknowledged => false
  -- One refused head says nothing about the others.
  | .refused => false
  -- A pause hint means every further send would be refused too.
  | .hinted => true
  -- A silent peer would cost one more deadline per further send.
  | .timedOut => true
  -- No further send can reach a peer that is gone.
  | .disconnected => true

/-- The scheduler starts a document, and a waiting task sends its next block,
only while the stop flag is down. -/
def admits (stopped : Bool) : Bool := !stopped

/-- Exactly the outcomes other than progress and a one-head refusal stop the call. -/
theorem no_progress_stops (o : SendOutcome) :
    stopsTransfer o = (o != .acknowledged && o != .refused) := by
  -- Each of the five outcomes is checked by computation.
  cases o <;> rfl

/-- Once the flag is raised nothing more is admitted. -/
theorem stopped_admits_nothing : admits true = false := rfl

/-- One selected call with one send slot, the executable shape of the Rust call
with its slot counts set to one: each listed outcome ends the next document's
send, and the first stopping outcome ends the call. Returns how many PushLogs
were sent and whether every selected document was acknowledged. -/
def serialRun : List SendOutcome → Nat × Bool
  -- Nothing left to send: no PushLog, and nothing unacknowledged.
  | [] => (0, true)
  | o :: rest =>
    -- A stopping outcome: this one PushLog was sent, the rest never are.
    if stopsTransfer o then (1, false)
    -- Otherwise count this PushLog, keep going, and remember a refusal.
    else ((serialRun rest).1 + 1, (o == .acknowledged) && (serialRun rest).2)

/-- A call that met a stopping outcome never reports success. -/
theorem stopped_selection_fails :
    ∀ (l : List SendOutcome), (∃ o ∈ l, stopsTransfer o = true) → (serialRun l).2 = false
  -- An empty call has no outcome to stop on.
  | [], ⟨_, h, _⟩ => by simp at h
  | o :: rest, ⟨w, hw, hs⟩ => by
    by_cases h : stopsTransfer o = true
    -- The first outcome stops the call: it fails right there.
    · simp [serialRun, h]
    -- Otherwise the stopping outcome is later, and the later call fails.
    · have hw' : w ∈ rest := by
        rcases List.mem_cons.mp hw with rfl | hr
        · exact absurd hs h
        · exact hr
      have later := stopped_selection_fails rest ⟨w, hw', hs⟩
      simp [serialRun, h, later]

/-- Success means every selected document's PushLog was acknowledged. -/
theorem selected_success_needs_every_acknowledgement :
    ∀ (l : List SendOutcome), (serialRun l).2 = true → ∀ o ∈ l, o = .acknowledged
  -- An empty call has no documents to check.
  | [], _, _, h => by simp at h
  | x :: rest, hok, o, ho => by
    by_cases h : stopsTransfer x = true
    -- A stopped call cannot have reported success.
    · simp [serialRun, h] at hok
    -- Otherwise success needs this head acknowledged and the rest successful.
    · simp [serialRun, h] at hok
      rcases List.mem_cons.mp ho with rfl | hr
      · exact hok.1
      · exact selected_success_needs_every_acknowledgement rest hok.2 o hr

/-- After the first stopping outcome no further PushLog is sent: the call sends
the documents before it, that one, and nothing after. -/
theorem first_stop_ends_sending (o : SendOutcome) (post : List SendOutcome)
    (hs : stopsTransfer o = true) :
    ∀ (pre : List SendOutcome), (∀ x ∈ pre, stopsTransfer x = false) →
      (serialRun (pre ++ o :: post)).1 = pre.length + 1
  -- The stopping outcome comes first: exactly one PushLog.
  | [], _ => by simp [serialRun, hs]
  | x :: rest, hpre => by
    -- An earlier outcome that does not stop counts once and moves on.
    have hx : stopsTransfer x = false := hpre x (by simp)
    have later := first_stop_ends_sending o post hs rest
      (fun y hy => hpre y (by simp [hy]))
    simp [serialRun, hx, later]

/-- A silent peer costs one deadline per send slot, however many documents
were selected. -/
theorem silent_peer_costs_one_deadline (n : Nat) :
    (serialRun (List.replicate (n + 1) .timedOut)).1 = 1 := by
  -- The first timeout stops the call before the second document is sent.
  simp [List.replicate, serialRun, stopsTransfer]

/-- The name the native conformance test uses for each outcome. -/
def outcomeTag : SendOutcome → String
  | .acknowledged => "acknowledged"
  | .refused => "refused"
  | .hinted => "hinted"
  | .timedOut => "timedOut"
  | .disconnected => "disconnected"

/-- The executable owner derives inputs and expectations for native transfer cases. -/
def emit : IO Unit := do
  for explicit in [false, true] do
    for present in [false, true] do
      for acknowledged in [false, true] do
        let owner := ownership explicit present
        -- The native binding reads ownership from this executable owner.
        let ownerTag := match owner with
          | none => "refused"
          | some .selected => "selected"
          | some .replicator => "replicator"
        let outcome := owner.map fun own => completion own acknowledged present
        let tag := match outcome with
          | none => "refused"
          | some .acknowledged => "acknowledged"
          | some .retainedRetry => "retainedRetry"
          | some .failed => "failed"
        IO.println (String.intercalate "\t"
          [toString explicit, toString present, toString acknowledged, ownerTag, tag,
           toString (retainedAcknowledgement explicit present acknowledged)])
  -- Stop cases: three selected documents through one send slot, the first send
  -- ending with each outcome and every later one acknowledged.
  for o in [SendOutcome.acknowledged, .refused, .hinted, .timedOut, .disconnected] do
    let (sends, ok) := serialRun [o, .acknowledged, .acknowledged]
    IO.println (String.intercalate "\t"
      ["stop", outcomeTag o, toString (stopsTransfer o), toString sends, toString ok])

end DocumentPush

def main : IO Unit := DocumentPush.emit
