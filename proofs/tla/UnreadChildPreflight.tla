---- MODULE UnreadChildPreflight ----
\* A fetch starts by checking whether its root is already complete locally.
\* If that optimization hits a local read fault, an acknowledged clock root
\* returns its reserved permit to the existing paced clock. A reply-driven
\* root with a connected provider continues to its existing bounded fetch owner without charging an
\* attempt for the failed optimization. If storage then recovers, that same
\* reply owner may complete. A disconnected reply instead reports local failure
\* and ends; it has no guaranteed durable retry. This models only preflight.
\*
\* Rust correspondence: crates/p2p/src/sync/coordinator/dag_fetcher.rs
\* poll_fetch_dag, the pre-loop local-completeness check and attempt loop.
EXTENDS Naturals  \* Use ordinary booleans and numbers.
CONSTANTS Origin, Connected, DropReply, HoldClock, SilentUnavailable  \* Choose an owner and the planted bugs.
ASSUME /\ Origin \in {"Clock", "Reply"}  \* Model the two kinds of fetch owner.
       /\ Connected \in BOOLEAN /\ SilentUnavailable \in BOOLEAN /\ DropReply \in BOOLEAN /\ HoldClock \in BOOLEAN  \* Each guard is either present or broken.
VARIABLES phase, fault, attempts, permit, abandoned, ready, durable, reported  \* Track preflight, actual attempts and ownership.
vars == <<phase, fault, attempts, permit, abandoned, ready, durable, reported>>  \* Collect all changing state.
Init == /\ phase = "Preflight" /\ fault = TRUE /\ attempts = 0 /\ ready = FALSE /\ reported = FALSE  \* Start with a transient local fault before any actual attempt.
        /\ permit = (Origin = "Clock") /\ durable = (Origin = "Clock") /\ abandoned = FALSE  \* Only the clock begins with a reserved peer permit and durable owner.
PreflightError == /\ phase = "Preflight" /\ fault  \* The local optimization cannot establish a complete DAG.
                  /\ IF Origin = "Clock"  \* An acknowledged clock task yields to its existing owner.
                       THEN /\ phase' = "Wait" /\ permit' = HoldClock /\ abandoned' = FALSE  \* Return its permit unless the RED bug holds it.
                       ELSE /\ phase' = IF ~Connected THEN "Reported" ELSE IF DropReply THEN "Done" ELSE "Attempt"  \* An unavailable provider ends with a typed local failure.
                            /\ permit' = FALSE /\ abandoned' = (Connected /\ DropReply)  \* No actual attempt or peer permit was spent.
                  /\ reported' = (Origin = "Reply" /\ ~Connected /\ ~SilentUnavailable)  \* The live caller must observe an unavailable-provider local failure.
                  /\ UNCHANGED <<fault, attempts, ready, durable>>  \* The preflight charges no attempt and preserves the durable record.
Repair == /\ phase = "Attempt" /\ fault /\ fault' = FALSE  \* The local storage fault may clear before the actual attempt.
          /\ UNCHANGED <<phase, attempts, permit, abandoned, ready, durable, reported>>  \* Repair changes no owner or result.
Complete == /\ phase = "Attempt" /\ ~fault /\ phase' = "Done"  \* A healthy actual attempt can complete.
            /\ attempts' = attempts + 1 /\ ready' = TRUE  \* Count the actual attempt and emit ready.
            /\ UNCHANGED <<fault, permit, abandoned, durable, reported>>  \* No new durable owner is invented.
Done == /\ phase \in {"Wait", "Done", "Reported"} /\ UNCHANGED vars  \* A settled preflight or finished reply can rest.
Next == PreflightError \/ Repair \/ Complete \/ Done  \* Each step is one of these actions.
Spec == Init /\ [][Next]_vars  \* Begin at Init and take only the specified steps.
ReplyNotDropped == (Origin = "Reply") => ~abandoned  \* An optimization error must not abandon the reply owner.
UnavailableReportsLocal == (phase = "Reported") => reported  \* No connected provider must return an observable local failure.
ClockReleasesPermit == (Origin = "Clock" /\ phase = "Wait") => ~permit  \* An errored clock preflight returns its reserved permit.
ClockKeepsOwner == (Origin = "Clock") => durable  \* A clock preflight fault preserves its existing durable owner.
NoPreflightAttemptCharge == (phase = "Attempt") => attempts = 0  \* The optimization itself never consumes an attempt.
FairSpec == Spec /\ WF_vars(PreflightError) /\ WF_vars(Repair) /\ WF_vars(Complete)  \* Assume the preflight, eventual repair and actual attempt run fairly.
ReplyEventuallyReady == (Origin = "Reply" /\ Connected) => <>ready  \* Under those assumptions the same reply owner completes.
====
