------------------------ MODULE ReindexFailureListing ------------------------
\* The story, in plain words.
\*
\* A schema change owes a collection a reindex: a record with a fresh token.
\* A rebuild then reads the record's token, clears the indexes and walks the
\* documents. If a migration gave two documents one value of a unique index,
\* the rebuild stops, and the node writes down on the owed record which index
\* it could not fill and why, so the index listing shows that index as a
\* failed build. Like Go, the node then abandons that index: writers stop
\* maintaining it, so its leftover entries refuse no write. A rebuild may try
\* the same reindex again later (at open, or after another schema change); it
\* starts over from the clear and removes the failure first, so writers
\* maintain the index again while it walks. If the rebuild finishes, it
\* removes the owed record, and the index answers again. A later schema
\* change may owe the collection a new reindex at any time, also while an
\* older rebuild is still running.
\*
\* Who acts: schema changes (owe a new token), rebuilds (start under the token
\* they read, clear, then stop on a clash or finish), writers (write a
\* document, maintaining the index unless it is abandoned), and a reader of
\* the listing.
\*
\* What must never happen:
\*   - the listing shows a failure that no rebuild of the reindex owed now
\*     ever had (a stale failure from an older reindex);
\*   - the listing shows a failure once nothing is owed any more;
\*   - an index that lists as failed refuses a write;
\*   - an index is settled (nothing owed) while it holds entries that do not
\*     match the documents.
\*
\* How the code keeps the promise, six guards:
\*   1. a failure is written only onto the record whose token the stopped
\*      rebuild ran under (owed::record_failure reads owed_under(token));
\*   2. a newly owed reindex starts without a failure (owe_reindex and
\*      owe_full_reindex write failure: None);
\*   3. a finishing rebuild removes the owed record only if it still has the
\*      rebuild's token (clear_owed_reindex reads owes(token));
\*   4. recording a failure drops the rebuild's progress, so a rebuild that
\*      tries again clears first instead of resuming past documents written
\*      while the index was abandoned (record_failure calls progress::clear);
\*   5. the first page of a rebuild that tries again removes the failure, so
\*      writers maintain the index while it walks (owed::retry_failed);
\*   6. writers do not maintain the index the failure names
\*      (IndexStatuses::takes_writes, Collection::with_reindex_owed).
\*
\* Rust code modelled:
\*   crates/database/db/src/definition/migration/reindex/owed.rs
\*     owe_reindex, owe_full_reindex, record_failure, retry_failed,
\*     owed_status, owed_failure, clear_owed_reindex
\*   crates/database/db/src/definition/migration/reindex/page.rs
\*     ReindexPage::step (retries a failed rebuild, rebuilds rebuilt_indexes)
\*   crates/database/db/src/definition/migration/reindex/mod.rs
\*     DB::reindex_in_pages (a stopped page records its failure under the
\*     token it ran under)
\*   crates/database/db/src/definition/migration/reindex/failure.rs
\*     DB::record_failure (writes it in a transaction of its own)
\*   crates/database/db/src/collection/index_status.rs
\*     IndexStatuses::takes_writes, IndexStatuses::rebuilds
\*   crates/database/db/src/collection/mod.rs Collection::with_reindex_owed,
\*     Collection::with_current_index_actions, Collection::write_indexes,
\*     Collection::rebuilt_indexes
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
  \* TRUE when a finishing rebuild removes the owed record whatever token it
  \* now has (breaks guard 3).
  NoFinishTokenCheck,
  \* TRUE when a stopped rebuild keeps its progress, so a retry resumes
  \* instead of clearing (breaks guard 4).
  KeepProgressOnStop,
  \* TRUE when a retry leaves the failure in place while it walks (breaks
  \* guard 5).
  KeepFailureOnRetry,
  \* TRUE when writers keep maintaining the failed index (breaks guard 6).
  WritersIgnoreFailure

\* The constants have the shapes they should.
ASSUME /\ MaxTokens \in Nat /\ MaxTokens > 0
       /\ NoTokenCheck \in BOOLEAN
       /\ KeepFailureOnOwe \in BOOLEAN
       /\ NoFinishTokenCheck \in BOOLEAN
       /\ KeepProgressOnStop \in BOOLEAN
       /\ KeepFailureOnRetry \in BOOLEAN
       /\ WritersIgnoreFailure \in BOOLEAN

\* The tokens a schema change can hand out, one per owed reindex.
Tokens == 1..MaxTokens

\* 0 stands for "nothing is owed" and for "no progress".
None == 0

