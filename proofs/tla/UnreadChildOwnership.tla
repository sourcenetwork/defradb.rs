---- MODULE UnreadChildOwnership ----
\* An unreadable DAG child is a local fault. An acknowledged clock root
\* releases its permit and returns to its existing durable, paced owner. A
\* reply-driven root keeps its existing bounded fetch task, releases permits
\* during backoff, and returns a typed local failure after its last attempt.
\* The live background caller observes that result locally; this is not a
\* guaranteed alert, a remote nack, or a durable reply-driven retry obligation.
\* A new Push that fails its walk cannot acknowledge without a durable owner.
\*
\* This finite model has one root, three reply attempts, no contention, and no
\* real backoff duration. Clock cancellation, restart and valid supersession
\* preserve ownership as abstract steps; durable transaction atomicity and
\* successor linkage are assumptions. Reply caller cancellation, live network
\* failures, multi-root fairness and recovery after process restart are outside
\* the reply-driven model. A liveness configuration assumes fair scheduling.
\*
\* Rust correspondence: sync/coordinator/dag_fetcher.rs poll_fetch_dag;
\* sync/replication/handlers.rs handle_dag_needs_fetch;
\* sync/coordinator/dag_context.rs finish_clock_attempt;
\* sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt;
\* sync/coordinator/event_handler/{doc_sync,branchable_sync}.rs reply handlers.
\* peerBlamed records terminal exhaustion/disconnect attribution only;
\* per-window provider health updates and cooling are not modeled here.
EXTENDS Naturals  \* Use ordinary booleans and numbers.
CONSTANTS Origin, LoseOnError, InlineRetry, AckOnError, EnableInterrupt, LoseOnCancel, LoseOnRestart, LoseOnSupersede, EarlyReplyExit, HoldReplyPermit, BlamePeer, IgnoreReply  \* Choose the owner and planted guard mutations.
ASSUME /\ Origin \in {"Clock", "Reply", "Push"}  \* The owner is the receiver clock, a reply task, or a new Push.
       /\ LoseOnError \in BOOLEAN /\ InlineRetry \in BOOLEAN /\ AckOnError \in BOOLEAN  \* These switches plant owner, pacing and acknowledgement bugs.
       /\ EnableInterrupt \in BOOLEAN /\ LoseOnCancel \in BOOLEAN /\ LoseOnRestart \in BOOLEAN /\ LoseOnSupersede \in BOOLEAN  \* These switches exercise clock ownership through interrupts.
       /\ EarlyReplyExit \in BOOLEAN /\ HoldReplyPermit \in BOOLEAN /\ BlamePeer \in BOOLEAN /\ IgnoreReply \in BOOLEAN  \* These switches plant reply-owner and reporting bugs.
MaxAttempts == 3  \* The existing reply owner allows at most three attempts.

VARIABLES phase, fault, verified, durable, ack, permit, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed  \* Track the attempt, evidence, owner and returned-result state.
vars == <<phase, fault, verified, durable, ack, permit, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Collect all state fields for unchanged steps.

Init == /\ phase = "Walk" /\ fault = TRUE /\ verified = FALSE  \* Begin an attempt with unreadable storage and no verified child.
        /\ durable = (Origin = "Clock") /\ ack = (Origin = "Clock")  \* Only the existing clock starts with an acknowledged durable root.
        /\ permit = TRUE /\ reported = FALSE /\ ready = FALSE /\ inlineCount = 0 /\ successor = FALSE  \* The task holds its permit and has not completed or reported.
        /\ attempts = 1 /\ localFailure = FALSE /\ peerBlamed = FALSE  \* This is the first attempt and no result or peer blame exists.

Repair == /\ fault /\ fault' = FALSE  \* The environment may repair the local storage fault.
          /\ UNCHANGED <<phase, verified, durable, ack, permit, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Repair itself changes no ownership or result.
WalkError == /\ phase = "Walk" /\ fault /\ phase' = "Error"  \* A faulty walk records no proof of completion.
             /\ UNCHANGED <<fault, verified, durable, ack, permit, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* The failed read changes no owner or permit.
