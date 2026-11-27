---- MODULE WorkerHeartbeat ----
\* The story, in plain words.
\*
\* A worker thread takes turns: it polls one of its tasks, then its I/O hook
\* (the regolith IoQueue, which reads blocks and finishes commits for this
\* thread), then a task again, and so on. It shows which poll it is in on a
\* heartbeat: a task poll with the time it started, a hook poll by its
\* number alone, so the worker reads no clock for it. When a task poll
\* returns, the heartbeat moves straight on to the hook poll that follows,
\* in one store. The hook poll stays shown until the next task poll or the
\* end of the turn replaces it.
\*
\* Two monitors on other threads (the code runs monitor::WATCHERS = 2 of
\* them, so one always watches the other) look at the heartbeat now and
\* then, each on its own, so their steps interleave. A task poll is timed
\* from its start; a hook poll from when a monitor first saw it. When a
\* poll has run past the limit a monitor marks the worker stuck, naming
\* that poll, so no new work is routed to the worker, and reports it by the
\* name the heartbeat gives. The read, the mark, a second look (the
\* recheck) and the claim of the report are separate steps: the worker and
\* the other monitor can move between any two. After the recheck, a mark
\* for a poll no longer shown is taken back; each scan also sweeps such a
\* mark first. A mark that names the poll shown now is never replaced by a
\* slower monitor's mark for an older poll. A hook poll's report is claimed
\* only if no later hook poll was claimed already, so a slow monitor never
\* reports an older hook poll a second time.
\*
\* Once the heartbeat has left a poll, the worker looks for a mark naming
\* that very poll: a task poll that was marked is remembered as reported, a
\* hook poll's mark is simply taken off.
\*
\* The bug that was on main: the hook showed nothing of its own and the
\* task's heartbeat stayed up until the end of the turn. A slow hook poll
\* after a short task was reported under the task's name, and its mark was
\* taken by the task's next poll, which was then remembered as stalled. The
\* mark was also a bare flag, so a mark the monitor raised just after a poll
\* ended could be taken by the next poll.
\*
\* What must never happen:
\*   1. a monitor reports a poll that is not the one shown and running, or
\*      one not really past the limit - INV_ReportTrue;
\*   2. a task is remembered as reported for a poll of its own that did not
\*      run past the limit - INV_MarkTrue;
\*   3. with both monitors at rest, a mark stands for a poll that is
\*      neither shown nor waiting for the worker's look, so the worker is
\*      routed around for nothing - INV_NotLeftStuck;
\*   4. a poll that was marked while shown loses its mark while it is still
\*      shown, so new work is routed to a stuck worker - INV_KeepsMark;
\*   5. one task poll is reported twice - INV_TaskOnce;
\*   6. one hook poll is reported twice - INV_HookOnce.
\*
\* What the model leaves out: the worker's look for its mark is a plain load
\* in the code, so on a weakly ordered machine it can miss a mark raised in
\* the same instant; the model's steps are sequentially consistent, and the
\* scan's sweep (WorkerShared::sweep_mark) is what takes such a missed mark
\* off in the code. A hook poll the worker makes on its own (with no task
\* before it, or while spinning idle) takes the heartbeat over from idle in
\* the same way and is not modeled apart; the workers unit tests
\* a_watched_slow_hook_with_no_task_before_it_is_found and its unwatched
\* twin (pool/hook_tests.rs) pin that path. A pool given a MonitorDriver
\* runs one scan at a time, which is a behaviour of this model where the
\* second monitor never moves.
\*
\* The Rust code each guard models (crates/foundation/workers/src):
\*   HookBeat        the task poll hands the heartbeat to the hook poll as it
\*                   returns, and the hook poll stays shown until replaced
\*                   (worker/local.rs Local::poll; worker/hook_beat.rs
\*                   Local::begin_hook, Local::close_hook, Local::end_beat;
\*                   worker/heartbeat.rs Heartbeat::begin_hook);
\*   MarkNamesPoll   the mark holds the word of the poll it is for, and the
\*                   worker takes only its own poll's mark
\*                   (worker/shared.rs WorkerShared::mark_stuck,
\*                   WorkerShared::unmark; worker/local.rs Local::after_poll);
\*   HookClears      the worker takes a hook poll's mark off once the
\*                   heartbeat has left it (worker/hook_beat.rs
\*                   Local::close_hook);
\*   Recheck         the monitor's second look after its mark
\*                   (pool/monitor.rs scan);
\*   MarkKeepsLive   a monitor's mark never replaces a mark naming the poll
\*                   shown now (worker/shared.rs WorkerShared::mark_stuck);
\*   ClaimMax        a task or hook poll's report is claimed by raising the
\*                   last claimed poll's word to its own, and only a raise
\*                   reports (pool/monitor.rs scan, the fetch_max on
\*                   WorkerShared::reported and WorkerShared::hook_reported).
\* The monitor's sighting of a hook poll is WorkerShared::hook_elapsed, and
\* its sweep WorkerShared::sweep_mark.

