/-!
Explicit selected-document transfer is independent of continuous replication.
It owns only its selected heads and reports failure unless they are all
acknowledged, even when a subscription exists or disappears during the transfer.
Initial replay instead requires an installed replicator; removal
cannot turn it into an independent transfer or manufacture durable retry work.
The existing rooted grant and retry-registration owners retain their contracts.

A receiver capacity hint pauses this call and retries the same head: it is not
an acknowledgement and does not end a healthy large selection. The pause/retry
allowance is fixed by the existing send timeout, never reset by more hints or
acknowledgements. Exhausting it, a send deadline, a disconnect or caller drop
stops scheduling; started sends still drain. SelectedPushIsolation.tla models
the concurrency and the fixed pause budget; this owner binds attempt outcomes
to exact completion.
Rust: crates/db/src/merge/push_docs.rs and push_docs/stop.rs.
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

/-- One reply or the expiry of the current head's pause allowance. -/
inductive SendOutcome where
  | acknowledged | refused | hinted | timedOut | disconnected | pauseExpired
  deriving DecidableEq, Repr

/-- A pause retries the same head; hard failures and lack of progress end the call. -/
def stopsTransfer : SendOutcome → Bool
  | .acknowledged | .refused | .hinted => false
  | .timedOut | .disconnected | .pauseExpired => true

def admits (stopped : Bool) : Bool := !stopped

theorem no_progress_stops (o : SendOutcome) :
    stopsTransfer o = (o == .timedOut || o == .disconnected || o == .pauseExpired) := by
  cases o <;> rfl

theorem pause_does_not_stop : stopsTransfer .hinted = false := rfl

theorem stopped_admits_nothing : admits true = false := rfl

/-- `required` heads remain. Hints consume attempts, never heads or acknowledgements. -/
def serialRun : Nat → List SendOutcome → Nat × Bool
  | 0, _ => (0, true)
  | _ + 1, [] => (0, false)
  | n + 1, o :: rest =>
    if stopsTransfer o then (1, false)
    else
      let next := serialRun (if o == .hinted then n + 1 else n) rest
      (next.1 + 1, (o == .acknowledged || o == .hinted) && next.2)

theorem stopped_selection_fails (n : Nat) (o : SendOutcome) (rest : List SendOutcome)
    (hs : stopsTransfer o = true) : (serialRun (n + 1) (o :: rest)).2 = false := by
  simp [serialRun, hs]

def acknowledgements : List SendOutcome → Nat
  | [] => 0
  | .acknowledged :: rest => acknowledgements rest + 1
  | _ :: rest => acknowledgements rest

/-- Success requires a fresh acknowledgement for every required head. -/
theorem selected_success_needs_every_acknowledgement (l : List SendOutcome) :
    ∀ required, (serialRun required l).2 = true → required ≤ acknowledgements l := by
  induction l with
  | nil => intro required; cases required <;> simp [serialRun, acknowledgements]
  | cons o rest ih =>
    intro required success
    cases required with
    | zero => exact Nat.zero_le _
    | succ n =>
      cases o <;> simp [serialRun, stopsTransfer, acknowledgements] at success ⊢
      · have h := ih n success
        omega
      · exact ih (n + 1) success

theorem pause_requires_ack : serialRun 1 [.hinted] = (1, false) := rfl

theorem pause_then_ack_succeeds : serialRun 1 [.hinted, .acknowledged] = (2, true) := rfl

theorem repeated_pause_is_bounded :
    serialRun 1 [.hinted, .hinted, .pauseExpired] = (3, false) := rfl

/-- Each fresh acknowledgement advances to a new head with its own allowance. -/
def healthyHeadPauses (allowance : Nat) (pauses : List Nat) : Bool :=
  pauses.all (fun delay => delay < allowance)

/-- Hints for one unacknowledged head consume its one budget; they never reset it. -/
def sameHeadPauses (allowance : Nat) (pauses : List Nat) : Bool :=
  pauses.foldl (· + ·) 0 < allowance

theorem fresh_heads_do_not_inherit_elapsed : healthyHeadPauses 2 [1, 1, 1] = true := rfl

theorem same_head_hints_do_not_extend : sameHeadPauses 2 [1, 1] = false := rfl

/-- The first hard failure ends sending after the preceding acknowledged heads. -/
theorem first_stop_ends_sending (o : SendOutcome) (post : List SendOutcome)
    (hs : stopsTransfer o = true) :
    ∀ pre, (∀ x ∈ pre, x = .acknowledged) →
      (serialRun (pre.length + 1) (pre ++ o :: post)).1 = pre.length + 1
  | [], _ => by simp [serialRun, hs]
  | x :: rest, hpre => by
      have hx := hpre x (by simp)
      subst x
      have later := first_stop_ends_sending o post hs rest (fun y hy => hpre y (by simp [hy]))
      simpa [serialRun, stopsTransfer, List.length_cons] using later

theorem silent_peer_costs_one_deadline (n : Nat) :
    (serialRun (n + 1) (List.replicate (n + 1) .timedOut)).1 = 1 := by
  cases n <;> simp [List.replicate, serialRun, stopsTransfer]

/-- The name the native conformance test uses for each outcome. -/
def outcomeTag : SendOutcome → String
  | .acknowledged => "acknowledged"
  | .refused => "refused"
  | .hinted => "hinted"
  | .timedOut => "timedOut"
  | .disconnected => "disconnected"
  | .pauseExpired => "pauseExpired"

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
    let attempts := if o == .hinted then [o, .acknowledged, .acknowledged, .acknowledged]
                    else [o, .acknowledged, .acknowledged]
    let (sends, ok) := serialRun 3 attempts
    IO.println (String.intercalate "\t"
      ["stop", outcomeTag o, toString (stopsTransfer o), toString sends, toString ok])

  IO.println (String.intercalate "\t"
    ["head_budgets", "3", "100", "60", toString (healthyHeadPauses 100 [60, 60, 60])])

  let exhausted := serialRun 1 [.hinted, .hinted, .pauseExpired]
  IO.println (String.intercalate "\t" ["pause_limit", toString (stopsTransfer .pauseExpired), toString exhausted.2])

end DocumentPush

def main : IO Unit := DocumentPush.emit
