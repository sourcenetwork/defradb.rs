---- MODULE DeferredDrainTask ----
\* The story: a deferred index owes work after a write (see DeferredIndex.tla
\* for what a drain does to one index). This model is about who asks for the
\* work and when the one drain task gets to it.
\*
\* A write that owes an index also asks for a drain of it. The drain task
\* sleeps until it is asked, or until its sweep timer is due. When it wakes it
\* runs a pass: the indexes it was asked for, or every index on a sweep. In a
\* pass each index gets one turn. A turn may leave more work, may succeed, or
\* may fail. The sweep also covers what a crash forgot: a reopened node runs
\* one sweep pass first (the settle at open).
\*
\* Who acts: a hot index written all the time; a quiet index written twice,
\* once by a purge and once by an update, whose drain can fail once; a poison
\* index whose drain always fails; the drain task; time; one crash.
\*
\* What must never happen: an index owes work that nobody has asked for and no
\* pass is about to do; the quiet index stays owed forever while the hot index
\* keeps the task busy.
\*
\* A turn that drains some documents but leaves one owed because its migration
\* fails (DeferredIndex.tla, Carry) is kept for the sweep without a wake, as a
\* failed index is here. Requests still drain the index's other documents and
\* skip that one; each sweep tries it again.
\*
\* Code it models:
\*   crates/db/src/index/deferred/task.rs     drain_pass (turns, kept failures,
\*                                            documents left to the sweep),
\*                                            next_wake (the fixed sweep deadline,
\*                                            a due sweep first), the settle at open
\*   crates/db/src/index/deferred/requests.rs DrainRequests::request, defer, take
\*   crates/db/src/index/manager/deferred.rs  IndexManager::owe (asks on commit)
\*   crates/db/src/collection/ops/purge.rs    purge_documents (asks like any write)
\*   crates/db/src/definition/migration/helpers.rs cache_migrated_document_with_indexes
EXTENDS Naturals, FiniteSets

CONSTANTS
  Poison,        \* the index whose drain always fails; first in the task's order
  Hot,           \* the index a steady stream of writes keeps owing
  Quiet,         \* the index a purge and an update owe once each; its drain may fail once
  Interval,      \* how many ticks pass before the sweep is due
  Isolate,       \* "On": a failing index leaves the rest of the pass alone | "Off": the pass stops there
  KeepFailed,    \* "On": a failed index is kept for the next sweep | "Off": its request is dropped
  Requeue,       \* "On": an index with work left after its turn asks again | "Off": it does not
  PurgeRequests, \* "On": a purge asks for a drain like any write | "Off": it only owes
  SweepTimer,    \* "Fixed": a request does not push the sweep back | "Rearm": every pass restarts it | "None": no sweep
  SweepFirst     \* "On": a due sweep wins over a stored request | "Off": either may win

\* The three indexes, in the order a pass visits them.
Indexes == {Poison, Hot, Quiet}

\* The poison index comes first, so a pass that stops at it skips the others.
ASSUME Poison < Hot /\ Hot < Quiet
\* The sweep needs at least two ticks, so a pass can come between them.
ASSUME Interval \in Nat /\ Interval >= 2
\* Each switch takes one of its values.
ASSUME Isolate \in {"On", "Off"}
ASSUME KeepFailed \in {"On", "Off"}
ASSUME Requeue \in {"On", "Off"}
ASSUME PurgeRequests \in {"On", "Off"}
ASSUME SweepTimer \in {"Fixed", "Rearm", "None"}
ASSUME SweepFirst \in {"On", "Off"}

VARIABLES
  owed,      \* turns of work each index's log holds: 0, 1, or 2 (2 means "more than one turn")
  queue,     \* the indexes asked for that no pass has taken yet
  permit,    \* whether a wake is stored for the task (it holds at most one)
  failed,    \* the indexes whose last drain failed
  timer,     \* ticks since the sweep was armed
  pc,        \* the task: "Wait" (asleep) or "Pass" (running a pass)
  todo,      \* the indexes the running pass has still to visit
  sweeping,  \* whether the running pass is a sweep
  flaky,     \* whether the quiet index's one failure can still happen
  purged,    \* whether the purge happened
  updated,   \* whether the quiet index's update happened
  crashed    \* whether the one crash happened

\* Every variable, for stuttering and fairness.
vars == <<owed, queue, permit, failed, timer, pc, todo, sweeping, flaky, purged, updated, crashed>>