EXTENDS Naturals, TLC

\* Limit:    how long a poll may run before it counts as stuck.
\* MaxTime:  the last tick of the clock in one behaviour.
\* MaxPolls: how many polls the worker makes in one behaviour; even ones
\*           are task polls, odd ones hook polls.
\* Watchers: the monitors that scan at once, as names.
\* HookBeat, MarkNamesPoll, HookClears, Recheck, MarkKeepsLive, ClaimMax:
\* the guards above; a RED configuration turns one or more off.
CONSTANTS Limit, MaxTime, MaxPolls, Watchers, HookBeat, MarkNamesPoll,
          HookClears, Recheck, MarkKeepsLive, ClaimMax

\* time:        the pool's clock.
\* done:        how many polls have finished.
\* wpc:         where the worker is: "between" polls, "running" one, or
\*              "after" a task poll, looking for its mark.
\* runStart:    when the current poll started.
\* ranFor:      how long the last task poll ran, once it returned.
\* closing:     the worker still owes a look for the open hook poll's mark.
\* openHook:    the hook poll the heartbeat shows, not yet looked at, or None.
\* beatOn:      the heartbeat shows a poll.
\* beatPoll:    which poll it shows.
\* beatStart:   when a shown task poll started.
\* beatSince:   when the heartbeat began to show that poll.
\* mark:        the monitors' one shared mark: the poll it names, or None.
\* mpc:         for each monitor, where it is: at "rest", having "read" a
\*              long poll, having "marked" it and not yet rechecked, or
\*              "checked" and about to claim its report.
\* mPoll:       for each monitor, the poll it read.
\* sightPoll:   the hook poll the monitors last saw, or None.
\* sightAt:     when they first saw it.
\* hookRep:     the hook poll whose report was last claimed, 0 for none
\*              (poll 0 is a task, so no hook poll is 0).
\* hookDone:    every hook poll reported so far.
\* taskRep:     the task poll whose report was last claimed, None for none.
\* taskDone:    every task poll reported so far.
\* held:        the poll a mark last landed on while that poll was shown,
\*              or None: bookkeeping for INV_KeepsMark, not in the code.
\* badReport:   a monitor reported something that was not true.
\* badMark:     a short task poll was remembered as reported.
\* badTaskTwice: a task poll was reported a second time.
\* badTwice:    a hook poll was reported a second time.
VARIABLES time, done, wpc, runStart, ranFor, closing, openHook, beatOn,
          beatPoll, beatStart, beatSince, mark, mpc, mPoll, sightPoll,
          sightAt, hookRep, hookDone, taskRep, taskDone, held, badReport,
          badMark, badTaskTwice, badTwice

vars == <<time, done, wpc, runStart, ranFor, closing, openHook, beatOn,
          beatPoll, beatStart, beatSince, mark, mpc, mPoll, sightPoll,
          sightAt, hookRep, hookDone, taskRep, taskDone, held, badReport,
          badMark, badTaskTwice, badTwice>>

