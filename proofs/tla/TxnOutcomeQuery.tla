---- MODULE TxnOutcomeQuery ----
\* The story, in plain words.
\*
\* A client sends a commit, and the connection may drop before the node tells
\* it whether the commit landed. The node keeps the transaction's outcome for a
\* short time. The owner may ask again, and the node answers with the outcome
\* it saw. Another client gets only "unknown". An ID written by an earlier
\* server start names no live record of this start, so it also gets "unknown".
\*
\* What must hold:
\*   1. a concrete answer to the owner equals the outcome the query saw;
\*   2. another client never receives a concrete outcome;
\*   3. a query from an old start never receives a concrete outcome;
\*   4. a lost final answer is either not asked for yet, the outcome the
\*      owner's query saw, or safely unknown.
\*
\* The model gives the owner a fair chance to ask before the record expires
\* (ownerMayQuery). That is the timing premise behind the liveness promise:
\* retention is finite, so an owner that never asks before expiry may get only
\* "unknown".
\*
\* RED configs remove one guard:
\*   * QueryWorks=FALSE answers every owner query "unknown";
\*   * BrokenStart=TRUE answers an old-start ID as if it were current;
\*   * LeaksOwner=TRUE answers another client with the owner's outcome.

EXTENDS Naturals

CONSTANTS
  Idle,          \* how many ticks a final outcome is retained
  QueryWorks,    \* TRUE when the owner's query is implemented
  CheckToken,    \* TRUE when the query checks the server-start token
  CheckOwner,    \* TRUE when the query checks the caller's ownership
  BrokenStart,   \* TRUE plants a query that ignores the start token
  LeaksOwner     \* TRUE plants a foreign-state leak

VARIABLES
  status,             \* open, committing, committed or aborted
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
  queryToken          \* the server-start token the last query saw

vars == <<status, lost, answer, askedBy, retained, token, idToken, age,
          ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>

OldStart == token # idToken
\* OldStart says the ID was written by an earlier server start.

Final(state) == state \in {"committed", "aborted"}
\* Final says the commit or abort has settled and cannot change again.

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
  \* Every transaction starts open, under the current start token, before any
  \* query has answered.

QueueCommit ==
  /\ status = "open"
  /\ status' = "committing"
  /\ UNCHANGED <<lost, answer, askedBy, retained, token, idToken, age,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
  \* A commit arrives, but its answer has not been decided yet.

FinishCommit ==
  /\ status = "committing"
  /\ status' \in {"committed", "aborted"}
  /\ age' = 0
  /\ retained' = TRUE
  /\ UNCHANGED <<lost, answer, askedBy, token, idToken,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
  \* The node decides the commit; either final result is possible here.

LoseAnswer ==
  /\ lost' = TRUE
  /\ UNCHANGED <<status, answer, askedBy, retained, token, idToken, age,
                 queryToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered>>
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
  /\ ownerMayQuery' = FALSE
  /\ querySeenStatus' = status
  /\ queryTokenAnswered' = (answer' # "unknown")
  /\ queryToken' = token
  /\ UNCHANGED <<status, lost, retained, token, idToken, age>>
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
  \* Another client asks. A correct node says only unknown, because it cannot
  \* disclose another caller's transaction.

Restart ==
  /\ token = 1
  /\ ~ownerMayQuery
  /\ token' = 2
  /\ UNCHANGED <<status, lost, answer, askedBy, retained, idToken, age,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  \* A restart mints a new token only after the owner's query chance. One
  \* transition is enough without creating unbounded token values. A RED
  \* BrokenStart model begins with an old ID instead.

Expire ==
  /\ ~ownerMayQuery
  /\ Final(status)
  /\ age < Idle
  /\ age' = age + 1
  /\ retained' = ((age + 1) < Idle)
  /\ UNCHANGED <<status, lost, answer, askedBy, token, idToken,
                 ownerMayQuery, querySeenStatus, queryTokenAnswered, queryToken>>
  \* The record ages only after a final outcome, and only after the owner has
  \* had its fair chance to ask; after Idle ticks it is no longer retained.

Next ==
  \/ QueueCommit
  \/ FinishCommit
  \/ LoseAnswer
  \/ OwnerQuery
  \/ ForeignQuery
  \/ Restart
  \/ Expire
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

LIVE_QuestionAnswered ==
  [](lost /\ Final(status) /\ ~OldStart /\ ownerMayQuery
       => <>(answer = status))
  \* A lost final answer is eventually reported exactly, while the owner still
  \* has its fair chance before retention expires.

====
