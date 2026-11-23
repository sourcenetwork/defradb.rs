\* A client waits for the answer to a transaction commit.
\* Actors: the owner client, a foreign client, and the node.
\* What can happen: the commit is queued, its answer is lost, the owner or a
\* foreign client asks for the outcome, the record expires, or the node restarts.
\* What must never happen: the owner is told anything but the node's final
\* outcome, a foreign client is told a state, or an old start's ID is answered.

\* A client waits for the answer to a transaction commit.
\* Actors: the owner client, a foreign client, and the node.
\* What can happen: the commit is queued, its answer is lost, the owner or a
\* foreign client asks for the outcome, the record expires, or the node restarts.
\* What must never happen: the owner is told anything but the node's final
\* outcome, a foreign client is told a state, or an old start's ID is answered.

MODULE TxnOutcomeQuery

EXTENDS Naturals

CONSTANTS
    \* The idle age, in ticks, after which a final outcome may be forgotten.
    Idle,
    \* TRUE when the owner's outcome query is implemented.
    QueryWorks,
    \* TRUE when the query checks the token of the server start.
    CheckToken,
    \* TRUE when the query checks the caller's ownership.
    CheckOwner,
    \* FALSE in the green model; TRUE plants a query that ignores the token.
    BrokenStart,
    \* FALSE in the green model; TRUE plants a foreign-state leak.
    LeaksOwner

VARIABLES
    \* The node's state: open, committing, committed or aborted.
    status,
    \* Whether the owner lost the commit's answer.
    lost,
    \* The last answer a query returned, or "none".
    answer,
    \* Which client asked the last query: "owner" or "foreign".
    askedBy,
    \* Whether the final outcome is still retained.
    retained,
    \* The current server-start token.
    token,
    \* The token written into the client's transaction ID.
    idToken,
    \* The number of idle ticks since the outcome became final.
    age

vars == <<status, lost, answer, askedBy, retained, token, idToken, age>>

OldStart == token # idToken
\* OldStart says the ID was written by an earlier server start.

Final(state) == state \in {"committed", "aborted"}
\* Final says the commit or abort is settled and cannot change again.

Init ==
    /\ status = "open"
    /\ lost = FALSE
    /\ answer = "none"
    /\ askedBy = "none"
    /\ retained = TRUE
    /\ token = 1
    /\ idToken = 1
    /\ age = 0
    \* Every transaction starts open, with the same current and ID tokens.

QueueCommit ==
    /\ status = "open"
    /\ status' = "committing"
    /\ UNCHANGED <<lost, answer, askedBy, retained, token, idToken, age>>
    \* A commit arrives, but its answer has not been decided yet.

FinishCommit ==
    /\ status = "committing"
    /\ status' \in {"committed", "aborted"}
    /\ age' = 0
    /\ retained' = TRUE
    /\ UNCHANGED <<lost, answer, askedBy, token, idToken>>
    \* The node decides the commit; either final result is possible here.

LoseAnswer ==
    /\ lost' = TRUE
    /\ UNCHANGED <<status, answer, askedBy, retained, token, idToken, age>>
    \* The connection drops before the client receives an answer.

OwnerQuery ==
    /\ askedBy' = "owner"
    /\ answer' =
        IF token # idToken /\ CheckToken
            THEN IF BrokenStart THEN status ELSE "unknown"
        ELSE IF status \in {"open", "committing", "committed", "aborted"}
                /\ QueryWorks /\ retained
            THEN status
        ELSE "unknown"
    /\ UNCHANGED <<status, lost, retained, token, idToken, age>>
    \* The owner asks for the outcome; the node answers its current or retained state.

ForeignQuery ==
    /\ askedBy' = "foreign"
    /\ answer' = IF LeaksOwner THEN status ELSE "unknown"
    /\ UNCHANGED <<status, lost, retained, token, idToken, age>>
    \* A foreign client asks; a correct node says only unknown.

Restart ==
    /\ token' = token + 1
    /\ idToken' = idToken
    /\ UNCHANGED <<status, lost, answer, askedBy, retained, age>>
    \* A restart mints a new token; a broken model wrongly rewrites the old ID.

Expire ==
    /\ age < Idle
    /\ age' = age + 1
    /\ retained' = age + 1 < Idle
    /\ UNCHANGED <<status, lost, answer, askedBy, token, idToken>>
    \* Each idle tick ages the retained outcome until the idle limit passes.

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
\* The system starts once and takes only the actions above, allowing stuttering.

INV_Agreement ==
    QueryWorks => (answer # "unknown" => answer = status)
    \* Any concrete owner answer equals the node's own state.

INV_Foreign ==
    ~LeaksOwner => (askedBy = "foreign" => answer = "unknown")
    \* A foreign query never reveals a state.

INV_StartToken ==
    CheckToken => (token # idToken => answer = "unknown")
    \* An ID from an earlier start is never answered with a new state.

INV_FinalAnswer ==
    lost /\ Final(status) /\ QueryWorks /\ ~LeaksOwner /\ CheckToken =>
        answer \in {"none", status, "unknown"}
    \* A lost answer is either still unasked, the final state, or safely unknown.

INV_Bounded ==
    age <= Idle
    \* Retention ages only to the configured idle limit.

LIVE_QuestionAnswered ==
    [](lost /\ Final(status) /\ ~OldStart => <>(answer = status))
    \* A lost final answer is eventually reported to the owner while retained.

====