\* The monitors' variables, which the worker's and the clock's steps leave
\* alone.
monVars == <<mpc, mPoll, sightPoll, sightAt, hookRep, hookDone, taskRep,
             taskDone, held, badReport, badTaskTwice, badTwice>>

\* The monitors.
Mon == Watchers

\* The monitors are alike, so a state and the same state with the two
\* swapped are checked once.
MonSymmetry == Permutations(Mon)

\* No poll: a number no poll has.
None == MaxPolls + 1

\* What poll number n polls: the task, then the hook, in turn.
Kind(n) == IF n % 2 = 0 THEN "task" ELSE "hook"

\* Every hook poll number.
HookPolls == {n \in 0..MaxPolls : Kind(n) = "hook"}

\* Every task poll number.
TaskPolls == {n \in 0..MaxPolls : Kind(n) = "task"}

\* The larger of a and b.
Max(a, b) == IF a >= b THEN a ELSE b

\* Every value each variable may take.
TypeOK ==
    /\ time \in 0..MaxTime                           \* a tick of the clock
    /\ done \in 0..MaxPolls                          \* polls finished
    /\ wpc \in {"between", "running", "after"}       \* a worker step
    /\ runStart \in 0..MaxTime                       \* a start time
    /\ ranFor \in 0..MaxTime                         \* a duration
    /\ closing \in BOOLEAN                           \* a look owed
    /\ openHook \in 0..None                          \* a hook poll or None
    /\ beatOn \in BOOLEAN                            \* a poll shown or not
    /\ beatPoll \in 0..MaxPolls                      \* the poll shown
    /\ beatStart \in 0..MaxTime                      \* a start time
    /\ beatSince \in 0..MaxTime                      \* a time
    /\ mark \in 0..None                              \* a poll or None
    /\ mpc \in [Mon -> {"rest", "read", "marked", "checked"}] \* each monitor's step
    /\ mPoll \in [Mon -> 0..MaxPolls]                \* each monitor's poll read
    /\ sightPoll \in 0..None                         \* a hook poll or None
    /\ sightAt \in 0..MaxTime                        \* a time
    /\ hookRep \in 0..MaxPolls                       \* a hook poll, or 0
    /\ hookDone \subseteq HookPolls                  \* hook polls reported
    /\ taskRep \in 0..None                           \* a task poll, or None
    /\ taskDone \subseteq TaskPolls                  \* task polls reported
    /\ held \in 0..None                              \* a poll or None
    /\ badReport \in BOOLEAN                         \* a false report seen
    /\ badMark \in BOOLEAN                           \* a false mark taken
    /\ badTaskTwice \in BOOLEAN                      \* a task double report seen
    /\ badTwice \in BOOLEAN                          \* a double report seen

\* At first nothing runs, nothing is shown, and nothing is marked.
Init ==
    /\ time = 0                 \* the clock starts
    /\ done = 0                 \* no poll yet
    /\ wpc = "between"          \* the worker is between polls
    /\ runStart = 0             \* no poll started
    /\ ranFor = 0               \* no poll ran
    /\ closing = FALSE          \* no look owed
    /\ openHook = None          \* no hook poll open
    /\ beatOn = FALSE           \* the heartbeat shows nothing
    /\ beatPoll = 0             \* an unused default
    /\ beatStart = 0            \* an unused default
    /\ beatSince = 0            \* an unused default
    /\ mark = None              \* no mark
    /\ mpc = [m \in Mon |-> "rest"] \* every monitor waits
    /\ mPoll = [m \in Mon |-> 0]    \* nothing read
    /\ sightPoll = None         \* no hook poll seen
    /\ sightAt = 0              \* an unused default
    /\ hookRep = 0              \* no hook poll claimed
    /\ hookDone = {}            \* no hook poll reported
    /\ taskRep = None           \* no task poll claimed
    /\ taskDone = {}            \* no task poll reported
    /\ held = None              \* no mark landed yet
    /\ badReport = FALSE        \* nothing reported yet
    /\ badMark = FALSE          \* nothing marked yet
    /\ badTaskTwice = FALSE     \* nothing task-reported twice yet
    /\ badTwice = FALSE         \* nothing reported twice yet

