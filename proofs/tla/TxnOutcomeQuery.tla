---- MODULE TxnOutcomeQuery ----
\* The story, in plain words.
\*
\* A client sends a commit, and the connection may drop before the node tells
\* it whether the commit landed. The node keeps the transaction's outcome for a
\* short time. The owner may ask again, and the node answers with the outcome
\* it saw: committed, aborted, or indeterminate when the commit failed while
\* the storage engine made it durable, so it may have landed. While the
\* transaction is still open or committing that answer is not the end of it:
\* the owner asks again, and a later answer reports the final outcome.
\* Another client gets only "unknown". An ID written by an earlier
\* server start names no live record of this start, so it also gets "unknown".
\*
\* What must hold:
\*   1. a concrete answer to the owner equals the outcome the query saw;
\*   2. another client never receives a concrete outcome;
\*   3. a query from an old start never receives a concrete outcome;
\*   4. a lost final answer is either not asked for yet, the outcome the
\*      owner's query saw, or safely unknown;
\*   5. an owner that stopped asking holds a final outcome or "unknown",
\*      never a stale "open" or "committing".
\*
\* The model gives the owner a fair chance to ask before the record expires
\* (ownerMayQuery). That is the timing premise behind the liveness promise:
\* retention is finite, so an owner that never asks before expiry may get only
\* "unknown". The owner keeps that chance while every answer it got was
\* "open" or "committing", so polling mid-commit does not use it up.
\*
\* The node keeps at most Cap records per worker. When a new transaction begins
\* and the store is full, the oldest final results make room; a live record
\* (an open or committing transaction) is never dropped, because its result is
\* still to come. A store holding only live records grows past the cap rather
\* than forget one. Other transactions' records are counted, not named: how
\* many are live and how many are final.
\*
\* The code this models: crates/database/db/src/txn/owner/mod.rs, OutcomeStore
\* (insert, evict, expire, mark) and Owners::outcome / Owners::outcome_status;
\* crates/database/db/src/txn/registry/lifecycle.rs,
\* DbTransactionRegistry::outcome_as.
\*
\* RED configs remove one guard:
\*   * QueryWorks=FALSE answers every owner query "unknown";
\*   * BrokenStart=TRUE answers an old-start ID as if it were current;
\*   * LeaksOwner=TRUE answers another client with the owner's outcome;
\*   * ConsumesOnRead=TRUE drops the record when the owner reads it, so an
\*     owner that polled mid-commit gets "unknown" for the final outcome;
\*   * PollsOnce=TRUE ends the owner's asking after its first answer, so an
\*     owner that polled mid-commit is left holding "open" or "committing";
\*   * EvictsLive=TRUE brings back the old last-resort eviction: a full store
\*     with no final result to drop drops a live record, so the owner later
\*     gets "unknown" for a commit that landed.

EXTENDS Naturals

CONSTANTS
  Idle,          \* how many ticks a final outcome is retained
  QueryWorks,    \* TRUE when the owner's query is implemented
  CheckToken,    \* TRUE when the query checks the server-start token
  CheckOwner,    \* TRUE when the query checks the caller's ownership
  BrokenStart,   \* TRUE plants a query that ignores the start token
  LeaksOwner,    \* TRUE plants a foreign-state leak
  ConsumesOnRead, \* TRUE plants a query that removes the record it reads
  PollsOnce,      \* TRUE plants an owner that stops asking after one answer
  EvictsLive,     \* TRUE plants a full store that drops a live record
  Cap             \* how many records one worker's store keeps before evicting

VARIABLES
  status,             \* open, committing, committed, aborted or indeterminate
  lost,               \* whether the owner lost the commit's answer
  answer,             \* the last answer returned, or "none"
  askedBy,            \* owner, foreign, or none
  retained,           \* whether the final outcome is still retained
  token,              \* the current server-start token
  idToken,            \* the token written into the client's transaction ID
  age,                \* idle ticks since the outcome became final
  ownerMayQuery,      \* the owner still has its fair chance before expiry
  querySeenStatus,    \* the real status at the moment of the last query
  queryTokenAnswered, \* whether a concrete answer was issued under the ID's token
  queryToken,         \* the server-start token the last query saw
  otherLive,          \* how many other transactions' live records are stored
  otherFinal          \* how many other transactions' final results are stored

