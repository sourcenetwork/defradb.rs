--------------------------- MODULE StrictAccessRead ---------------------------
\* The story, in plain words.
\*
\* A caller reads documents of a collection under a document access policy.
\* Every row the scan finds is checked: may this caller read it? The answer
\* comes from the document ACP, which on a remote provider (vera.rs) can fail
\* to come: the provider is unreachable, its verified revision is stale, it
\* is busy, or its proof deadline passed. Those failures are late, not wrong:
\* the provider's own waits (provider/verified.rs: busy, overtaken, stale)
\* run first, and only a check still unanswered after them is a failure here.
\* A check can also break for another reason (a storage error, a malformed
\* answer); that is no answer either.
\*
\* By default a row whose check failed is dropped and the scan goes on (fail
\* closed): the caller sees fewer rows than it may, and no error. That stays
\* as it is, for existing callers and Go parity.
\*
\* A caller that will decide on what it reads asks for a strict read
\* (RequestContext::with_strict_access_reads). A strict read:
\*   - drops a row the provider denies, exactly as the default does: a denial
\*     is an answer, never an error, so an error never tells a caller that a
\*     document it may not read exists because it was denied;
\*   - fails the whole request with "access check unavailable" when a check
\*     could not be answered, returning no rows: retryable when the provider
\*     was late, not retryable when the check broke. The error names nothing
\*     about the document; it says only that some row's check had no answer.
\*     That the scan found a candidate row is all it can reveal, and it is
\*     the price of never answering partially;
\*   - never returns a row the caller may not read.
\* The rows a strict read kept reach the caller only when it answers: a read
\* that fails hands back none. The same rule holds wherever a strict
\* request checks access, not only in the permission filter: a mutation's
\* authorization, a cid read, commit history and the edge answering a
\* prepared access input all fail it with "access check unavailable".
\* In a transaction, a strict read that failed wrote nothing: the request
\* fails and the transaction stays usable.
\*
\* What must never happen:
\*   - any read returns a row the caller may not read;
\*   - a strict read answers with fewer rows than the caller may read;
\*   - a failed strict read hands back any row;
\*   - a strict read fails on a row the caller is simply denied;
\*   - a strict read fails before the provider's waits had their turn;
\*   - the error names the document whose check failed;
\*   - a failed strict read leaves its transaction unusable.
\*
\* Rust owners: crates/engine/query/src/plan/permission_filter.rs
\* AcpReadCheck::allows; crates/engine/query/src/runner/strict_access.rs
\* access_check_failure; crates/foundation/defra-core/src/request_context.rs
\* RequestContext::strict_access_reads; crates/engine/query/src/error.rs
\* QueryError::AccessCheckUnavailable; crates/engine/vera/src/vera_rs/provider/verified.rs
\* verified; crates/engine/vera/src/vera_rs/provider/reach.rs read_error.
EXTENDS Naturals, FiniteSets

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

\* The rows the scan finds.
Docs == {"d1", "d2"}

\* What a check can come to: an answer (allow or deny), late (the provider
\* did not answer in time) or broken (no answer for another reason).
Outcomes == {"allow", "deny", "late", "broken"}

\* Every state variable, with what it stands for.
VARIABLES
  strict,     \* TRUE when the caller asked for a strict read
  readable,   \* per row: TRUE when the caller may read it
  check,      \* per row: what its check came to, "pending" before it ran
  waited,     \* per row: TRUE once a late check had the provider's waits
  scanned,    \* the rows the scan has decided
  kept,       \* the rows the scan kept, held until the read answers
  returned,   \* the rows the read hands back
  outcome,    \* "running", "answered" or "failed"
  retryable,  \* TRUE when the failure was a late provider
  told,       \* the rows the error names
  txnUsable   \* TRUE while the transaction the read ran in can take requests

\* The tuple of all variables, for stuttering steps.
vars == <<strict, readable, check, waited, scanned, kept, returned, outcome, retryable, told, txnUsable>>

\* The type of every variable.
TypeOK ==
  \* Strict or not.
  /\ strict \in BOOLEAN
  \* Readability per row.
  /\ readable \in [Docs -> BOOLEAN]
  \* A check per row, pending or an outcome.
  /\ check \in [Docs -> Outcomes \cup {"pending"}]
  \* Whether each row's check waited.
  /\ waited \in [Docs -> BOOLEAN]
  \* Decided rows are rows.
  /\ scanned \subseteq Docs
  \* Kept rows are rows.
  /\ kept \subseteq Docs
  \* Returned rows are rows.
  /\ returned \subseteq Docs
  \* The read is running, answered or failed.
  /\ outcome \in {"running", "answered", "failed"}
  \* The failure is retryable or not.
  /\ retryable \in BOOLEAN
  \* The rows the error names.
  /\ told \subseteq Docs
  \* The transaction is usable or not.
  /\ txnUsable \in BOOLEAN

\* Any mode, any readability, nothing checked yet.
Init ==
  \* The caller chose a mode.
  /\ strict \in BOOLEAN
  \* Each row is readable to the caller or not.
  /\ readable \in [Docs -> BOOLEAN]
  \* No check ran.
  /\ check = [d \in Docs |-> "pending"]
  \* No check waited.
  /\ waited = [d \in Docs |-> FALSE]
  \* Nothing decided.
  /\ scanned = {}
  \* Nothing kept.
  /\ kept = {}
  \* Nothing returned.
  /\ returned = {}
  \* The read runs.
  /\ outcome = "running"
  \* No failure.
  /\ retryable = FALSE
  \* Nothing named.
  /\ told = {}
  \* The transaction takes requests.
  /\ txnUsable = TRUE