\* Whether a look for poll n's mark finds one to take: with MarkNamesPoll
\* only the mark naming n, on main any mark at all.
Takes(n) == IF MarkNamesPoll THEN mark = n ELSE mark # None

\* The heartbeat shows poll n from now on.
Show(n) ==
    /\ beatOn' = TRUE           \* a poll is shown
    /\ beatPoll' = n            \* this one
    /\ beatSince' = time        \* since now

\* Time passes, wherever the worker and the monitor are.
Tick ==
    /\ time < MaxTime                                   \* the clock has room
    /\ time' = time + 1                                 \* one tick on
    /\ UNCHANGED <<done, wpc, runStart, ranFor, closing, openHook, beatOn,
                   beatPoll, beatStart, beatSince, mark, badMark,
                   monVars>>                             \* nothing else moves

\* A task poll starts: the heartbeat shows it with its start, replacing an
\* open hook poll, whose mark the worker then owes a look for.
BeginTask ==
    /\ wpc = "between" /\ ~closing                      \* free to start
    /\ done < MaxPolls /\ Kind(done) = "task"           \* a task poll is due
    /\ Show(done)                                       \* it is shown
    /\ beatStart' = time                                \* starting now
    /\ runStart' = time                                 \* it runs from now
    /\ wpc' = "running"                                 \* it runs
    /\ closing' = (openHook # None)                     \* a hook poll to look at
    /\ UNCHANGED <<time, done, ranFor, openHook, mark, badMark,
                   monVars>>                             \* nothing else moves

\* The hook poll starts. With HookBeat the task poll already showed it.
BeginHook ==
    /\ wpc = "between" /\ ~closing                      \* free to start
    /\ done < MaxPolls /\ Kind(done) = "hook"           \* the hook is due
    /\ runStart' = time                                 \* it runs from now
    /\ wpc' = "running"                                 \* it runs
    /\ UNCHANGED <<time, done, ranFor, closing, openHook, beatOn, beatPoll,
                   beatStart, beatSince, mark, badMark,
                   monVars>>                             \* nothing else moves

\* The worker looks for the open hook poll's mark, now that the heartbeat
\* has left it, and takes it off (only with HookClears).
Close ==
    /\ closing                                          \* a look is owed
    /\ IF HookClears /\ Takes(openHook)                 \* a mark to take off
       THEN mark' = None                                \* it comes down
       ELSE UNCHANGED mark                              \* it stays as it is
    /\ closing' = FALSE                                 \* the look is done
    /\ openHook' = None                                 \* no hook poll open
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, beatOn, beatPoll,
                   beatStart, beatSince, badMark,
                   monVars>>                             \* nothing else moves

\* A task poll returns. With HookBeat the heartbeat moves straight on to the
\* hook poll that follows (or goes idle if none does); on main it stays.
EndTask ==
    /\ wpc = "running" /\ Kind(done) = "task" /\ ~closing \* a task poll runs
    /\ wpc' = "after"                                   \* it has returned
    /\ ranFor' = time - runStart                        \* this long
    /\ IF ~HookBeat                                     \* main
       THEN UNCHANGED <<beatOn, beatPoll, beatSince, openHook>> \* the task stays shown
       ELSE IF done + 1 < MaxPolls                      \* a hook poll follows
            THEN /\ Show(done + 1)                      \* it is shown now
                 /\ openHook' = done + 1                \* and open
            ELSE /\ beatOn' = FALSE                     \* nothing follows: idle
                 /\ UNCHANGED <<beatPoll, beatSince, openHook>> \* nothing open
    /\ UNCHANGED <<time, done, runStart, closing, beatStart, mark, badMark,
                   monVars>>                             \* nothing else moves