vars == <<status, lost, answer, askedBy, retained, token, idToken, age,
          ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken,
          otherLive, otherFinal>>
\* Every variable, so a step that leaves them all alone is a stutter.

others == <<otherLive, otherFinal>>
\* The other transactions' records, which most steps leave alone.

OldStart == token # idToken
\* OldStart says the ID was written by an earlier server start.

Final(state) == state \in {"committed", "aborted", "indeterminate"}
\* Final says the commit or abort has settled and cannot change again. An
\* indeterminate commit is final too: this server start never learns more
\* about it (owner/mod.rs Outcome::is_final).

Size == (IF retained THEN 1 ELSE 0) + otherLive + otherFinal
\* Size counts every record in the store: ours, if it is still kept, plus the
\* other transactions' live records and final results.

Init ==
  /\ status = "open"
  /\ lost = FALSE
  /\ answer = "none"
  /\ askedBy = "none"
  /\ retained = TRUE
  /\ token = 1
  /\ idToken = IF BrokenStart THEN 2 ELSE 1
  /\ age = 0
  /\ ownerMayQuery = TRUE
  /\ querySeenStatus = "none"
  /\ queryTokenAnswered = FALSE
  /\ queryToken = 1
  /\ otherLive = 0
  /\ otherFinal = 0
  \* Every transaction starts open, under the current start token, before any
  \* query has answered, in a store that holds only its record.

QueueCommit ==
  /\ status = "open"
  /\ status' = "committing"
  /\ UNCHANGED <<lost, answer, askedBy, retained, token, idToken, age,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
  /\ UNCHANGED others
  \* A commit arrives, but its answer has not been decided yet.

FinishCommit ==
  /\ status = "committing"
  /\ status' \in {"committed", "aborted", "indeterminate"}
  /\ age' = 0
  /\ UNCHANGED <<lost, answer, askedBy, retained, token, idToken,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
  /\ UNCHANGED others
  \* The node decides the commit; any of the three final results is possible
  \* here (owner/mod.rs Outcome::of).

LoseAnswer ==
  /\ lost' = TRUE
  /\ UNCHANGED <<status, answer, askedBy, retained, token, idToken, age,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
  /\ UNCHANGED others
  \* The connection drops before the client receives an answer.

