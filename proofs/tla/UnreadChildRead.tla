---- MODULE UnreadChildRead ----
\* A parent names an unmerged child. The blockstore first says that child
\* exists, then the child read can return bytes, absence, or an error. The
\* walker and one of several callers decide whether to publish ready work,
\* request missing data, retain an obligation, or report an error.
\*
\* A complete result must mean that the child was actually read; a child that
\* disappears after a positive has is an inconsistent observation, while an
\* error is unknown rather than complete or proven absent. A clock-owned root
\* that had already been acknowledged
\* keeps its durable recovery obligation when the walk cannot complete.
\*
\* This finite model abstracts a single unmerged child and one walk. It is
\* not a model of durable storage transactions, mutable DAGs, a complete
\* scheduler, every peer, or a proof of the Rust implementation. An
\* `is_merged` error followed by traversal is existing behavior and is not
\* changed or modeled here; the walk begins after that conservative fallback.
\*
\* Rust correspondence: crates/p2p/src/sync/manager/links.rs
\* find_all_missing_links; coordinator/dag_fetcher.rs poll_fetch_dag and
\* fetch_dag_attempt; coordinator/event_handler/doc_sync.rs; branchable_sync.rs;
\* manager/process/pushlog.rs and pending_dag.rs; event_handler/car.rs; and
\* replication/handlers.rs dag_is_local.
EXTENDS Naturals  \* Use ordinary booleans and numbers.

CONSTANTS ReadOutcome, WalkerMode, ConsumerMode, Route, ContradictionMode, HasErrorMode, ReplyRecoveryMode  \* Choose one outcome, caller, and planted guard settings.
ASSUME /\ ReadOutcome \in {"Bytes", "None", "Error", "ParseError", "HasFalse", "HasError"}  \* The child observation must be one of these six outcomes.
       /\ WalkerMode \in {"Checked", "Skip"}  \* The walker either checks reads or skips a fault.
       /\ ConsumerMode \in {"Checked", "Swallow"}  \* The caller either handles errors or swallows them.
       /\ ContradictionMode \in {"Error", "Missing"}  \* A contradictory read is either an error or misclassified as missing.
       /\ HasErrorMode \in {"Error", "Missing"}  \* A failed has is either an error or misclassified as missing.
       /\ ReplyRecoveryMode \in {"Keep", "Drop"}  \* DocSync retains its existing fetch owner unless the bug drops it.
       /\ Route \in {"Initial", "Rooted", "Final", "DocSync", "Branchable", "Push", "Car", "Resync", "Pending"}  \* These labels cover the callers at the changed boundary.

\* phase is the walk program counter; outcome is what it told its caller.
\* inspected is evidence that the unmerged child was successfully read.
\* obligation records a clock-owned durable root that was acknowledged earlier.
VARIABLES phase, outcome, inspected, obligation, ack, decision  \* Remember the walk, its evidence, its owner and its decision.
vars == <<phase, outcome, inspected, obligation, ack, decision>>  \* Collect all fields that a step may update.

\* A positive `has` and following get are independent; the separate HasFalse
\* case models ordinary confirmed absence without issuing that get.
Init == /\ phase = "Read" /\ outcome = "Unknown" /\ inspected = FALSE  \* Start before reading, with no evidence or outcome.
        /\ obligation = (Route \in {"Initial", "Rooted", "Final", "Resync", "Pending"})  \* These route labels represent an already-acknowledged clock root.
        /\ ack = obligation /\ decision = "None"  \* The original ack is backed by its record; no decision exists yet.

\* The child read returned bytes; model the leaf as having no more links.
ReadBytes == /\ phase = "Read" /\ ReadOutcome = "Bytes"  \* The read stage receives parseable child bytes.
             /\ phase' = "Decide" /\ inspected' = TRUE /\ outcome' = "Complete"  \* Mark the child inspected and the frontier complete.
             /\ UNCHANGED <<obligation, ack, decision>>  \* No owner or acknowledgement changes.

\* A negative has is an ordinary missing CID and may be fetched.
HasFalse == /\ phase = "Read" /\ ReadOutcome = "HasFalse"  \* The existence check reports an absent child.
            /\ phase' = "Decide" /\ outcome' = "Missing"  \* Return an ordinary missing frontier to the caller.
            /\ UNCHANGED <<inspected, obligation, ack, decision>>  \* No child was read and no owner changes.

\* A failed existence check is a storage error, not proof that a CID is absent.
HasError == /\ phase = "Read" /\ ReadOutcome = "HasError"  \* The existence check instead fails.
            /\ phase' = "Decide"  \* The walker now returns to its caller.
            /\ outcome' = IF HasErrorMode = "Error" THEN "Error" ELSE "Missing"  \* Preserve the error unless the RED guard is removed.
            /\ UNCHANGED <<inspected, obligation, ack, decision>>  \* No owner or acknowledgement changes.

