---- MODULE CommitResponseWait ----
\* The story, in plain words.
\*
\* One client asks a live HTTP server to commit one explicit transaction. The
\* commit may stay queued or running forever, because the worker or storage is
\* wedged. The server does not use a client idle timeout to guess: it has its
\* own bound on how long it waits to answer. When the bound passes while the
\* outcome is still unknown, it answers "unknown"; the commit keeps its ticket
\* and may still land or fail later. The client therefore settles, but it does
\* not mistake an unknown answer for an abort.
\*
\* Correspondence with the Rust code:
\*   crates/surfaces/http/src/server.rs Server::router commit bound, and
\*   its admitted layer: the bound drops only the wait on the handler's edge
\*   task, so the handler, every commit step included, runs to its end;
\*   crates/engine/query/src/runner/executor.rs QueryRunner::commit_txn_as;
\*   crates/database/db/src/txn/owner/mod.rs Owners::finish_detached (the commit stays
\*   owned and fenced after its waiter goes); regolith's CommitTicket, whose
\*   drop never cancels a commit already claimed by commit_nowait.

EXTENDS Naturals
\* Naturals gives the model's discrete clock.

CONSTANTS Bound, CommitBound, CommitKeepsRunning
\* Bound is the number of ticks before the server answers unknown.
\* CommitBound turns the server's bound on or off.
\* CommitKeepsRunning says whether an unknown answer leaves the commit alone.

ASSUME /\ Bound \in 1..3
       /\ CommitBound \in BOOLEAN
       /\ CommitKeepsRunning \in BOOLEAN
\* A tiny bound keeps TLC small; the switches are either on or off.

Responses == {"none", "success", "failure", "unknown"}
\* What the server has sent to the waiting client.

CommitStates == {"none", "queued", "running", "landed", "failed", "cancelled"}
\* The abstract life of the storage commit.

VARIABLES clock, waiting, submitted, answered, response, commit
\* time       - the server clock, in whole ticks;
\* waiting    - whether the client is still waiting for an answer;
\* sent       - whether the commit was handed to the server;
\* answered   - whether the client has received an answer;
\* response   - that answer, or "none" before it;
\* commit     - where the storage commit is.

vars == <<clock, waiting, submitted, answered, response, commit>>
\* Every variable, for "nothing else changes" and for fairness.

Init == /\ clock = 0
        /\ waiting = FALSE
        /\ submitted = FALSE
        /\ answered = FALSE
        /\ response = "none"
        /\ commit = "none"
\* At the start no request exists, the clock is zero, and nothing answered.

Request(c) == /\ ~submitted
              /\ waiting' = TRUE
              /\ submitted' = TRUE
              /\ commit' = "queued"
              /\ UNCHANGED <<clock, answered, response>>
\* The client sends one commit; the server queues it and begins to wait.

StartCommit == /\ commit = "queued"
               /\ commit' = "running"
               /\ UNCHANGED <<clock, waiting, submitted, answered, response>>
\* The owner takes the queued commit and starts the storage commit. This step
\* is not forced, so a wedged queue may never take it.

FinishCommit(outcome) == /\ commit = "running"
                         /\ outcome \in {"landed", "failed"}
                         /\ commit' = outcome
                         /\ UNCHANGED <<clock, waiting, submitted, answered, response>>
\* Storage finishes the running commit. This step is not forced either, so a
\* wedged commit may never finish.

Answer == /\ waiting
          /\ commit \in {"landed", "failed"}
          /\ response' = IF commit = "landed" THEN "success" ELSE "failure"
          /\ waiting' = FALSE
          /\ answered' = TRUE
          /\ UNCHANGED <<clock, submitted, commit>>
\* The commit finished before the bound, so the client gets its real outcome.

ApplyBoundKeep == /\ CommitBound
                  /\ CommitKeepsRunning
                  /\ waiting
                  /\ commit \in {"queued", "running"}
                  /\ clock >= Bound
                  /\ response' = "unknown"
                  /\ waiting' = FALSE
                  /\ answered' = TRUE
                  /\ commit' = commit
                  /\ clock' = clock
                  /\ submitted' = submitted
\* The server's bound passed; the commit's ticket stays with its owner.

ApplyBoundCancel == /\ CommitBound
                  /\ ~CommitKeepsRunning
         /\ waiting
         /\ commit \in {"queued", "running"}
         /\ clock >= Bound
         /\ response' = "unknown"
         /\ waiting' = FALSE
         /\ answered' = TRUE
         /\ commit' = "cancelled"
         /\ clock' = clock
         /\ submitted' = submitted
\* The server's bound passed with no known outcome. It answers unknown; in the
\* real design the commit's ticket stays owned, while the RED switch cancels it.

Tick == /\ clock < Bound + 1
        /\ clock' = clock + 1
        /\ UNCHANGED <<waiting, submitted, answered, response, commit>>
\* Time moves while it is still needed; capping it keeps the state space small.

Next == Request("client") \/ StartCommit \/ FinishCommit("landed")
        \/ FinishCommit("failed") \/ Answer \/ ApplyBoundKeep \/ ApplyBoundCancel \/ Tick
\* One request is enough: every guard is about one commit and its answer.

Spec == Init /\ [][Next]_vars /\ WF_vars(Tick)
        /\ WF_vars(ApplyBoundKeep) /\ WF_vars(ApplyBoundCancel)
        /\ WF_vars(Answer)
\* The bound and a pre-bound outcome are eventually delivered. A stuck commit
\* deliberately has no fairness, so only the server bound guarantees settling.

TypeOK == /\ clock \in 0..Bound + 1
          /\ waiting \in BOOLEAN
          /\ submitted \in BOOLEAN
          /\ answered \in BOOLEAN
          /\ response \in Responses
          /\ commit \in CommitStates
\* Every variable stays in its declared range.

ClientSettles == waiting ~> answered
\* A client waiting for an commit answer eventually receives some answer.

UnknownIsNotAbort == (response = "unknown") => (commit # "cancelled")
\* An unknown answer must not cancel or mean cancellation of the commit.

AnswerOnlyAfterSend == answered => submitted
\* The server answers only a commit that was actually sent.
====
