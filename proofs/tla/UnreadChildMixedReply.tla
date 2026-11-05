---- MODULE UnreadChildMixedReply ----
\* A reply-driven fetch owns at most three attempts. An attempt may fail due to
\* a local walk fault or a network fault. When all attempts fail, remembering
\* any local fault returns a local failure rather than blaming a provider
\* after fewer than three genuine network failures. A successful later attempt
\* can still complete. This model abstracts actual network and storage work.
\* peerPenalized tracks only terminal exhaustion/disconnect or terminal
\* Unservable blame. Per-window provider health updates and cooling are
\* outside this model and may occur after a real Busy or fast failure.
\*
\* Rust correspondence: crates/p2p/src/sync/coordinator/dag_fetcher.rs
\* poll_fetch_dag and FetchAttemptOutcome; the latch is per reply-owned task.
EXTENDS Naturals  \* Use ordinary numbers and booleans.
CONSTANTS RememberLocal, BrokenEarly  \* Select the latch and any early-exit mutant.
ASSUME /\ RememberLocal \in BOOLEAN /\ BrokenEarly \in {"None", "Unservable", "Deferred", "ProviderBusy"}  \* Select exactly one planted early-exit bug.
MaxAttempts == 3  \* The existing reply owner has three total attempts.
VARIABLES phase, attempt, sawLocal, everLocal, peerPenalized, result, permit  \* Track the bounded owner, faults, outcome and permit.
vars == <<phase, attempt, sawLocal, everLocal, peerPenalized, result, permit>>  \* Collect every changing field.
Init == /\ phase = "Attempt" /\ attempt = 1 /\ sawLocal = FALSE /\ everLocal = FALSE  \* Begin the first turn with no recorded local fault.
        /\ peerPenalized = FALSE /\ result = "None" /\ permit = TRUE  \* The current attempt holds a permit and has not ended.
LocalFault == /\ phase = "Attempt" /\ phase' = "After"  \* A local read fault ends this attempt.
              /\ sawLocal' = TRUE /\ everLocal' = TRUE /\ permit' = FALSE  \* Remember the fault and release the permit.
              /\ UNCHANGED <<attempt, peerPenalized, result>>  \* This fault does not itself cause terminal blame or disconnect.
NetworkFault == /\ phase = "Attempt" /\ phase' = "After"  \* A network failure ends this attempt.
                /\ sawLocal' = IF RememberLocal THEN sawLocal ELSE FALSE  \* Only the RED bug forgets an earlier local fault.
                /\ permit' = FALSE  \* Release the attempt permit before any backoff.
                /\ UNCHANGED <<attempt, everLocal, peerPenalized, result>>  \* A single attempt does not itself finalize exhaustion.
Retry == /\ phase = "After" /\ attempt < MaxAttempts  \* Existing bounded attempts remain after backoff.
         /\ phase' = "Attempt" /\ attempt' = attempt + 1 /\ permit' = TRUE  \* The same owner takes another permit for one turn.
         /\ UNCHANGED <<sawLocal, everLocal, peerPenalized, result>>  \* The local-fault history travels with this task.
Success == /\ phase = "Attempt" /\ phase' = "Done" /\ result' = "Success" /\ permit' = FALSE  \* A later success completes and releases the permit.
           /\ UNCHANGED <<attempt, sawLocal, everLocal, peerPenalized>>  \* Success does not cause terminal blame or disconnect.
Finish == /\ phase = "After" /\ attempt = MaxAttempts /\ phase' = "Done"  \* The bounded owner now ends after all three attempts.
          /\ result' = IF sawLocal THEN "LocalFailure" ELSE "NetworkFailure"  \* A remembered local fault takes precedence at exhaustion.
          /\ peerPenalized' = ~sawLocal  \* Terminal exhaustion can blame a peer only without a shortening local fault.
          /\ UNCHANGED <<attempt, sawLocal, everLocal, permit>>  \* Final classification does not reacquire a permit.
EarlyExit(kind) == /\ phase = "Attempt" /\ kind \in {"Unservable", "Deferred", "ProviderBusy"}  \* A provider or contention outcome can end a reply turn early.
                   /\ phase' = "Done" /\ permit' = FALSE  \* The ending attempt releases its permit.
                   /\ result' = IF sawLocal /\ BrokenEarly # kind THEN "LocalFailure" ELSE kind  \* Preserve a previous local fault across every non-success exit.
                   /\ peerPenalized' = (kind = "Unservable" /\ (~sawLocal \/ BrokenEarly = kind))  \* The mutant applies terminal blame despite the shortened network budget.
                   /\ UNCHANGED <<attempt, sawLocal, everLocal>>  \* Early termination retains the fault history.
Done == /\ phase = "Done" /\ UNCHANGED vars  \* A finished task may rest.
Next == LocalFault \/ NetworkFault \/ Retry \/ Success \/ Finish \/ (\E kind \in {"Unservable", "Deferred", "ProviderBusy"}: EarlyExit(kind)) \/ Done  \* A step is one of the bounded attempt actions.
Spec == Init /\ [][Next]_vars  \* Begin at Init and allow only those steps.
NoPenaltyAfterLocal == everLocal => ~peerPenalized  \* A local fault cannot lead to terminal blame or disconnect.
NoNetworkExhaustionAfterLocal == (phase = "Done" /\ everLocal) => result # "NetworkFailure"  \* Mixed faults are not labelled pure network exhaustion.
LocalFailureOnEarlyExit == (phase = "Done" /\ everLocal /\ result \in {"Unservable", "Deferred", "ProviderBusy"}) => FALSE  \* Every early non-success after a local fault must return LocalFailure.
BackoffReleasesPermit == phase = "After" => ~permit  \* The task does not hold its permit while waiting.
====
