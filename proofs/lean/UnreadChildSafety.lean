/-!
A DAG child can be confirmed absent, successfully read, or unreadable after a
positive existence check. Only a negative existence check is a normal missing
frontier. An unreadable or contradictory result is a local fault. A clock
attempt that sees that fault returns its permit and preserves its acknowledged
durable owner. A reply-driven task uses its existing bounded attempts and then
returns a typed failure, observed by its live caller without inventing a lease.

This proof covers only these pure local transitions. UnreadChildOwnership.tla
models a single clock or external owner with explicitly assumed eventual repair.
Neither proves all Rust code paths or concurrent storage and scheduler behavior.
-/
namespace UnreadChildSafety -- Keep these proof names together.

-- The three possible outcomes of checking whether a child exists.
inductive Existence where -- List the existence-check outcomes.
  | present -- The store reported a present child.
  | absent -- The store reported a missing child.
  | fault -- The existence check failed.
  deriving DecidableEq -- Allow equality checks between existence outcomes.

-- The possible result of trying to read a child after a positive `has`.
inductive ChildRead where -- List the child-read outcomes.
  | bytes -- The child returned bytes that can be parsed.
  | none -- The child contradicted an earlier positive `has`.
  | ioError -- Storage failed to return a child.
  | parseError -- The returned bytes cannot be interpreted.
  deriving DecidableEq -- Allow equality checks between read outcomes.

-- What the caller is allowed to conclude about this child.
inductive Verdict where -- List the three possible conclusions.
  | covered -- The child was read and parsed.
  | missing -- A negative `has` confirmed absence.
  | localFault -- An error or contradictory observation prevents a decision.
  deriving DecidableEq -- Allow equality checks between conclusions.

-- A confirmed negative `has` is missing; otherwise the read must decide.
def classify (has : Existence) (read : ChildRead) : Verdict := -- Define how the two observations are classified.
  match has with -- Consider the result of the existence check.
  | .present => match read with -- If present, inspect the subsequent child read.
    | .bytes => .covered -- Parsed bytes cover this child.
    | .none => .localFault -- Missing bytes contradict the positive existence check.
    | .ioError => .localFault -- A failed read is a local storage fault.
    | .parseError => .localFault -- Invalid bytes are a local parse fault.
  | .absent => .missing -- A confirmed negative check names a missing child.
  | .fault => .localFault -- A failed check cannot prove absence.

-- Ordinary confirmed absence is still available to the fetch path.
theorem negative_has_is_missing (read : ChildRead) : classify .absent read = .missing := by -- State that every negative check yields missing.
  rfl -- The definition reduces to missing.

-- A failed existence check is a local fault, not a confirmed missing block.
theorem failed_has_is_fault (read : ChildRead) : classify .fault read = .localFault := by -- State that every failed check yields a fault.
  rfl -- The definition reduces to a fault.

-- A positive has followed by no bytes is inconclusive, not network absence.
theorem inconsistent_read_is_fault : classify .present .none = .localFault := by -- State that a contradictory read yields a fault.
  rfl -- The definition reduces to a fault.

-- A storage error cannot be made into a complete or missing frontier.
theorem io_error_is_fault : classify .present .ioError = .localFault := by -- State that a storage read error yields a fault.
  rfl -- The definition reduces to a fault.

-- A parser error cannot be made into a complete or missing frontier.
theorem parse_error_is_fault : classify .present .parseError = .localFault := by -- State that a parser error yields a fault.
  rfl -- The definition reduces to a fault.

-- The relevant parts of a clock or reply-driven root's ownership state.
structure OwnerState where -- Keep the state needed for one owner.
  ack : Bool -- The sender has already been told that this root was accepted.
  durable : Bool -- The root has a durable receiver retry obligation.
  permit : Bool -- This attempt holds a fetch permit.
  backoff : Bool -- The existing receiver clock must wait before another turn.
  reported : Bool -- A live reply-driven caller observed a local failure.
  ready : Bool -- The root was published as ready to merge.
  localFailure : Bool -- The bounded reply owner returned a typed local failure.
  peerBlamed : Bool -- Terminal exhaustion/disconnect was attributed to a provider.

-- A clock fault returns the permit while keeping its record and ack unchanged.
def clockFault (s : OwnerState) : OwnerState := -- Define the fault transition for a clock owner.
  { s with permit := false, backoff := true } -- Return the permit and set its backoff.

-- A reply with attempts left keeps its existing owner and waits without a slot.
def replyWait (s : OwnerState) : OwnerState := -- Define the reply retry boundary.
  { s with permit := false, backoff := true } -- Release its permit before backoff.

-- Exhaustion returns a typed local failure; it does not terminally blame a peer.
-- Per-window provider health updates and cooling are outside this model.
def replyExhausted (s : OwnerState) : OwnerState := -- Define the bounded owner's result.
  { s with permit := false, localFailure := true } -- Return a local-failure result without a permit.

-- The still-live caller observes the typed result locally.
def observeReply (s : OwnerState) : OwnerState := -- Define local caller observation.
  { s with reported := true } -- This records observation, not guaranteed alert delivery.

-- A clock fault preserves every existing durable retry obligation.
theorem clock_preserves_owner (s : OwnerState) : (clockFault s).durable = s.durable := by -- State that the durable record stays the same.
  rfl -- The update does not modify the record.

