/-!
Explicit selected-document transfer is independent of continuous replication.
It owns only its selected heads and reports failure unless they are all
acknowledged, even when a subscription exists or disappears during the transfer.
Initial replay instead requires an installed replicator; removal
cannot turn it into an independent transfer or manufacture durable retry work.
The existing rooted grant and retry-registration owners retain their contracts.

A receiver capacity hint pauses this call and retries the same head: it is not
an acknowledgement and does not end a healthy large selection. Every head has
its own pause/retry allowance, the send timeout, which starts at that head's
first actual send. More hints for the same head never extend it; an
acknowledgement moves on to the next head, which starts a fresh allowance of
its own. Exhausting a head's allowance defers that one head: it fails alone and
the rest of the selection is still tried, because the receiver is answering. A
send deadline, a disconnect or caller drop stops scheduling; started sends
still drain. SelectedPushIsolation.tla models the concurrency and the per-head
pause budget; this owner binds attempt outcomes to exact completion.
Rust: crates/db/src/merge/push_docs.rs, push_docs/selected.rs and push_docs/stop.rs.
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

/-- A silent peer or a lost connection ends the call. A pause retries the same
head, and a head whose pauses outlast its deadline fails alone, like a refusal. -/
def stopsTransfer : SendOutcome → Bool
  -- The receiver answered: the transfer can still make progress.
  | .acknowledged | .refused | .hinted | .pauseExpired => false
  -- The peer is silent or unreachable: nothing more can be sent now.
  | .timedOut | .disconnected => true

/-- A task may send only while the call's stop flag is down. -/
def admits (stopped : Bool) : Bool := !stopped

/-- Exactly a timeout or a disconnect stops the transfer. -/
theorem no_progress_stops (o : SendOutcome) :
    stopsTransfer o = (o == .timedOut || o == .disconnected) := by
  -- Each outcome is checked by evaluating both sides.
  cases o <;> rfl

/-- A receiver pause never stops the transfer. -/
theorem pause_does_not_stop : stopsTransfer .hinted = false := rfl

/-- A head whose pauses outlast its deadline does not stop the transfer. -/
theorem pause_expiry_does_not_stop : stopsTransfer .pauseExpired = false := rfl

/-- A raised stop flag admits no task. -/
theorem stopped_admits_nothing : admits true = false := rfl

/-- `required` heads remain. Hints consume attempts, never heads or acknowledgements. -/
def serialRun : Nat → List SendOutcome → Nat × Bool
  -- No head is left: the run succeeded.
  | 0, _ => (0, true)
  -- Heads are left but no attempt is: the run failed.
  | _ + 1, [] => (0, false)
  -- One more attempt with outcome `o`.
  | n + 1, o :: rest =>
    -- A stopping outcome ends the run after this one send, as a failure.
    if stopsTransfer o then (1, false)
    else
      -- A hint keeps the same head; any other outcome moves to the next head.
      let next := serialRun (if o == .hinted then n + 1 else n) rest
      -- Count this send; succeed only if this head was not lost and the rest succeed.
      (next.1 + 1, (o == .acknowledged || o == .hinted) && next.2)

/-- A stopping outcome makes the whole selection fail. -/
theorem stopped_selection_fails (n : Nat) (o : SendOutcome) (rest : List SendOutcome)
    (hs : stopsTransfer o = true) : (serialRun (n + 1) (o :: rest)).2 = false := by
  -- The run takes the stopping branch at once.
  simp [serialRun, hs]

/-- How many attempts in a run were acknowledgements: the heads the receiver
accepted. -/
def acknowledgements : List SendOutcome → Nat
  -- No attempts: nothing was acknowledged.
  | [] => 0
  -- An acknowledgement counts one, plus whatever the later attempts count.
  | .acknowledged :: rest => acknowledgements rest + 1
  -- Any other outcome (a pause, refusal, expiry or stop) counts nothing.
  | _ :: rest => acknowledgements rest

/-- Success requires a fresh acknowledgement for every required head. -/
theorem selected_success_needs_every_acknowledgement (l : List SendOutcome) :
    ∀ required, (serialRun required l).2 = true → required ≤ acknowledgements l := by
  -- Induct on the attempts.
  induction l with
  -- With no attempts, success means no head was required.
  | nil => intro required; cases required <;> simp [serialRun, acknowledgements]
  | cons o rest ih =>
    intro required success
    cases required with
    -- No head required: zero acknowledgements are enough.
    | zero => exact Nat.zero_le _
    | succ n =>
      -- Only an acknowledgement or a hint can lead to success; the other cases are false.
      cases o <;> simp [serialRun, stopsTransfer, acknowledgements] at success ⊢
      -- An acknowledgement: the rest acknowledged the remaining n heads, plus this one.
      · have h := ih n success
        omega
      -- A hint: the rest acknowledged all n + 1 heads, this attempt none.
      · exact ih (n + 1) success