\* The first index of a set in the task's order.
First(S) == CHOOSE i \in S : \A j \in S : i <= j

\* The sweep is due: the timer ran out, if there is a sweep at all.
Due == SweepTimer /= "None" /\ timer = Interval

\* One more turn of work for index i, capped at 2.
Owe(i) == owed' = [owed EXCEPT ![i] = IF @ < 2 THEN @ + 1 ELSE @]

\* Every variable stays in its range.
TypeOK ==
  /\ owed \in [Indexes -> 0..2]          \* turns owed
  /\ queue \subseteq Indexes             \* requests
  /\ permit \in BOOLEAN                  \* the stored wake
  /\ failed \subseteq Indexes            \* failures
  /\ timer \in 0..Interval               \* the sweep timer
  /\ pc \in {"Wait", "Pass"}             \* the task's step
  /\ todo \subseteq Indexes              \* the pass's work
  /\ sweeping \in BOOLEAN                \* a sweep or not

\* The node opens with the poison index owing work, and settles: a sweep pass first.
Init ==
  /\ owed = [i \in Indexes |-> IF i = Poison THEN 1 ELSE 0]   \* only the poison index owes
  /\ queue = {}                          \* nobody asked yet
  /\ permit = FALSE                      \* no wake stored
  /\ failed = {}                         \* nothing failed yet
  /\ timer = 0                           \* the sweep was just armed
  /\ pc = "Pass"                         \* the settle at open runs first
  /\ todo = Indexes                      \* it visits every index
  /\ sweeping = TRUE                     \* the settle is a sweep
  /\ flaky = TRUE                        \* the quiet index can still fail once
  /\ purged = FALSE                      \* no purge yet
  /\ updated = FALSE                     \* no update yet
  /\ crashed = FALSE                     \* no crash yet

\* A write to the hot index commits: it owes a turn and asks for a drain.
HotWrite ==
  /\ Owe(Hot)                            \* the log grows
  /\ queue' = queue \cup {Hot}           \* the commit asks for the index
  /\ permit' = TRUE                      \* and wakes the task
  /\ UNCHANGED <<failed, timer, pc, todo, sweeping, flaky, purged, updated, crashed>>

\* A purge on the quiet collection commits: it owes, and asks only if purges ask.
Purge ==
  /\ ~purged                             \* one purge is enough to explore
  /\ purged' = TRUE
  /\ Owe(Quiet)                          \* the log grows
  /\ IF PurgeRequests = "On"
     THEN queue' = queue \cup {Quiet} /\ permit' = TRUE   \* asks like any write
     ELSE UNCHANGED <<queue, permit>>                     \* owes in silence
  /\ UNCHANGED <<failed, timer, pc, todo, sweeping, flaky, updated, crashed>>

\* An update on the quiet collection commits: it owes and asks.
QuietUpdate ==
  /\ ~updated                            \* one update is enough to explore
  /\ updated' = TRUE
  /\ Owe(Quiet)                          \* the log grows
  /\ queue' = queue \cup {Quiet}         \* the commit asks for the index
  /\ permit' = TRUE                      \* and wakes the task
  /\ UNCHANGED <<failed, timer, pc, todo, sweeping, flaky, purged, crashed>>

\* Time passes toward the sweep.
Tick ==
  /\ SweepTimer /= "None"                \* there is a timer to run
  /\ timer < Interval                    \* it has not run out
  /\ timer' = timer + 1
  /\ UNCHANGED <<owed, queue, permit, failed, pc, todo, sweeping, flaky, purged, updated, crashed>>

\* The task wakes for the sweep: every index, and the requests too.
WakeSweep ==
  /\ pc = "Wait"                         \* asleep
  /\ Due                                 \* the sweep is due
  /\ pc' = "Pass" /\ sweeping' = TRUE    \* a sweep pass starts
  /\ todo' = Indexes                     \* it visits every index
  /\ queue' = {}                         \* it takes the requests with it
  /\ timer' = 0                          \* the sweep is armed again
  /\ UNCHANGED <<owed, permit, failed, flaky, purged, updated, crashed>>

\* The task wakes for a request and takes every index asked for.
WakeRequest ==
  /\ pc = "Wait"                         \* asleep
  /\ permit                              \* a wake is stored
  /\ SweepFirst = "On" => ~Due           \* a due sweep goes first when the guard is on
  /\ pc' = "Pass" /\ sweeping' = FALSE   \* a request pass starts
  /\ todo' = queue                       \* it visits what was asked for
  /\ queue' = {} /\ permit' = FALSE      \* the requests and the wake are taken
  /\ timer' = IF SweepTimer = "Rearm" THEN 0 ELSE timer   \* the bug: a pass pushes the sweep back
  /\ UNCHANGED <<owed, failed, flaky, purged, updated, crashed>>