WalkSuccess == /\ phase = "Walk" /\ ~fault /\ phase' = "Complete" /\ verified' = TRUE  \* A healthy walk inspects and verifies the child.
               /\ UNCHANGED <<fault, durable, ack, permit, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Reading the child does not alter ownership.
FinishSuccess == /\ phase = "Complete" /\ phase' = "Done" /\ ready' = TRUE /\ permit' = FALSE  \* A successful walk emits ready and releases its permit.
                 /\ UNCHANGED <<fault, verified, durable, ack, reported, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* No failure or peer blame is manufactured.

FinishClockError == /\ phase = "Error" /\ Origin = "Clock"  \* A clock-owned local fault takes this branch.
                    /\ phase' = IF InlineRetry THEN "Walk" ELSE "Backoff"  \* The existing clock waits unless the RED bug retries inline.
                    /\ durable' = IF LoseOnError THEN FALSE ELSE durable  \* The durable owner stays unless the RED bug discards it.
                    /\ permit' = IF InlineRetry THEN TRUE ELSE FALSE  \* The permit is released before backoff.
                    /\ inlineCount' = IF InlineRetry THEN 1 ELSE inlineCount  \* Record the planted inline-retry violation.
                    /\ UNCHANGED <<fault, verified, ack, reported, ready, successor, attempts, localFailure, peerBlamed>>  \* The sender ack and other fields remain unchanged.
DispatchClock == /\ phase = "Backoff" /\ Origin = "Clock" /\ durable /\ phase' = "Walk" /\ permit' = TRUE  \* After backoff the same durable owner takes another turn.
                 /\ UNCHANGED <<fault, verified, durable, ack, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Dispatch changes no other state.

FinishReplyError == /\ phase = "Error" /\ Origin = "Reply"  \* A reply-owned local fault takes this branch.
                    /\ IF attempts < MaxAttempts /\ ~EarlyReplyExit  \* If existing bounded attempts remain, the owner keeps its turn.
                         THEN /\ phase' = "Backoff" /\ localFailure' = FALSE  \* Wait before the next existing attempt, without returning failure yet.
                              /\ permit' = IF HoldReplyPermit THEN TRUE ELSE FALSE  \* Release the permit unless the RED bug holds it.
                         ELSE /\ phase' = "Returned" /\ localFailure' = TRUE /\ permit' = FALSE  \* After the bound, return typed local failure and release the permit.
                    /\ peerBlamed' = BlamePeer  \* Only the RED bug attributes terminal exhaustion to a local fault.
                    /\ UNCHANGED <<fault, verified, durable, ack, reported, ready, inlineCount, successor, attempts>>  \* This step creates no durable owner and no ready event.
ResumeReply == /\ phase = "Backoff" /\ Origin = "Reply" /\ attempts < MaxAttempts  \* The same reply owner may resume after backoff.
               /\ phase' = "Walk" /\ permit' = TRUE /\ attempts' = attempts + 1  \* Take a permit for one more bounded attempt.
               /\ UNCHANGED <<fault, verified, durable, ack, reported, ready, inlineCount, successor, localFailure, peerBlamed>>  \* Resuming creates no other owner or result.
ObserveReply == /\ phase = "Returned" /\ Origin = "Reply" /\ localFailure  \* The still-live background caller receives the typed result.
                /\ phase' = "Done" /\ reported' = ~IgnoreReply  \* It observes and logs the result unless the RED caller discards it.
                /\ UNCHANGED <<fault, verified, durable, ack, permit, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Logging is local and does not change recovery ownership.
FinishPushError == /\ phase = "Error" /\ Origin = "Push"  \* A new Push propagates its local walk error.
                   /\ phase' = "Done" /\ reported' = TRUE /\ permit' = FALSE  \* End its turn, report locally, and return the permit.
                   /\ ack' = IF AckOnError THEN TRUE ELSE ack  \* Only the RED bug sends success without a durable owner.
                   /\ UNCHANGED <<fault, verified, durable, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Do not create a durable owner or ready event.