OwnerQuery ==
  /\ lost
  /\ ownerMayQuery
  /\ askedBy' = "owner"
  /\ answer' =
       IF CheckToken /\ (token # idToken)
       THEN IF BrokenStart /\ QueryWorks /\ retained
            THEN status
            ELSE "unknown"
       ELSE IF QueryWorks /\ retained
            THEN status
            ELSE "unknown"
  /\ ownerMayQuery' = IF PollsOnce
                      THEN FALSE
                      ELSE (answer' \in {"open", "committing"})
  \* An answer that is not final yet leaves the owner asking again; the
  \* planted bug stops it after the first answer, whatever that answer was.
  /\ retained' = (retained /\ ~ConsumesOnRead)
  \* A correct read leaves the record; the planted bug removes it.
  /\ querySeenStatus' = status
  /\ queryTokenAnswered' = (answer' # "unknown")
  /\ queryToken' = token
  /\ UNCHANGED <<status, lost, token, idToken, age>>
  /\ UNCHANGED others
  \* The owner asks about its lost answer; a correct node answers the outcome
  \* it still retains, or unknown after expiry or for an old-start ID.
  \* querySeenStatus records the real status at the moment of the query, not
  \* the answer, so INV_Agreement can catch an answer that differs from it.

ForeignQuery ==
  /\ askedBy' = "foreign"
  /\ answer' = IF LeaksOwner THEN status ELSE "unknown"
  /\ querySeenStatus' = status
  /\ queryTokenAnswered' = (answer' # "unknown")
  /\ queryToken' = token
  /\ UNCHANGED <<status, lost, retained, token, idToken, age,
                 ownerMayQuery>>
  /\ UNCHANGED others
  \* Another client asks. A correct node says only unknown, because it cannot
  \* disclose another caller's transaction.

Restart ==
  /\ token = 1
  /\ ~ownerMayQuery
  /\ token' = 2
  /\ UNCHANGED <<status, lost, answer, askedBy, retained, idToken, age,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  /\ UNCHANGED others
  \* A restart mints a new token only after the owner's query chance. One
  \* transition is enough without creating unbounded token values. A RED
  \* BrokenStart model begins with an old ID instead.

Expire ==
  /\ ~ownerMayQuery
  /\ Final(status)
  /\ age < Idle
  /\ age' = age + 1
  /\ retained' = (retained /\ ((age + 1) < Idle))
  \* A record the cap already dropped stays dropped.
  /\ UNCHANGED <<status, lost, answer, askedBy, token, idToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  /\ UNCHANGED others
  \* The record ages only after a final outcome, and only after the owner has
  \* had its fair chance to ask; after Idle ticks it is no longer retained.

OtherBegin ==
  /\ otherLive < Cap
  \* Bound the other live transactions so the model stays finite.
  /\ IF Size < Cap
     THEN UNCHANGED <<retained, otherFinal>>
     \* Below the cap the new record just goes in.
     ELSE IF otherFinal > 0
     THEN /\ otherFinal' = otherFinal - 1
          /\ UNCHANGED retained
     \* At the cap the oldest final result makes room. Another transaction's
     \* final result counts as older than ours: ours is protected by the
     \* timing premise below.
     ELSE IF retained /\ Final(status)
     THEN /\ ~ownerMayQuery
          /\ retained' = FALSE
          /\ UNCHANGED otherFinal
     \* Our final result is the only one left, so it makes room, but only
     \* after the owner's fair chance to ask: the cap is retention too, and
     \* the same premise as Expire applies.
     ELSE /\ retained' = (retained /\ ~EvictsLive)
          /\ UNCHANGED otherFinal
     \* Only live records are left. A correct store drops none of them and
     \* grows past the cap; the planted bug drops our live record.
  /\ otherLive' = otherLive + 1
  \* The new transaction's live record is stored.
  /\ UNCHANGED <<status, lost, answer, askedBy, token, idToken, age,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  \* Another transaction begins, which is the moment the store may evict.

OtherFinish ==
  /\ otherLive > 0
  /\ otherLive' = otherLive - 1
  /\ otherFinal' = otherFinal + 1
  /\ UNCHANGED <<status, lost, answer, askedBy, retained, token, idToken, age,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  \* Another transaction commits or aborts; its record becomes a final result.

OtherExpire ==
  /\ otherFinal > 0
  /\ otherFinal' = otherFinal - 1
  /\ UNCHANGED <<status, lost, answer, askedBy, retained, token, idToken, age,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken,
                 otherLive>>
  \* Another transaction's final result outlives its retention and is dropped.

Next ==
  \/ QueueCommit
  \/ FinishCommit
  \/ LoseAnswer
  \/ OwnerQuery
  \/ ForeignQuery
  \/ Restart
  \/ Expire
  \/ OtherBegin
  \/ OtherFinish
  \/ OtherExpire
  \* Any one enabled action may be the next step.

Fair == WF_vars(OwnerQuery)
\* Once the owner's query is enabled, it is eventually taken.

Spec == Init /\ [][Next]_vars /\ Fair
\* The system starts once, takes only the actions above, and answers a fair
\* owner query.

INV_Agreement ==
  (answer \notin {"none", "unknown"}) => answer = querySeenStatus
  \* Every concrete owner answer equals the outcome that query saw.

INV_Foreign ==
  (askedBy = "foreign") => answer = "unknown"
  \* A foreign query never reveals a concrete state.

INV_StartToken ==
  CheckToken => (queryTokenAnswered => queryToken = idToken)
  \* A concrete answer was issued only under the ID's own start token.

INV_FinalAnswer ==
  (lost /\ Final(status) /\ QueryWorks) =>
    answer \in {"none", querySeenStatus, "unknown"}
  \* A lost final answer is unasked, the outcome the owner query saw, or
  \* safely unknown.

INV_Bounded ==
  age <= Idle
  \* Retention ages only to the configured idle limit.

INV_OwnerSettled ==
  (askedBy = "owner" /\ ~ownerMayQuery) =>
    answer \in {"committed", "aborted", "unknown"}
  \* An owner that stopped asking was left with a final outcome or unknown,
  \* never with an "open" or "committing" that the commit has since outgrown.

LIVE_QuestionAnswered ==
  [](lost /\ Final(status) /\ ~OldStart /\ ownerMayQuery
       => <>(answer = status))
  \* A lost final answer is eventually reported exactly, while the owner still
  \* has its fair chance before retention expires.

====