\* The pass visits its next index.
Visit ==
  /\ pc = "Pass" /\ todo /= {}           \* a pass with work left
  /\ LET i == First(todo) IN             \* the next index in order
     \/ /\ ~sweeping /\ i \in failed     \* a failed index waits for the sweep
        /\ queue' = queue \cup {i}       \* it is kept, without a wake
        /\ todo' = todo \ {i}            \* the pass moves on
        /\ UNCHANGED <<owed, permit, failed, flaky>>
     \/ /\ sweeping \/ i \notin failed   \* its turn runs
        /\ owed[i] = 0                   \* and finds nothing owed
        /\ failed' = failed \ {i}        \* that counts as a success
        /\ todo' = todo \ {i}            \* the pass moves on
        /\ UNCHANGED <<owed, queue, permit, flaky>>
     \/ /\ sweeping \/ i \notin failed   \* its turn runs
        /\ owed[i] > 0                   \* with work to do
        /\ i = Poison \/ (i = Quiet /\ flaky)   \* and fails
        /\ flaky' = IF i = Quiet THEN FALSE ELSE flaky   \* the quiet index fails once only
        /\ failed' = failed \cup {i}     \* the failure is remembered
        /\ queue' = IF KeepFailed = "On" THEN queue \cup {i} ELSE queue   \* kept for later, without a wake
        /\ todo' = IF Isolate = "On" THEN todo \ {i} ELSE {}   \* the bug: the rest of the pass is dropped
        /\ UNCHANGED <<owed, permit>>
     \/ /\ sweeping \/ i \notin failed   \* its turn runs
        /\ owed[i] > 0 /\ i /= Poison    \* with work it can do
        /\ owed' = [owed EXCEPT ![i] = @ - 1]   \* one turn of work is done
        /\ failed' = failed \ {i}        \* it drains again
        /\ IF owed[i] > 1 /\ Requeue = "On"
           THEN queue' = queue \cup {i} /\ permit' = TRUE   \* more left: it asks again, after the others
           ELSE UNCHANGED <<queue, permit>>
        /\ todo' = todo \ {i}            \* the pass moves on
        /\ UNCHANGED flaky
  /\ UNCHANGED <<timer, pc, sweeping, purged, updated, crashed>>

\* The pass is over and the task goes back to sleep.
PassEnd ==
  /\ pc = "Pass" /\ todo = {}            \* nothing left to visit
  /\ pc' = "Wait" /\ sweeping' = FALSE   \* asleep again
  /\ UNCHANGED <<owed, queue, permit, failed, timer, todo, flaky, purged, updated, crashed>>

\* A crash forgets the requests, the wake and the failures; the logs stay.
\* The reopened node settles with a sweep pass.
Crash ==
  /\ ~crashed                            \* one crash is enough to explore
  /\ crashed' = TRUE
  /\ queue' = {} /\ permit' = FALSE      \* requests lived in memory only
  /\ failed' = {}                        \* so did the failures
  /\ pc' = "Pass" /\ todo' = Indexes /\ sweeping' = TRUE   \* the settle at open
  /\ timer' = 0                          \* a fresh sweep timer
  /\ UNCHANGED <<owed, flaky, purged, updated>>

\* Any one step happens next.
Next ==
  \/ HotWrite \/ Purge \/ QuietUpdate \/ Tick
  \/ WakeSweep \/ WakeRequest \/ Visit \/ PassEnd \/ Crash

\* Time passes, the task keeps running when it can, and the hot index is written forever.
Fairness ==
  /\ WF_vars(Tick) /\ WF_vars(WakeSweep) /\ WF_vars(WakeRequest)
  /\ WF_vars(Visit) /\ WF_vars(PassEnd) /\ WF_vars(HotWrite)

\* Start in Init, take any steps, under the fairness above.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every value stays in its range.
INV_TypeOK == TypeOK

\* Work an index owes is always asked for, or in the pass now running.
INV_OwedKept == \A i \in Indexes : owed[i] > 0 => i \in queue \cup todo

\* The quiet index ends up owing nothing, however busy the hot index keeps the task.
PROP_QuietSettles == <>[](owed[Quiet] = 0)
====
