---------------------------- MODULE CommitOutcome ----------------------------
\* The story. A client asks the node to commit a transaction. The storage
\* engine either refuses the commit before it applies anything (a conflict, a
\* stall, a closed handle) or starts to apply it. Once it has started, it may
\* finish, or it may fail part way: an I/O error while the log record is
\* synced. After such a failure the record may or may not be on disk, and
\* nobody can tell until the node reads it back after a restart.
\*
\* The node answers the commit with what it knows: committed, aborted, or
\* indeterminate. It also remembers that answer for a while (the registry's
\* retained outcome), so a client that lost the answer can ask again, and a
\* second commit of the same transaction finds `transaction not found` with
\* the retained outcome attached. The retained entry can expire.
\*
\* What must never happen: the client concludes "aborted" while the write is
\* on disk (it would run it again and apply it twice), or concludes
\* "committed" while it is not. And a refusal is always reported as aborted,
\* so a caller can safely run a refused transaction again.
\*
\* Models crates/engine/storage/src/backends/regolith/transaction.rs
\* (commit_failure, refused_before_apply), crates/engine/storage/src/corekv/
\* errors.rs (Error::at_commit, Error::commit_outcome),
\* crates/database/db/src/txn/owner/mod.rs (Outcome::of),
\* crates/database/db/src/txn/registry/lifecycle.rs (commit_as,
\* commit_unavailable, outcome_as) and
\* crates/surfaces/http/src/handlers/graphql/transactions.rs
\* (commit_error_response).
EXTENDS TLC

\* Each constant is one guard of the design; a RED configuration flips one.
\* ClassifyFailure: a failure after the engine started applying is reported
\*   indeterminate (FALSE: reported aborted, the old behaviour).
\* RetainFailure: the registry retains indeterminate for such a failure
\*   (FALSE: it retains aborted, the old lifecycle).
\* NotFoundSaysUnknown: a commit of a transaction the registry no longer holds
\*   answers aborted only when an abort is retained (FALSE: always aborted).
\* ReportRefusal: a refusal before applying is reported aborted (FALSE:
\*   reported indeterminate, which is safe but useless to the caller).
\* LostReplyUnknown: a client whose write got no answer it can read (a 5xx
\*   with no outcome, a timeout after the request was sent) concludes
\*   indeterminate (FALSE: it concludes aborted and may run it again).
CONSTANTS ClassifyFailure, RetainFailure, NotFoundSaysUnknown, ReportRefusal,
          LostReplyUnknown

\* phase: where the commit is: "open", "applying", "refused" or "done".
\* durable: whether the transaction's writes are on disk.
\* reported: the answer the first commit sent ("none" before it ends).
\* retained: what the registry remembers ("none" before, or after expiry).
\* belief: what the client concluded ("none" until it concludes something).
VARIABLES phase, durable, reported, retained, belief

\* Every variable, for stuttering steps.
vars == <<phase, durable, reported, retained, belief>>

\* The answers a commit can carry, and "none" for no answer yet.
Answers == {"none", "committed", "aborted", "indeterminate"}

\* Nothing has happened: the transaction is open and nothing is on disk.
Init == /\ phase = "open"
        /\ durable = FALSE
        /\ reported = "none"
        /\ retained = "none"
        /\ belief = "none"

\* The engine refuses the commit before applying anything: nothing lands.
\* The answer and the retained outcome both say aborted (or, in the RED
\* configuration that drops ReportRefusal, indeterminate).
Refuse == /\ phase = "open"
          /\ phase' = "refused"
          /\ reported' = IF ReportRefusal THEN "aborted" ELSE "indeterminate"
          /\ retained' = reported'
          /\ UNCHANGED <<durable, belief>>

\* The engine starts to apply the commit: from here on it may land.
Apply == /\ phase = "open"
         /\ phase' = "applying"
         /\ UNCHANGED <<durable, reported, retained, belief>>

\* The engine finishes: the writes are on disk and the answer says so.
Succeed == /\ phase = "applying"
           /\ phase' = "done"
           /\ durable' = TRUE
           /\ reported' = "committed"
           /\ retained' = "committed"
           /\ UNCHANGED belief

\* The engine fails part way: the record may or may not have reached disk,
\* so both futures are explored. The answer and the retained outcome follow
\* the ClassifyFailure and RetainFailure guards.
Fail == /\ phase = "applying"
        /\ phase' = "done"
        /\ durable' \in {TRUE, FALSE}
        /\ reported' = IF ClassifyFailure THEN "indeterminate" ELSE "aborted"
        /\ retained' = IF RetainFailure THEN "indeterminate" ELSE "aborted"
        /\ UNCHANGED belief

\* Whether the commit has ended, with an answer and a retained outcome.
Ended == phase \in {"refused", "done"}

\* The registry forgets the transaction after its idle age.
Expire == /\ Ended
          /\ retained # "none"
          /\ retained' = "none"
          /\ UNCHANGED <<phase, durable, reported, belief>>

\* The client reads the first commit's answer and believes it.
Hear == /\ Ended
        /\ belief' = reported
        /\ UNCHANGED <<phase, durable, reported, retained>>

\* The client lost the answer and asks for the outcome: it learns what is
\* retained, or "unknown" (no conclusion) once the entry expired.
Ask == /\ Ended
       /\ belief' = IF retained = "none" THEN "none" ELSE retained
       /\ UNCHANGED <<phase, durable, reported, retained>>

\* The answer a second commit of the transaction gets: `transaction not
\* found`, aborted only when an abort is retained, indeterminate otherwise
\* (or always aborted, in the RED configuration that drops the guard).
NotFoundAnswer == IF retained = "aborted" \/ ~NotFoundSaysUnknown
                  THEN "aborted"
                  ELSE "indeterminate"

\* The client commits the transaction again (a resend, or a retry after a
\* lost answer) and believes the not-found answer.
Recommit == /\ Ended
            /\ belief' = NotFoundAnswer
            /\ UNCHANGED <<phase, durable, reported, retained>>

\* The answer never reaches the client in a form it can read: a 5xx with no
\* outcome, or a timeout after the request was sent. This can happen at any
\* phase, even before the engine starts, because the server keeps running a
\* request whose wait timed out. The client concludes indeterminate (or, in
\* the RED configuration that drops LostReplyUnknown, aborted). Models
\* crates/entry-points/cli/src/commands/client/http_client/mod.rs
\* (HttpClient::extract_error, HttpClient::send_error).
LoseReply == /\ belief' = IF LostReplyUnknown THEN "indeterminate" ELSE "aborted"
             /\ UNCHANGED <<phase, durable, reported, retained>>

\* Any one step, by the engine, the registry or the client.
Next == Refuse \/ Apply \/ Succeed \/ Fail \/ Expire \/ Hear \/ Ask \/ Recommit
        \/ LoseReply

\* The whole behaviour: start in Init, then take any steps.
Spec == Init /\ [][Next]_vars

\* Every variable keeps a value from its intended set.
TypeOK == /\ phase \in {"open", "applying", "refused", "done"}
          /\ durable \in BOOLEAN
          /\ reported \in Answers
          /\ retained \in Answers
          /\ belief \in Answers

\* A client that concludes "aborted" can run the transaction again safely:
\* nothing it wrote is on disk.
INV_AbortedMeansNothingLanded == belief = "aborted" => ~durable

\* A client that concludes "committed" has its writes on disk.
INV_CommittedMeansLanded == belief = "committed" => durable

\* A refusal before applying is answered aborted, so the caller may retry.
INV_RefusalIsAborted == phase = "refused" => reported = "aborted"
=============================================================================