\* What the provider can say about row `d`: the true answer, or no answer.
Said(d) == {IF readable[d] THEN "allow" ELSE "deny", "late", "broken"}

\* The check of a row runs. A late check has the provider's waits; with the
\* no-wait bug it is taken as final at once.
Ask(d) ==
  \* Only while the read runs and the row was not asked.
  /\ outcome = "running"
  /\ check[d] = "pending"
  \* The provider says something.
  /\ \E said \in Said(d): check' = [check EXCEPT ![d] = said]
  \* A late answer had its waits, unless the bug skips them.
  /\ waited' = [waited EXCEPT ![d] = (check'[d] = "late" /\ Bug # "no_wait")]
  \* Nothing else changes.
  /\ UNCHANGED <<strict, readable, scanned, kept, returned, outcome, retryable, told, txnUsable>>

\* After its waits a late check may still get its answer.
Answered(d) ==
  \* Only a late check that waited, while the read runs and the row is open.
  /\ outcome = "running"
  /\ check[d] = "late"
  /\ waited[d]
  /\ d \notin scanned
  \* The provider's true answer arrives.
  /\ check' = [check EXCEPT ![d] = IF readable[d] THEN "allow" ELSE "deny"]
  \* Nothing else changes.
  /\ UNCHANGED <<strict, readable, waited, scanned, kept, returned, outcome, retryable, told, txnUsable>>

\* Whether a check came to no answer.
Unanswered(d) == check[d] \in {"late", "broken"}

\* The scan decides a row on its check.
Decide(d) ==
  \* Only while the read runs, for a checked row not yet decided.
  /\ outcome = "running"
  /\ check[d] # "pending"
  /\ d \notin scanned
  \* The row is decided.
  /\ scanned' = scanned \cup {d}
  \* An allow keeps it (and with the reveal bug, so does no answer).
  /\ kept' = IF check[d] = "allow" \/ (Unanswered(d) /\ Bug = "reveal")
               THEN kept \cup {d} ELSE kept
  \* Kept rows reach the caller only when the read answers; with the bug they
  \* are handed out as they are decided.
  /\ returned' = IF Bug = "publish_early" THEN kept' ELSE returned
  \* A strict read fails on no answer, or, with the bug, on a denial too;
  \* with the partial bug it drops the row like the default.
  /\ IF strict /\ Bug # "partial" /\ (Unanswered(d) \/ (check[d] = "deny" /\ Bug = "denial_errors"))
       THEN /\ outcome' = "failed"
            /\ retryable' = (check[d] = "late")
            /\ told' = IF Bug = "names_row" THEN {d} ELSE {}
       ELSE UNCHANGED <<outcome, retryable, told>>
  \* Nothing else changes.
  /\ UNCHANGED <<strict, readable, check, waited, txnUsable>>

\* Every row decided and none failed the read: it answers.
Finish ==
  \* Only a running read with every row decided.
  /\ outcome = "running"
  /\ scanned = Docs
  \* It answers.
  /\ outcome' = "answered"
  \* The rows it kept reach the caller.
  /\ returned' = kept
  \* Nothing else changes.
  /\ UNCHANGED <<strict, readable, check, waited, scanned, kept, retryable, told, txnUsable>>

\* A failed read wrote nothing; with the bug it leaves its transaction broken.
Fail ==
  \* Only after a failure, once.
  /\ outcome = "failed"
  /\ txnUsable
  /\ Bug = "txn_broken"
  \* The bug ends the transaction.
  /\ txnUsable' = FALSE
  \* Nothing else changes.
  /\ UNCHANGED <<strict, readable, check, waited, scanned, kept, returned, outcome, retryable, told>>

\* Every step the read can take.
Next == Fail \/ Finish \/ \E d \in Docs: Ask(d) \/ Answered(d) \/ Decide(d)

\* From the initial state, any sequence of steps.
Spec == Init /\ [][Next]_vars

\* No read ever returns a row the caller may not read.
INV_NeverRevealed == \A d \in returned: readable[d]

\* A strict read that answers holds every row the caller may read.
INV_StrictComplete ==
  (strict /\ outcome = "answered") => returned = {d \in Docs: readable[d]}

\* A strict read fails only on a row whose check had no answer, never on one
\* the caller is denied.
INV_DenialNeverErrors ==
  outcome = "failed" => \E d \in scanned: Unanswered(d)

\* A late check is final only after the provider's waits.
INV_LateWaited == \A d \in scanned: check[d] = "late" => waited[d]

\* The error names no row.
INV_NamesNothing == told = {}

\* A failed read leaves its transaction usable.
INV_TxnUsable == txnUsable

\* A failed read hands back no row: the request answers with its error alone.
INV_FailedReturnsNothing == outcome = "failed" => returned = {}

\* By default no read fails on a check: it fails closed and keeps scanning.
INV_DefaultNeverFails == ~strict => outcome # "failed"

\* As a property the model must break: a default read that answers holds
\* every row the caller may read. It does not, by design (DefaultPartialWitness).
DefaultComplete ==
  (~strict /\ outcome = "answered") => returned = {d \in Docs: readable[d]}
=============================================================================