\* The child disappeared or was not returned after has=true. The conservative
\* walker reports this contradiction separately; the old branch ignores it.
ReadNone == /\ phase = "Read" /\ ReadOutcome = "None"  \* A positive has is followed by an empty get.
            /\ phase' = "Decide"  \* The walker now returns to its caller.
            /\ outcome' = IF WalkerMode = "Skip" THEN "Complete"  \* The old bug skips the child and reports complete.
                           ELSE IF ContradictionMode = "Error" THEN "Contradiction" ELSE "Missing"  \* Otherwise return a contradiction unless the RED mode misclassifies it.
            /\ UNCHANGED <<inspected, obligation, ack, decision>>  \* No owner or acknowledgement changes.

\* A storage error does not establish absence. The corrected walker returns
\* an error; the old branch silently omits the child from the frontier.
ReadError == /\ phase = "Read" /\ ReadOutcome = "Error"  \* The child read returns a storage error.
             /\ phase' = "Decide"  \* The walker now returns to its caller.
             /\ outcome' = IF WalkerMode = "Checked" THEN "Error" ELSE "Complete"  \* Return the error unless the RED walker skips it.
             /\ UNCHANGED <<inspected, obligation, ack, decision>>  \* No owner or acknowledgement changes.

\* Bytes that cannot be parsed cannot prove a complete linked subtree either.
ParseError == /\ phase = "Read" /\ ReadOutcome = "ParseError"  \* The child bytes cannot be parsed.
              /\ phase' = "Decide" /\ outcome' = "Error"  \* Return an error, never a complete frontier.
              /\ UNCHANGED <<inspected, obligation, ack, decision>>  \* No owner or acknowledgement changes.

\* A caller may publish readiness only when its walk reports complete.
\* The planted bug also treats an error as if it reported an empty frontier.
Complete == /\ phase = "Decide"  \* A caller is deciding how the walk ended.
            /\ (outcome = "Complete" \/ (outcome \in {"Error", "Contradiction"} /\ ConsumerMode = "Swallow"))  \* Complete is allowed; a RED caller also swallows a fault.
            /\ phase' = "Done" /\ decision' = "Ready"  \* The caller publishes ready and finishes.
            /\ UNCHANGED <<outcome, inspected, obligation, ack>>  \* The earlier read evidence and owner do not change.

\* A confirmed absent child is eligible for fetch, not a completed DAG.
FetchMissing == /\ phase = "Decide" /\ outcome = "Missing"  \* The caller sees an ordinary missing child.
                /\ phase' = "Done" /\ decision' = "Fetch"  \* It schedules a fetch rather than publishing ready.
                /\ UNCHANGED <<outcome, inspected, obligation, ack>>  \* The earlier read evidence and owner do not change.

\* An error is deferred or returned to the caller. Push must not acknowledge
\* success here; already acknowledged clock roots remain durably owed.
DeferError == /\ phase = "Decide" /\ outcome \in {"Error", "Contradiction"} /\ ConsumerMode = "Checked"  \* A checked caller sees a local or contradictory-read fault.
              /\ phase' = "Done"  \* This local walk ends without a ready event.
              /\ decision' = IF Route = "DocSync" /\ ReplyRecoveryMode = "Keep" THEN "ReportAndRetry" ELSE "Error"  \* DocSync reports locally and retains its existing fetch owner.
              /\ UNCHANGED <<outcome, inspected, obligation, ack>>  \* The earlier read evidence and owner do not change.

\* After a decision, remain idle for invariant checking.
Done == /\ phase = "Done" /\ UNCHANGED vars  \* Once the decision is made, this one-turn model rests.
Next == ReadBytes \/ HasFalse \/ HasError \/ ReadNone \/ ReadError \/ ParseError \/ Complete \/ FetchMissing \/ DeferError \/ Done  \* A step is one listed read, decision, or idle action.
Spec == Init /\ [][Next]_vars  \* Start at Init and allow only these steps.

\* No caller publishes ready work based on an unread, unmerged child.
ReadyRequiresRead == decision = "Ready" => inspected  \* A ready event requires a successful child inspection.
\* An underlying storage read error is not represented as a missing CID.
ErrorIsNotAbsence == ReadOutcome \in {"None", "Error", "ParseError", "HasError"} => decision # "Fetch"  \* A local read or has error is never treated as confirmed absence.
\* A pre-existing acknowledged clock obligation survives this one turn.
AcknowledgedStillOwed == ack => obligation  \* An earlier acknowledgement keeps its durable owner in this turn.
\* The Push caller must not acquire an acknowledgement by ignoring failure.
NoNewAckOnError == (Route = "Push" /\ ReadOutcome \in {"None", "Error", "ParseError", "HasError"}) => ~ack  \* A failed new Push does not claim success.
\* A local DocSync walk failure preserves its existing reply-driven fetch turn.
DocSyncKeepsOwner == (Route = "DocSync" /\ ReadOutcome \in {"None", "Error", "ParseError", "HasError"} /\ phase = "Done") => decision = "ReportAndRetry"  \* The handler reports the fault and schedules that root.
====