/-- A pause alone is not success. -/
theorem pause_requires_ack : serialRun 1 [.hinted] = (1, false) := rfl

/-- A pause followed by an acknowledgement succeeds. -/
theorem pause_then_ack_succeeds : serialRun 1 [.hinted, .acknowledged] = (2, true) := rfl

/-- Pauses that outlast the head's deadline fail that head. -/
theorem repeated_pause_is_bounded :
    serialRun 1 [.hinted, .hinted, .pauseExpired] = (3, false) := rfl

/-- A head deferred by its pauses fails alone: the other heads are still sent
and acknowledged, and the call reports the deferred head as a failure. -/
theorem expired_head_leaves_the_rest :
    serialRun 3 [.pauseExpired, .acknowledged, .acknowledged] = (3, false) := rfl

/-- Each fresh acknowledgement advances to a new head with its own allowance. -/
def healthyHeadPauses (allowance : Nat) (pauses : List Nat) : Bool :=
  -- Each head's own pause fits within its own allowance.
  pauses.all (fun delay => delay < allowance)

/-- Hints for one unacknowledged head consume its one budget; they never reset it. -/
def sameHeadPauses (allowance : Nat) (pauses : List Nat) : Bool :=
  -- All of one head's pauses together must fit within its one allowance.
  pauses.sum < allowance

/-- However many heads a selection has, each head that pauses for less than its
own allowance stays healthy: no head inherits the time earlier heads spent. -/
theorem fresh_heads_do_not_inherit_elapsed (allowance : Nat) (pauses : List Nat)
    (fits : ∀ delay ∈ pauses, delay < allowance) :
    healthyHeadPauses allowance pauses = true := by
  -- `all` is true exactly when every head's pause fits, which is the assumption.
  simpa [healthyHeadPauses] using fits

/-- Three heads together pause longer than one allowance, yet each fits its own,
so all three stay healthy where one shared budget would have expired. -/
example : healthyHeadPauses 2 [1, 1, 1] = true ∧ sameHeadPauses 2 [1, 1, 1] = false := by
  -- Both sides are computed directly.
  decide

/-- A head's later pauses only add to the time it has spent. -/
theorem spent_only_grows (earlier later : List Nat) :
    earlier.sum ≤ (earlier ++ later).sum := by
  -- Induct on the earlier pauses.
  induction earlier with
  -- With no earlier pauses, the earlier total is zero.
  | nil => simp
  -- One more earlier pause adds the same amount to both totals.
  | cons delay rest ih =>
    simp only [List.cons_append, List.sum_cons]
    -- The rest already satisfies the bound, so adding `delay` to both keeps it.
    omega

/-- Once one head's pauses have used up its allowance, no further hint for that
head ever gives it room again: more hints never extend a head's deadline. -/
theorem same_head_hints_do_not_extend (allowance : Nat) (earlier later : List Nat)
    (spent : sameHeadPauses allowance earlier = false) :
    sameHeadPauses allowance (earlier ++ later) = false := by
  -- The earlier pauses already reach the allowance.
  simp [sameHeadPauses] at spent ⊢
  -- The later pauses only add to that total.
  have grows := spent_only_grows earlier later
  -- So the whole total reaches the allowance too.
  omega

/-- The first hard failure ends sending after the preceding acknowledged heads. -/
theorem first_stop_ends_sending (o : SendOutcome) (post : List SendOutcome)
    (hs : stopsTransfer o = true) :
    ∀ pre, (∀ x ∈ pre, x = .acknowledged) →
      (serialRun (pre.length + 1) (pre ++ o :: post)).1 = pre.length + 1
  -- No acknowledged heads before it: the stopping outcome is the only send.
  | [], _ => by simp [serialRun, hs]
  | x :: rest, hpre => by
      -- The first earlier attempt is an acknowledgement.
      have hx := hpre x (by simp)
      subst x
      -- By induction, the rest of the earlier heads plus the stop send one each.
      have later := first_stop_ends_sending o post hs rest (fun y hy => hpre y (by simp [hy]))
      -- This acknowledgement adds one send in front of them.
      simpa [serialRun, stopsTransfer, List.length_cons] using later

/-- A silent peer costs one send, not one per document. -/
theorem silent_peer_costs_one_deadline (n : Nat) :
    (serialRun (n + 1) (List.replicate (n + 1) .timedOut)).1 = 1 := by
  -- The first timeout stops the run, whatever n is.
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
  for o in [SendOutcome.acknowledged, .refused, .hinted, .timedOut, .disconnected, .pauseExpired] do
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