\* The worker looks for the returned task poll's mark. Taking one, the task
\* is remembered as reported: a false memory if that poll was short.
AfterTask ==
    /\ wpc = "after"                                    \* a task poll returned
    /\ IF Takes(done)                                   \* a mark to take
       THEN mark' = None                                \* it comes down
       ELSE UNCHANGED mark                              \* none for it
    /\ badMark' = (badMark \/ (Takes(done) /\ ranFor <= Limit)) \* a short task blamed
    /\ done' = done + 1                                 \* one more poll done
    /\ wpc' = "between"                                 \* between polls
    /\ UNCHANGED <<time, runStart, ranFor, closing, openHook, beatOn,
                   beatPoll, beatStart, beatSince,
                   monVars>>                             \* nothing else moves

\* The hook poll returns. With HookBeat it stays shown until the next task
\* poll or the end of the turn; on main the turn ends and the heartbeat
\* goes idle.
EndHook ==
    /\ wpc = "running" /\ Kind(done) = "hook"           \* the hook runs
    /\ done' = done + 1                                 \* one more poll done
    /\ wpc' = "between"                                 \* between polls
    /\ IF HookBeat                                      \* the fix
       THEN UNCHANGED beatOn                            \* still shown
       ELSE beatOn' = FALSE                             \* main: the turn ends
    /\ UNCHANGED <<time, runStart, ranFor, closing, openHook, beatPoll,
                   beatStart, beatSince, mark, badMark,
                   monVars>>                             \* nothing else moves

\* The turn ends with a hook poll open: the heartbeat goes idle and the
\* worker owes a look for that poll's mark.
TurnEnd ==
    /\ wpc = "between" /\ ~closing /\ openHook # None   \* a hook poll still shown
    /\ beatOn' = FALSE                                  \* idle
    /\ closing' = TRUE                                  \* a look is owed
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, openHook, beatPoll,
                   beatStart, beatSince, mark, badMark,
                   monVars>>                             \* nothing else moves

\* The worker's variables, which a monitor's steps leave alone.
workerVars == <<time, done, wpc, runStart, ranFor, closing, openHook, beatOn,
                beatPoll, beatStart, beatSince, badMark>>

\* Monitor m moves from step a to step b and keeps its read.
Step(m, a, b) ==
    /\ mpc[m] = a                                       \* m is at step a
    /\ mpc' = [mpc EXCEPT ![m] = b]                     \* and moves to b

\* Whether a mark names the poll the heartbeat shows now.
MarkLive == mark # None /\ beatOn /\ beatPoll = mark

\* Monitor m's scan starts by taking off a mark for a poll the heartbeat no
\* longer shows. In the code that is a load, a look at the heartbeat and a
\* compare-and-swap; it is one step here because a poll that is no longer
\* shown is never shown again, so nothing between them changes the answer.
Sweep(m) ==
    /\ mpc[m] = "rest" /\ mark # None                   \* a mark, m free
    /\ ~MarkLive                                        \* its poll is not shown
    /\ mark' = None                                     \* it comes down
    /\ UNCHANGED <<workerVars, mpc, mPoll, sightPoll, sightAt, hookRep,
                   hookDone, taskRep, taskDone, held, badReport,
                   badTaskTwice, badTwice>>  \* nothing else moves

\* Monitor m first sees a hook poll: it notes when, and reports nothing. In
\* the code this is one compare-and-swap on the shared sighting, so both
\* monitors agree on it.
Sight(m) ==
    /\ mpc[m] = "rest" /\ beatOn /\ Kind(beatPoll) = "hook" \* a hook poll shown
    /\ sightPoll # beatPoll                             \* not seen before
    /\ sightPoll' = beatPoll                            \* seen now
    /\ sightAt' = time                                  \* at this time
    /\ UNCHANGED <<workerVars, mark, mpc, mPoll, hookRep, hookDone, held,
                   taskRep, taskDone, badReport, badTaskTwice,
                   badTwice>>                             \* nothing else moves

