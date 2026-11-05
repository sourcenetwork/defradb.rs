---- MODULE PushLogJoin ----
\* The story, in plain words.
\*
\* Two peers (the senders) push the same block to this node at almost the same
\* moment: one is a new replicator replaying the documents that already exist,
\* the other is the document's own live push. The block has one name, its CID.
\* This node lets one request at a time work on a CID: the request that takes
\* the CID's guard first processes the block (stores it and records it so it
\* is merged), and lets the guard go when it is finished, whether it worked or
\* not. The second request used to be turned away at once ("already being
\* processed, retry later"), so its sender backed off and its acknowledgement
\* came late. Now the second request joins: it waits, for a bounded time, for
\* the first to let go, then looks again. If the block is safe by then it
\* answers success; if not (the first attempt failed) it takes the guard and
\* processes its own copy; if the time runs out first it gives today's answer,
\* "already being processed, retry later".
\*
\* What must never happen:
\*   1. a request answers success while the block is not safe (merged, or
\*      registered so this node finishes it on its own) - INV_SuccessImpliesDurable;
\*   2. two requests process the block at the same time - INV_OneProcessor;
\*   3. a request that joined waits forever - LIVE_EveryJoinEnds.
\*
\* Correspondence with the Rust code, crates/p2p/src/sync/manager/process/pushlog.rs
\* and join.rs unless named otherwise:
\*
\*   | Model        | Rust                                                                     |
\*   |--------------|--------------------------------------------------------------------------|
\*   | Arrive       | process_pushlog_with_provider_evidence: process_queue.try_acquire_nowait |
\*   | Check        | join_in_flight: needs_nothing_more (recovery registered,                 |
\*   |              | then blockstore.is_merged)                                               |
\*   | Acquire      | join_in_flight: process_queue.try_acquire                                |
\*   | Wake         | join_in_flight: ProcessWait::released (sync/queue.rs), the owner's Event |
\*   |              | set by ProcessQueue::release_sync when its ProcessGuard drops            |
\*   | Timeout      | join_in_flight: workers::timeout(PUSHLOG_JOIN_DEADLINE, ..) elapsing,    |
\*   |              | answered with Error::PushLogInFlight                                     |
\*   | Finish       | process_block_inner + assert_pushlog_left_durable_state, then the        |
\*   |              | ProcessGuard drops                                                       |
\*   | guard        | ProcessQueue.owners (sync/queue.rs)                                      |
\*   | durable      | is_pending_dag_recovery_registered || blockstore.is_merged               |
\*   | Recheck      | the look join_in_flight takes after every wake (RED: skipped)            |
\*   | UseGuard     | try_acquire before processing a copy (RED: processed without it)         |
\*   | Deadline     | the workers::timeout around the join (RED: no deadline)                  |

EXTENDS Naturals, FiniteSets
\* Naturals gives us numbers and +; FiniteSets gives Cardinality (the size of a set).

CONSTANTS Recheck, UseGuard, Deadline
\* Three switches. All TRUE is the real design; each RED configuration turns one off.

ASSUME /\ Recheck \in BOOLEAN
       /\ UseGuard \in BOOLEAN
       /\ Deadline \in BOOLEAN
\* Each switch is either on or off, nothing else.

Senders == {"replay", "live"}
\* The two requests carrying the same block: the replicator's replay and the live push.

None == "none"
\* A marker meaning "nobody": nobody owns the guard, or a request waits on nobody.

MaxFailures == 1
\* At most one processing attempt fails in a run: enough to show the waiter
\* must not trust a finished owner, and it keeps the model tiny.

States == {"idle", "check", "acquire", "waiting", "owner", "acked", "nacked", "fallback"}
\* Where one request can be:
\*   idle     - it has not arrived yet;
\*   check    - it joined and is looking whether the block is safe already;
\*   acquire  - it saw the block is not safe and is about to try for the guard;
\*   waiting  - someone else holds the guard and it waits for them to let go;
\*   owner    - it is processing its own copy of the block;
\*   acked    - it answered success;
\*   nacked   - its own processing failed and it answered that error;
\*   fallback - its wait ran out of time and it answered "already being processed".

JoinStates == {"check", "acquire", "waiting"}
\* The states of a request that joined someone else's processing.

EndOfJoin == {"owner", "acked", "fallback"}
\* How a join may end: it processes its own copy, answers success, or falls back.

VARIABLES pc, guard, waitingOn, durable, failures
\* pc        - where each request is (one of States);
\* guard     - which request owns the CID right now, or None;
\* waitingOn - for a waiting request, the owner whose letting go it waits for;
\* durable   - whether the block is safe (merged or registered for recovery);
\* failures  - how many processing attempts have failed so far.

vars == <<pc, guard, waitingOn, durable, failures>>
\* All the variables together, so we can say "nothing else changes".

TypeOK ==
  \* Every variable holds a value of the right kind.
  /\ pc \in [Senders -> States]
  \* each request is in one of the named states;
  /\ guard \in Senders \cup {None}
  \* the guard is owned by one request or by nobody;
  /\ waitingOn \in [Senders -> Senders \cup {None}]
  \* each request waits on one request or on nobody;
  /\ durable \in BOOLEAN
  \* the block is safe or it is not;
  /\ failures \in 0..MaxFailures
  \* and no more attempts failed than the run allows.

Init ==
  \* At the start nothing has happened yet.
  /\ pc = [s \in Senders |-> "idle"]
  \* neither request has arrived;
  /\ guard = None
  \* nobody owns the CID;
  /\ waitingOn = [s \in Senders |-> None]
  \* nobody waits on anybody;
  /\ durable = FALSE
  \* the block is not stored anywhere yet;
  /\ failures = 0
  \* and nothing has failed.

Arrive(s) ==
  \* Request s arrives and tries for the guard once, without waiting
  \* (try_acquire_nowait). If the CID is free it becomes the owner; if not it joins.
  /\ pc[s] = "idle"
  \* only a request that has not arrived yet can arrive;
  /\ IF guard = None
     \* the CID is free:
     THEN /\ guard' = s
          \* s takes the guard,
          /\ pc' = [pc EXCEPT ![s] = "owner"]
          \* and starts processing its block;
     ELSE /\ guard' = guard
          \* someone else owns it: the guard stays theirs,
          /\ pc' = [pc EXCEPT ![s] = "check"]
          \* and s joins, starting with a look at the block;
  /\ UNCHANGED <<waitingOn, durable, failures>>
  \* arriving changes nothing else.

Check(s) ==
  \* A joined request looks whether the block is safe already. If it is, it
  \* answers success; if not, it goes on to try for the guard.
  /\ pc[s] = "check"
  \* only a request that is looking can take this step;
  /\ pc' = [pc EXCEPT ![s] = IF durable THEN "acked" ELSE "acquire"]
  \* success only when the block is safe, otherwise on to the guard;
  /\ UNCHANGED <<guard, waitingOn, durable, failures>>
  \* looking changes nothing else.

Acquire(s) ==
  \* A joined request that saw the block is not safe tries for the guard
  \* (try_acquire). Free: it owns it and processes its own copy. Taken: it
  \* waits for that owner to let go.
  /\ pc[s] = "acquire"
  \* only a request that is about to try for the guard can take this step;
  /\ IF guard = None
     \* the CID is free:
     THEN /\ guard' = s
          \* s takes the guard,
          /\ pc' = [pc EXCEPT ![s] = "owner"]
          \* and processes its own copy;
          /\ waitingOn' = waitingOn
          \* it waits on nobody;
     ELSE IF UseGuard
          \* someone owns it, and the guard is respected (the real design):
          THEN /\ guard' = guard
               \* the guard stays with its owner,
               /\ pc' = [pc EXCEPT ![s] = "waiting"]
               \* s starts to wait,
               /\ waitingOn' = [waitingOn EXCEPT ![s] = guard]
               \* and remembers whose letting go it waits for;
          ELSE /\ guard' = guard
               \* the planted bug: the guard stays with its owner,
               /\ pc' = [pc EXCEPT ![s] = "owner"]
               \* yet s processes its copy anyway, next to the owner,
               /\ waitingOn' = waitingOn
               \* and waits on nobody;
  /\ UNCHANGED <<durable, failures>>
  \* trying for the guard does not touch the block.

Wake(s) ==
  \* The owner a request waits on has let the guard go, so the wait ends. In
  \* the real design the request looks again; the planted bug answers success
  \* at once, trusting that the owner must have finished the job.
  /\ pc[s] = "waiting"
  \* only a waiting request can wake;
  /\ guard # waitingOn[s]
  \* and only once the owner it waits on no longer holds the guard;
  /\ pc' = [pc EXCEPT ![s] = IF Recheck THEN "check" ELSE "acked"]
  \* look again (real) or answer success blindly (bug);
  /\ waitingOn' = [waitingOn EXCEPT ![s] = None]
  \* the request no longer waits on anybody;
  /\ UNCHANGED <<guard, durable, failures>>
  \* waking changes nothing else.

Timeout(s) ==
  \* The join's deadline passes while the request is looking or waiting: it
  \* gives up and answers "already being processed, retry later". Without a
  \* deadline (the planted bug) this never happens.
  /\ Deadline
  \* only when the join has a deadline;
  /\ pc[s] \in {"check", "waiting"}
  \* the deadline can fire only where the join pauses (the look and the wait);
  /\ pc' = [pc EXCEPT ![s] = "fallback"]
  \* the request answers the retry-later error;
  /\ waitingOn' = [waitingOn EXCEPT ![s] = None]
  \* and stops waiting on anybody;
  /\ UNCHANGED <<guard, durable, failures>>
  \* the guard and the block are untouched: a joined request owns neither.

Finish(s) ==
  \* A processing request finishes and lets the guard go. Either its processing
  \* worked, so the block is safe and it answers success, or (at most once per
  \* run) it failed, so the block is no safer than before and it answers the error.
  /\ pc[s] = "owner"
  \* only a request that is processing can finish;
  /\ guard' = IF guard = s THEN None ELSE guard
  \* it lets the guard go if it held it (the bug's second processor holds nothing);
  /\ \/ /\ durable' = TRUE
        \* success: the block is now merged or registered for recovery,
        /\ pc' = [pc EXCEPT ![s] = "acked"]
        \* and the request answers success,
        /\ failures' = failures
        \* with no new failure;
     \/ /\ failures < MaxFailures
        \* failure, if this run still has its one failure to spend:
        /\ failures' = failures + 1
        \* count it,
        /\ pc' = [pc EXCEPT ![s] = "nacked"]
        \* the request answers its error,
        /\ durable' = durable
        \* and the block is exactly as safe as before;
  /\ UNCHANGED waitingOn
  \* finishing does not change who waits on whom.

Next ==
  \* One step of the system: some request takes one of its steps.
  \E s \in Senders :
    \* for some request s,
    \/ Arrive(s)
    \* it arrives,
    \/ Check(s)
    \* or looks at the block,
    \/ Acquire(s)
    \* or tries for the guard,
    \/ Wake(s)
    \* or wakes when its owner lets go,
    \/ Timeout(s)
    \* or runs out of time,
    \/ Finish(s)
    \* or finishes its own processing.

Fairness ==
  \* What the scheduler promises. A joined request keeps moving: it eventually
  \* looks, eventually tries for the guard, eventually wakes once its owner let
  \* go, and its deadline eventually fires if it is still paused at the look or
  \* the wait (strong fairness: the clock keeps running across loop rounds).
  \* Nothing promises that a request arrives, nor that an owner ever finishes:
  \* an owner may be slow forever, which is exactly what the deadline is for.
  \A s \in Senders :
    \* for every request,
    /\ WF_vars(Check(s))
    \* a request that can look eventually does;
    /\ WF_vars(Acquire(s))
    \* a request that can try for the guard eventually does;
    /\ WF_vars(Wake(s))
    \* a request whose owner let go eventually wakes;
    /\ SF_vars(Timeout(s))
    \* a deadline that keeps being able to fire eventually fires.

Spec == Init /\ [][Next]_vars /\ Fairness
\* The whole system: start at Init, take Next steps, under the fairness promises.

INV_SuccessImpliesDurable ==
  \* A request answers success only when the block is safe.
  \A s \in Senders : pc[s] = "acked" => durable
  \* for every request: if it said success, the block is merged or registered.

INV_OneProcessor ==
  \* At most one request processes the block at any moment.
  Cardinality({s \in Senders : pc[s] = "owner"}) <= 1
  \* the set of processing requests never has two members.

LIVE_EveryJoinEnds ==
  \* A request that joined never waits forever: it ends up processing its own
  \* copy, answering success, or falling back to "retry later".
  \A s \in Senders : (pc[s] \in JoinStates) ~> (pc[s] \in EndOfJoin)
  \* for every request: once it is in the join, it eventually leaves it for one of the three ends.

====