CancelClock == /\ EnableInterrupt /\ Origin = "Clock" /\ permit /\ phase # "Done"  \* A running clock task may be cancelled.
               /\ phase' = "Backoff" /\ permit' = FALSE /\ verified' = FALSE  \* Return its permit and discard this turn's read evidence.
               /\ durable' = IF LoseOnCancel THEN FALSE ELSE durable  \* Preserve its durable record unless the RED bug drops it.
               /\ UNCHANGED <<fault, ack, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Cancellation does not revoke an ack or create terminal blame.
Restart == /\ EnableInterrupt /\ Origin = "Clock" /\ durable /\ phase # "Done"  \* A clock-owned process can restart.
           /\ phase' = "Backoff" /\ permit' = FALSE /\ verified' = FALSE  \* Restore a paced turn without the old permit or read evidence.
           /\ durable' = IF LoseOnRestart THEN FALSE ELSE durable  \* Preserve its record unless the RED bug loses it.
           /\ UNCHANGED <<fault, ack, reported, ready, inlineCount, successor, attempts, localFailure, peerBlamed>>  \* Restart does not change the ack or the peer.
Supersede == /\ Origin = "Clock" /\ durable /\ ~successor /\ phase # "Done"  \* An assumed valid successor may subsume this root.
             /\ phase' = "Done" /\ permit' = FALSE /\ durable' = FALSE  \* Retire the old turn and its record.
             /\ successor' = IF LoseOnSupersede THEN FALSE ELSE TRUE  \* The successor retains ownership unless the RED bug drops it.
             /\ UNCHANGED <<fault, verified, ack, reported, ready, inlineCount, attempts, localFailure, peerBlamed>>  \* The original ack and other fields remain unchanged.
Done == /\ phase = "Done" /\ UNCHANGED vars  \* A completed turn can remain idle.
Next == Repair \/ WalkError \/ WalkSuccess \/ FinishSuccess \/ FinishClockError \/ DispatchClock \/ FinishReplyError \/ ResumeReply \/ ObserveReply \/ FinishPushError \/ CancelClock \/ Restart \/ Supersede \/ Done  \* Each step is one of the actions above.
Spec == Init /\ [][Next]_vars  \* Start at Init and allow only these steps.

ReadyVerified == ready => verified  \* A ready event requires a successful inspection.
AckHasOwner == ack => durable \/ successor  \* An acknowledgement has the old or a valid successor owner.
NoInlineRetry == inlineCount = 0  \* A clock fault cannot spin while keeping its permit.
BackoffHasNoPermit == phase = "Backoff" => ~permit  \* Both existing owners release permits during backoff.
ReplyNotAbandonedEarly == (Origin = "Reply" /\ localFailure) => attempts = MaxAttempts  \* The reply owner uses its existing bounded attempts before failure.
LocalFaultNotPeerPenalty == ~peerBlamed  \* Local faults do not cause terminal exhaustion blame or disconnect.
LocalFailureObserved == (phase = "Done" /\ localFailure /\ Origin = "Reply") => reported  \* A still-live background caller explicitly observes its typed result.
ExternalPushErrorReported == (phase = "Done" /\ ~verified /\ Origin = "Push") => reported  \* A new Push's failed walk is surfaced locally.
ReplyHasNoNewLease == Origin = "Reply" => ~durable  \* A reply-driven retry does not invent a durable lease.

FairSpec == Spec /\ WF_vars(Repair) /\ WF_vars(WalkError) /\ WF_vars(WalkSuccess)  \* Assume local storage eventually recovers and walk actions run.
            /\ WF_vars(FinishClockError) /\ WF_vars(DispatchClock) /\ WF_vars(FinishSuccess)  \* Assume enabled clock and finish actions run fairly.
ClockEventuallyReady == (Origin = "Clock") => <>(ready \/ successor)  \* With those assumptions the clock is ready or validly superseded.
ReplyFairSpec == Spec /\ WF_vars(WalkError) /\ WF_vars(WalkSuccess) /\ WF_vars(FinishReplyError)  \* Assume walk and fault handling run fairly, even if storage stays broken.
                 /\ WF_vars(ResumeReply) /\ WF_vars(ObserveReply) /\ WF_vars(FinishSuccess)  \* Assume paced retry, local observation and successful finish run fairly.
ReplyEventuallySettles == (Origin = "Reply") => <>(ready \/ reported)  \* The bounded owner eventually succeeds or its live caller observes failure.
====