\* How long the monitors have seen the shown poll run: a task poll from its
\* start, a hook poll from its first sighting (0 when not yet seen).
Elapsed ==
    IF Kind(beatPoll) = "task" THEN time - beatStart
    ELSE IF sightPoll = beatPoll THEN time - sightAt ELSE 0

\* Whether poll n is still the worker's: a task poll still running, or the
\* open hook poll.
Live(n) ==
    IF Kind(n) = "task" THEN wpc = "running" /\ done = n
    ELSE openHook = n

\* Monitor m reads a poll past the limit; it will report it under the name
\* the heartbeat shows. The report is false unless that poll is still the
\* worker's and has been shown for longer than the limit.
Read(m) ==
    /\ mpc[m] = "rest" /\ beatOn /\ Elapsed > Limit     \* a poll shown past the limit
    /\ mpc' = [mpc EXCEPT ![m] = "read"]                \* m has read it
    /\ mPoll' = [mPoll EXCEPT ![m] = beatPoll]          \* remembering which
    /\ badReport' = (badReport \/ ~(Live(beatPoll)      \* false unless it is live
                                    /\ time - beatSince > Limit)) \* and long
    /\ UNCHANGED <<workerVars, mark, sightPoll, sightAt, hookRep, hookDone,
                   taskRep, taskDone, held, badTaskTwice,
                   badTwice>>                             \* nothing else moves

\* Whether m's mark lands: with MarkKeepsLive it never replaces a mark that
\* names the poll shown now (another monitor's mark for a newer poll than
\* the one m read); without it, it always lands. In the code a load, a look
\* at the heartbeat and a compare-and-swap that retries; one step here,
\* because a mark that is not live never becomes live again and the
\* compare-and-swap fails if the mark moved in between.
Lands(m) == ~MarkKeepsLive \/ ~MarkLive \/ mark = mPoll[m]

\* Monitor m marks the worker stuck, naming the poll it read; without its
\* recheck it goes straight on to its report.
Mark(m) ==
    /\ Step(m, "read", IF Recheck THEN "marked" ELSE "checked") \* recheck next, or report
    /\ IF Lands(m)                                      \* the mark lands
       THEN /\ mark' = mPoll[m]                         \* it names m's poll
            /\ held' = IF beatOn /\ beatPoll = mPoll[m] \* that poll is shown
                       THEN mPoll[m]                    \* it holds a live mark now
                       ELSE held                        \* bookkeeping unchanged
       ELSE UNCHANGED <<mark, held>>                    \* a newer poll's mark stays
    /\ UNCHANGED <<workerVars, mPoll, sightPoll, sightAt, hookRep, hookDone,
                   taskRep, taskDone, badReport, badTaskTwice,
                   badTwice>>                             \* nothing else moves