VARIABLES
  \* The token of the owed record, or None when nothing is owed.
  owed,
  \* Whether the owed record holds a failure.
  failure,
  \* How many tokens have been handed out so far.
  issued,
  \* The tokens of the rebuilds that are running now, each read when it began.
  running,
  \* The token whose rebuild has cleared the index and is walking: the
  \* progress record. None when no rebuild has cleared since it was dropped.
  cleared,
  \* Whether the index holds entries that do not match the documents (an
  \* older version's, a stopped rebuild's, or a write's the index missed).
  stale,
  \* Ghost: the tokens under which some rebuild stopped on a clash.
  failedUnder,
  \* Ghost: whether a write was refused while the index listed as failed.
  refusedWhileFailed

vars == <<owed, failure, issued, running, cleared, stale, failedUnder,
          refusedWhileFailed>>

\* Every variable holds a value of the kind it should.
TypeOK ==
  /\ owed \in Tokens \cup {None}
  /\ failure \in BOOLEAN
  /\ issued \in 0..MaxTokens
  /\ running \subseteq Tokens
  /\ cleared \in Tokens \cup {None}
  /\ stale \in BOOLEAN
  /\ failedUnder \subseteq Tokens
  /\ refusedWhileFailed \in BOOLEAN

\* What the listing shows: a failed build exactly when a failure is recorded.
\* It lives on the owed record, so removing the record removes it.
ListedFailed == failure

\* Whether writers maintain the index: not while it lists as failed, unless
\* guard 6 is broken.
Maintained == WritersIgnoreFailure \/ ~ListedFailed

\* At the start nothing is owed, nothing failed, nothing runs, and the index
\* matches the documents.
Init ==
  /\ owed = None
  /\ failure = FALSE
  /\ issued = 0
  /\ running = {}
  /\ cleared = None
  /\ stale = FALSE
  /\ failedUnder = {}
  /\ refusedWhileFailed = FALSE

\* A schema change owes a new reindex under a fresh token. The new record
\* starts without a failure, unless guard 2 is broken. The entries now
\* describe the documents at an older version, so they are stale.
Owe ==
  /\ issued < MaxTokens
  /\ issued' = issued + 1
  /\ owed' = issued + 1
  /\ failure' = IF KeepFailureOnOwe THEN failure ELSE FALSE
  /\ stale' = TRUE
  /\ UNCHANGED <<running, cleared, failedUnder, refusedWhileFailed>>

\* A rebuild begins under the token the owed record has now.
Begin ==
  /\ owed # None
  /\ owed \notin running
  /\ running' = running \cup {owed}
  /\ UNCHANGED <<owed, failure, issued, cleared, stale, failedUnder,
                 refusedWhileFailed>>

\* A rebuild under token t, with no progress of its own, starts from the
\* clear: its first page removes the failure (unless guard 5 is broken) and
\* the clear removes every entry, so nothing is stale any more.
Clear(t) ==
  /\ t \in running
  /\ owed = t
  /\ cleared # t
  /\ cleared' = t
  /\ stale' = FALSE
  /\ failure' = IF KeepFailureOnRetry THEN failure ELSE FALSE
  /\ UNCHANGED <<owed, issued, running, failedUnder, refusedWhileFailed>>

\* A rebuild under token t stops on a unique clash. It writes the failure only
\* if the record still has token t, unless guard 1 is broken; in the same
\* transaction it drops the progress, unless guard 4 is broken. The pages it
\* committed leave partial entries behind.
Stop(t) ==
  /\ t \in running
  /\ running' = running \ {t}
  /\ failedUnder' = failedUnder \cup {t}
  \* The record it writes onto still has its token (or the check is skipped).
  /\ failure' = IF (owed = t \/ (NoTokenCheck /\ owed # None))
                  THEN TRUE ELSE failure
  /\ cleared' = IF owed = t /\ ~KeepProgressOnStop THEN None ELSE cleared
  /\ stale' = IF owed = t THEN TRUE ELSE stale
  /\ UNCHANGED <<owed, issued, refusedWhileFailed>>

\* A rebuild under token t that has cleared and walked every document
\* finishes. It removes the owed record only if the record still has token t,
\* unless guard 3 is broken.
Finish(t) ==
  /\ t \in running
  /\ cleared = t
  /\ running' = running \ {t}
  /\ IF owed = t \/ (NoFinishTokenCheck /\ owed # None)
       THEN /\ owed' = None
            /\ failure' = FALSE
            /\ cleared' = None
       ELSE UNCHANGED <<owed, failure, cleared>>
  /\ UNCHANGED <<issued, stale, failedUnder, refusedWhileFailed>>

\* A writer writes a document. If it maintains the index, the index stays as
\* exact as it was, but a stale entry with the written value refuses the
\* write; if it does not, the index misses the write and becomes stale.
Write ==
  /\ IF Maintained
       THEN /\ refusedWhileFailed' = (refusedWhileFailed \/ (stale /\ ListedFailed))
            /\ UNCHANGED stale
       ELSE /\ stale' = TRUE
            /\ UNCHANGED refusedWhileFailed
  /\ UNCHANGED <<owed, failure, issued, running, cleared, failedUnder>>

\* Any one of the actions above happens next.
Next ==
  \/ Owe
  \/ Begin
  \/ \E t \in Tokens : Clear(t)
  \/ \E t \in Tokens : Stop(t)
  \/ \E t \in Tokens : Finish(t)
  \/ Write

\* The whole behaviour: start, then take steps.
Spec == Init /\ [][Next]_vars

\* Promise: a listed failure belongs to the reindex owed now: some rebuild
\* under its token stopped on a clash.
INV_FailureIsCurrent == ListedFailed => (owed # None /\ owed \in failedUnder)

\* Promise: once nothing is owed, no index lists as failed.
INV_SettledNotFailed == owed = None => ~ListedFailed

\* Promise: an index that lists as failed never refuses a write.
INV_FailedRefusesNoWrite == ~refusedWhileFailed

\* Promise: once nothing is owed, the index matches the documents.
INV_SettledIsExact == owed = None => ~stale

=============================================================================
