------------------------ MODULE ReindexFailureListing ------------------------
\* The story, in plain words.
\*
\* A schema change owes a collection a reindex: a record with a fresh token.
\* A rebuild then reads the record's token and walks the documents. If a
\* migration gave two documents one value of a unique index, the rebuild
\* stops, and the node writes down on the owed record which index it could not
\* fill and why, so the index listing shows that index as a failed build. If
\* the rebuild finishes, it removes the owed record, and the index answers
\* again. A later schema change may owe the collection a new reindex at any
\* time, also while an older rebuild is still running.
\*
\* Who acts: schema changes (owe a new token), rebuilds (start under the token
\* they read, then stop on a clash or finish), and a reader of the listing.
\*
\* What must never happen:
\*   - the listing shows a failure that no rebuild of the reindex owed now
\*     ever had (a stale failure from an older reindex);
\*   - the listing shows a failure once nothing is owed any more.
\*
\* How the code keeps the promise, three guards:
\*   1. a failure is written only onto the record whose token the stopped
\*      rebuild ran under (owed::record_failure reads owed_under(token));
\*   2. a newly owed reindex starts without a failure (owe_reindex and
\*      owe_full_reindex write failure: None);
\*   3. the failure lives inside the owed record, so the finishing rebuild
\*      that removes the record removes the failure with it
\*      (clear_owed_reindex).
\*
\* Rust code modelled:
\*   crates/database/db/src/definition/migration/reindex/owed.rs
\*     owe_reindex, owe_full_reindex, record_failure, owed_failure,
\*     clear_owed_reindex
\*   crates/database/db/src/definition/migration/reindex/mod.rs
\*     DB::reindex_in_pages (a stopped page records its failure under the
\*     token it ran under)
\*   crates/database/db/src/definition/migration/reindex/failure.rs
\*     DB::record_failure (writes it in a transaction of its own)
\*   crates/database/db/src/database/action.rs index_listing_executions
\*     (the listing shows the owed record's failure as an errored build)
EXTENDS Naturals, FiniteSets

CONSTANTS
  \* How many reindexes schema changes may owe in one behaviour.
  MaxTokens,
  \* TRUE when a stopped rebuild writes its failure whatever token the record
  \* now has (breaks guard 1).
  NoTokenCheck,
  \* TRUE when a newly owed reindex keeps the failure the record had (breaks
  \* guard 2).
  KeepFailureOnOwe,
  \* TRUE when the failure is kept beside the owed record, so removing the
  \* record leaves it (breaks guard 3).
  FailureBeside

\* The constants have the shapes they should.
ASSUME /\ MaxTokens \in Nat /\ MaxTokens > 0
       /\ NoTokenCheck \in BOOLEAN
       /\ KeepFailureOnOwe \in BOOLEAN
       /\ FailureBeside \in BOOLEAN

\* The tokens a schema change can hand out, one per owed reindex.
Tokens == 1..MaxTokens

\* 0 stands for "nothing is owed".
None == 0

VARIABLES
  \* The token of the owed record, or None when nothing is owed.
  owed,
  \* Whether the owed record (or, with FailureBeside, the record beside it)
  \* holds a failure.
  failure,
  \* How many tokens have been handed out so far.
  issued,
  \* The tokens of the rebuilds that are running now, each read when it began.
  running,
  \* Ghost: the tokens under which some rebuild stopped on a clash.
  failedUnder

vars == <<owed, failure, issued, running, failedUnder>>

\* Every variable holds a value of the kind it should.
TypeOK ==
  /\ owed \in Tokens \cup {None}
  /\ failure \in BOOLEAN
  /\ issued \in 0..MaxTokens
  /\ running \subseteq Tokens
  /\ failedUnder \subseteq Tokens

\* At the start nothing is owed, nothing failed, nothing runs.
Init ==
  /\ owed = None
  /\ failure = FALSE
  /\ issued = 0
  /\ running = {}
  /\ failedUnder = {}

\* A schema change owes a new reindex under a fresh token. The new record
\* starts without a failure, unless guard 2 is broken.
Owe ==
  /\ issued < MaxTokens
  /\ issued' = issued + 1
  /\ owed' = issued + 1
  /\ failure' = IF KeepFailureOnOwe THEN failure ELSE FALSE
  /\ UNCHANGED <<running, failedUnder>>

\* A rebuild begins under the token the owed record has now.
Begin ==
  /\ owed # None
  /\ owed \notin running
  /\ running' = running \cup {owed}
  /\ UNCHANGED <<owed, failure, issued, failedUnder>>

\* A rebuild under token t stops on a unique clash. It writes the failure only
\* if the record still has token t, unless guard 1 is broken.
Stop(t) ==
  /\ t \in running
  /\ running' = running \ {t}
  /\ failedUnder' = failedUnder \cup {t}
  \* The record it writes onto still has its token (or the check is skipped).
  /\ failure' = IF (owed = t \/ (NoTokenCheck /\ owed # None))
                  THEN TRUE ELSE failure
  /\ UNCHANGED <<owed, issued>>

\* A rebuild under token t finishes. It removes the owed record only if the
\* record still has token t; the failure goes with the record, unless it was
\* kept beside it (guard 3 broken).
Finish(t) ==
  /\ t \in running
  /\ running' = running \ {t}
  /\ IF owed = t
       THEN /\ owed' = None
            /\ failure' = IF FailureBeside THEN failure ELSE FALSE
       ELSE UNCHANGED <<owed, failure>>
  /\ UNCHANGED <<issued, failedUnder>>

\* Any one of the actions above happens next.
Next ==
  \/ Owe
  \/ Begin
  \/ \E t \in Tokens : Stop(t)
  \/ \E t \in Tokens : Finish(t)

\* The whole behaviour: start, then take steps.
Spec == Init /\ [][Next]_vars

\* What the listing shows: a failed build exactly when a failure is recorded
\* and, with the failure inside the record, a record is there to hold it.
ListedFailed == failure /\ (FailureBeside \/ owed # None)

\* Promise: a listed failure belongs to the reindex owed now: some rebuild
\* under its token stopped on a clash.
INV_FailureIsCurrent == ListedFailed => (owed # None /\ owed \in failedUnder)

\* Promise: once nothing is owed, no index lists as failed.
INV_SettledNotFailed == owed = None => ~ListedFailed

=============================================================================