\* Monitor m looks again: if the poll it marked is no longer shown, it
\* takes its mark back (with MarkNamesPoll only that poll's mark).
RecheckStep(m) ==
    /\ Step(m, "marked", "checked")                     \* m marked a poll
    /\ IF beatOn /\ beatPoll = mPoll[m]                 \* still shown
       THEN UNCHANGED mark                              \* the mark stands
       ELSE mark' = IF ~MarkNamesPoll \/ mark = mPoll[m] \* its own, or any on main
                    THEN None                           \* comes down
                    ELSE mark                           \* another poll's stays
    /\ UNCHANGED <<workerVars, mPoll, sightPoll, sightAt, hookRep, hookDone,
                   taskRep, taskDone, held, badReport, badTaskTwice,
                   badTwice>>                             \* nothing else moves

\* Whether m may claim the report of hook poll n: with ClaimMax only when no
\* hook poll as late as n was claimed (a fetch_max that raised the word);
\* without it whenever the last claim was for another poll (a swap).
Claims(n) == IF ClaimMax THEN hookRep < n ELSE hookRep # n

\* Whether m may claim the report of task poll n: with ClaimMax only when no
\* task poll as late as n was claimed (a fetch_max that raised the word);
\* without it only when the last claim was for another poll (a swap).
TaskClaims(n) == IF ClaimMax THEN taskRep = None \/ taskRep < n ELSE taskRep # n

\* Monitor m claims its report and goes back to rest. A task poll's report
\* is checked for truth at Read; a hook poll is reported only if m claims
\* it, and reporting one already reported is the double report. A task or
\* hook report is claimed the same way. A failed claim writes nothing new:
\* a fetch_max keeps the larger word, and a swap that fails put back the
\* same word.
Report(m) ==
    /\ Step(m, "checked", "rest")                       \* m is about to report
    /\ IF Kind(mPoll[m]) = "task"                       \* m has a task poll
       THEN IF TaskClaims(mPoll[m])                     \* m claims it
            THEN /\ badTaskTwice' = (badTaskTwice \/ mPoll[m] \in taskDone)
                 /\ taskDone' = taskDone \cup {mPoll[m]} \* reported now
                 /\ taskRep' = mPoll[m]                    \* the claim's word
                 /\ UNCHANGED <<hookDone, hookRep, badTwice>> \* no hook claim
            ELSE /\ UNCHANGED <<badTaskTwice, taskDone, taskRep,
                                 hookDone, hookRep, badTwice>> \* nothing claimed
       ELSE IF Claims(mPoll[m])                         \* m claims a hook poll
            THEN /\ badTwice' = (badTwice \/ mPoll[m] \in hookDone)
                 /\ hookDone' = hookDone \cup {mPoll[m]}
                 /\ hookRep' = IF ClaimMax
                               THEN Max(hookRep, mPoll[m])
                               ELSE mPoll[m]
                 /\ UNCHANGED <<badTaskTwice, taskDone, taskRep>>
            ELSE /\ UNCHANGED <<badTwice, hookDone, hookRep, badTaskTwice,
                                 taskDone, taskRep>>
    /\ UNCHANGED <<workerVars, mark, mPoll, sightPoll, sightAt, held,
                   badReport>>                            \* nothing else moves

\* Every monitor is at rest.
AllRest == \A m \in Mon : mpc[m] = "rest"

\* Every poll is done, the clock has stopped and the monitors rest:
\* nothing more happens.
Finished ==
    /\ done = MaxPolls /\ time = MaxTime /\ AllRest /\ ~closing \* all is over
    /\ UNCHANGED vars                                   \* and stays so

\* One step of the worker, the clock, or one of the monitors.
Next == \/ Tick \/ BeginTask \/ BeginHook \/ Close \/ EndTask \/ AfterTask
        \/ EndHook \/ TurnEnd
        \/ \E m \in Mon : \/ Sweep(m) \/ Sight(m) \/ Read(m) \/ Mark(m)
                          \/ RecheckStep(m) \/ Report(m)
        \/ Finished

\* Every behaviour starts at Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* The monitors only ever report a poll that is the worker's, past the limit.
INV_ReportTrue == ~badReport

\* A task is remembered as reported only for a poll of its own past the limit.
INV_MarkTrue == ~badMark

\* With every monitor at rest, no mark is left but for a poll shown or one
\* the worker will still look at.
INV_NotLeftStuck ==
    (AllRest /\ mark # None) =>
        \/ beatOn /\ beatPoll = mark                    \* its poll is shown
        \/ wpc = "after" /\ done = mark                 \* its task poll's look is owed
        \/ openHook = mark                              \* its hook poll's look is owed

\* A poll whose mark landed while it was shown keeps that mark for as long
\* as it is shown, so routing goes around the worker the whole time.
INV_KeepsMark ==
    (held # None /\ beatOn /\ beatPoll = held) => mark = held

\* No task poll is reported twice.
INV_TaskOnce == ~badTaskTwice

\* No hook poll is reported twice.
INV_HookOnce == ~badTwice

====