-- A clock fault returns the fetch permit and enters paced backoff.
theorem clock_releases_and_paces (s : OwnerState) : -- State the permit and pacing promise.
    (clockFault s).permit = false ∧ (clockFault s).backoff = true := by -- The fault gives the permit back and enters backoff.
  exact ⟨rfl, rfl⟩ -- Both fields reduce to the stated values.

-- A paced reply retry releases its permit without inventing a durable lease.
theorem reply_wait_preserves_owner (s : OwnerState) : -- State the reply pacing and ownership promise.
    (replyWait s).permit = false ∧ (replyWait s).durable = s.durable := by -- The same owner waits without a permit.
  exact ⟨rfl, rfl⟩ -- Both fields reduce to the stated values.

-- Exhaustion returns a typed failure and does not itself assign terminal blame.
theorem reply_exhausted_is_local (s : OwnerState) : -- State the returned failure and terminal-accounting promise.
    (replyExhausted s).localFailure = true ∧ (replyExhausted s).peerBlamed = s.peerBlamed := by -- The fault is local to the receiver.
  exact ⟨rfl, rfl⟩ -- Both fields reduce to the stated values.

-- A live caller can observe the failure without changing its retry owner.
theorem reply_observation_is_local (s : OwnerState) : -- State the local observation promise.
    (observeReply s).reported = true ∧ (observeReply s).durable = s.durable := by -- No delivery or durability guarantee is invented.
  exact ⟨rfl, rfl⟩ -- Both fields reduce to the stated values.

-- Neither failure transition itself marks an incomplete root as ready.
theorem fault_keeps_ready_state (s : OwnerState) : -- State that the ready state is unchanged.
    (clockFault s).ready = s.ready ∧ (replyExhausted s).ready = s.ready := by -- Neither clock nor reply fault changes it.
  exact ⟨rfl, rfl⟩ -- Both fields reduce to their previous values.

-- The final classification after a bounded reply owner fails every attempt.
inductive Exhaustion where -- Distinguish receiver storage failure from provider failure.
  | local -- At least one attempt could not inspect local storage.
  | network -- Every attempt failed without a local storage fault.
  deriving DecidableEq -- Permit equality checks between these outcomes.

-- A remembered local fault takes precedence over later network failures.
def exhaustion (sawLocal : Bool) : Exhaustion := -- Classify the exhausted owner.
  if sawLocal then .local else .network -- Never call mixed failures pure network exhaustion.

-- A local fault in any attempt prevents network-only exhaustion.
theorem local_fault_wins : exhaustion true = .local := by -- State the mixed-attempt rule.
  rfl -- The classifier returns the local outcome.

-- Without a local fault, the existing network classification remains.
theorem all_network_remains_network : exhaustion false = .network := by -- State the unaffected network-only case.
  rfl -- The classifier returns the network outcome.

-- The existing early outcomes never erase an actual local fault.
inductive EarlyExitKind where -- Name the early non-success outcomes.
  | unservable -- The provider cannot serve the remaining frontier.
  | deferred -- Ingest is owned elsewhere.
  | providerBusy -- Providers shed this attempt.
  deriving DecidableEq -- Permit equality checks for the outcomes.

-- A local fault takes precedence when an early non-success ends the owner.
def earlyResult (sawLocal : Bool) (kind : EarlyExitKind) : Bool := -- Return whether the typed result is LocalFailure.
  if sawLocal then true else match kind with -- Preserve the prior fault before classifying the provider outcome.
    | .unservable => false -- No earlier local fault occurred.
    | .deferred => false -- No earlier local fault occurred.
    | .providerBusy => false -- No earlier local fault occurred.

-- Every early non-success preserves an already recorded local fault.
theorem early_exit_preserves_local (kind : EarlyExitKind) : earlyResult true kind = true := by -- State the latch rule for each kind.
  rfl -- The local-fault condition determines the result.

-- A failed optimization either returns a clock turn or keeps a reply attempt.
inductive PreflightOwner where -- Name the existing fetch owners.
  | clock -- A durable receiver clock dispatched this turn.
  | reply -- A reply-driven task owns the bounded attempt loop.
  deriving DecidableEq -- Permit equality checks for these owners.

-- What happens immediately after the preflight local read fails.
inductive PreflightNext where -- Distinguish paced clock return from reply continuation.
  | clockBackoff -- Give the reserved permit back to the existing clock.
  | replyAttempt -- Continue to the already-owned bounded fetch attempt.
  | localFailure -- A disconnected provider returns a typed local failure.
  deriving DecidableEq -- Permit equality checks for the next action.

-- A preflight error chooses the next step without charging an attempt.
def preflightError (owner : PreflightOwner) (connected : Bool) : PreflightNext := -- Keep the existing owner.
  match owner with -- Select the owner's existing recovery path.
  | .clock => .clockBackoff -- A durable root yields to its clock.
  | .reply => if connected then .replyAttempt else .localFailure -- A reply attempts when connected, otherwise reports locally.

-- A reply-driven preflight fault does not abandon the bounded owner.
theorem reply_preflight_continues : preflightError .reply true = .replyAttempt := by -- State the reply-owner rule.
  rfl -- The classifier continues to the actual attempt.

-- An acknowledged clock preflight fault gives control back to its clock.
theorem clock_preflight_paces : preflightError .clock true = .clockBackoff := by -- State the clock-owner rule.
  rfl -- The classifier yields to paced clock recovery.

theorem disconnected_reply_reports : preflightError .reply false = .localFailure := by -- State the unavailable-provider outcome.
  rfl -- The classifier returns the typed failure.

end UnreadChildSafety -- Close this proof namespace.
