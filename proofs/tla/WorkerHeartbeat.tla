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
\* A monitor on another thread looks at the heartbeat now and then. A task
\* poll is timed from its start; a hook poll from when the monitor first saw
\* it. When a poll has run past the limit the monitor reports it, by the
\* name the heartbeat gives, and marks the worker stuck, naming that poll,
\* so no new work is routed to the worker. The read, the mark, and a second
\* look (the recheck) are separate steps: the worker can move on between
\* any two. After the recheck, a mark for a poll no longer shown is taken
\* back; each scan also sweeps such a mark first.
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
\*   1. the monitor reports a poll that is not the one shown and running, or
\*      one not really past the limit - INV_ReportTrue;
\*   2. a task is remembered as reported for a poll of its own that did not
\*      run past the limit - INV_MarkTrue;
\*   3. with the monitor at rest, a mark stands for a poll that is neither
\*      shown nor waiting for the worker's look, so the worker is routed
\*      around for nothing - INV_NotLeftStuck.
\*
\* What the model leaves out: the worker's look for its mark is a plain load
\* in the code, so on a weakly ordered machine it can miss a mark raised in
\* the same instant; the model's steps are sequentially consistent, and the
\* scan's sweep (WorkerShared::sweep_mark) is what takes such a missed mark
\* off in the code. A hook poll the worker makes on its own (with no task
\* before it, or while spinning idle) takes the heartbeat over from idle in
\* the same way and is not modeled apart.
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
\*                   (pool/monitor.rs scan).
\* The monitor's sighting of a hook poll is WorkerShared::hook_elapsed, and
\* its sweep WorkerShared::sweep_mark.

EXTENDS Naturals

\* Limit:    how long a poll may run before it counts as stuck.
\* MaxTime:  the last tick of the clock in one behaviour.
\* MaxPolls: how many polls the worker makes in one behaviour; even ones
\*           are task polls, odd ones hook polls.
\* HookBeat, MarkNamesPoll, HookClears, Recheck: the guards above; a RED
\* configuration turns one or more off.
CONSTANTS Limit, MaxTime, MaxPolls, HookBeat, MarkNamesPoll, HookClears, Recheck

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
\* mark:        the monitor's mark: the poll it names, or None.
\* mpc:         where the monitor is: at "rest", having "read" a long poll,
\*              or having "marked" it and not yet rechecked.
\* mPoll:       the poll the monitor read.
\* sightPoll:   the hook poll the monitor last saw, or None.
\* sightAt:     when it first saw it.
\* badReport:   the monitor reported something that was not true.
\* badMark:     a short task poll was remembered as reported.
VARIABLES time, done, wpc, runStart, ranFor, closing, openHook, beatOn,
          beatPoll, beatStart, beatSince, mark, mpc, mPoll, sightPoll,
          sightAt, badReport, badMark

vars == <<time, done, wpc, runStart, ranFor, closing, openHook, beatOn,
          beatPoll, beatStart, beatSince, mark, mpc, mPoll, sightPoll,
          sightAt, badReport, badMark>>

\* No poll: a number no poll has.
None == MaxPolls + 1

\* What poll number n polls: the task, then the hook, in turn.
Kind(n) == IF n % 2 = 0 THEN "task" ELSE "hook"

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
    /\ mpc \in {"rest", "read", "marked"}            \* a monitor step
    /\ mPoll \in 0..MaxPolls                         \* the poll read
    /\ sightPoll \in 0..None                         \* a hook poll or None
    /\ sightAt \in 0..MaxTime                        \* a time
    /\ badReport \in BOOLEAN                         \* a false report seen
    /\ badMark \in BOOLEAN                           \* a false mark taken

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
    /\ mpc = "rest"             \* the monitor waits
    /\ mPoll = 0                \* nothing read
    /\ sightPoll = None         \* no hook poll seen
    /\ sightAt = 0              \* an unused default
    /\ badReport = FALSE        \* nothing reported yet
    /\ badMark = FALSE          \* nothing marked yet

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
                   beatPoll, beatStart, beatSince, mark, mpc, mPoll,
                   sightPoll, sightAt, badReport, badMark>> \* nothing else moves

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
    /\ UNCHANGED <<time, done, ranFor, openHook, mark, mpc, mPoll,
                   sightPoll, sightAt, badReport, badMark>> \* nothing else moves

\* The hook poll starts. With HookBeat the task poll already showed it.
BeginHook ==
    /\ wpc = "between" /\ ~closing                      \* free to start
    /\ done < MaxPolls /\ Kind(done) = "hook"           \* the hook is due
    /\ runStart' = time                                 \* it runs from now
    /\ wpc' = "running"                                 \* it runs
    /\ UNCHANGED <<time, done, ranFor, closing, openHook, beatOn, beatPoll,
                   beatStart, beatSince, mark, mpc, mPoll, sightPoll,
                   sightAt, badReport, badMark>>          \* nothing else moves

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
                   beatStart, beatSince, mpc, mPoll, sightPoll, sightAt,
                   badReport, badMark>>                   \* nothing else moves

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
    /\ UNCHANGED <<time, done, runStart, closing, beatStart, mark, mpc, mPoll,
                   sightPoll, sightAt, badReport, badMark>> \* nothing else moves

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
                   beatPoll, beatStart, beatSince, mpc, mPoll, sightPoll,
                   sightAt, badReport>>                   \* nothing else moves

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
                   beatStart, beatSince, mark, mpc, mPoll, sightPoll,
                   sightAt, badReport, badMark>>          \* nothing else moves

\* The turn ends with a hook poll open: the heartbeat goes idle and the
\* worker owes a look for that poll's mark.
TurnEnd ==
    /\ wpc = "between" /\ ~closing /\ openHook # None   \* a hook poll still shown
    /\ beatOn' = FALSE                                  \* idle
    /\ closing' = TRUE                                  \* a look is owed
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, openHook, beatPoll,
                   beatStart, beatSince, mark, mpc, mPoll, sightPoll,
                   sightAt, badReport, badMark>>          \* nothing else moves

\* The scan starts by taking off a mark for a poll the heartbeat no longer
\* shows.
Sweep ==
    /\ mpc = "rest" /\ mark # None                      \* a mark, the monitor free
    /\ ~(beatOn /\ beatPoll = mark)                     \* its poll is not shown
    /\ mark' = None                                     \* it comes down
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, closing, openHook,
                   beatOn, beatPoll, beatStart, beatSince, mpc, mPoll,
                   sightPoll, sightAt, badReport, badMark>> \* nothing else moves

\* The monitor first sees a hook poll: it notes when, and reports nothing.
Sight ==
    /\ mpc = "rest" /\ beatOn /\ Kind(beatPoll) = "hook" \* a hook poll shown
    /\ sightPoll # beatPoll                             \* not seen before
    /\ sightPoll' = beatPoll                            \* seen now
    /\ sightAt' = time                                  \* at this time
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, closing, openHook,
                   beatOn, beatPoll, beatStart, beatSince, mark, mpc, mPoll,
                   badReport, badMark>>                   \* nothing else moves

\* How long the monitor has seen the shown poll run: a task poll from its
\* start, a hook poll from its first sighting (0 when not yet seen).
Elapsed ==
    IF Kind(beatPoll) = "task" THEN time - beatStart
    ELSE IF sightPoll = beatPoll THEN time - sightAt ELSE 0

\* Whether poll n is still the worker's: a task poll still running, or the
\* open hook poll.
Live(n) ==
    IF Kind(n) = "task" THEN wpc = "running" /\ done = n
    ELSE openHook = n

\* The monitor reads a poll past the limit and reports it under the name the
\* heartbeat shows. The report is false unless that poll is still the
\* worker's and has been shown for longer than the limit.
Read ==
    /\ mpc = "rest" /\ beatOn /\ Elapsed > Limit        \* a poll shown past the limit
    /\ mpc' = "read"                                    \* it has read it
    /\ mPoll' = beatPoll                                \* remembering which
    /\ badReport' = (badReport \/ ~(Live(beatPoll)      \* false unless it is live
                                    /\ time - beatSince > Limit)) \* and long
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, closing, openHook,
                   beatOn, beatPoll, beatStart, beatSince, mark, sightPoll,
                   sightAt, badMark>>                     \* nothing else moves

\* The monitor marks the worker stuck, naming the poll it read; without its
\* recheck it is done.
Mark ==
    /\ mpc = "read"                                     \* it read a long poll
    /\ mark' = mPoll                                    \* the mark names it
    /\ mpc' = IF Recheck THEN "marked" ELSE "rest"      \* recheck next, or rest
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, closing, openHook,
                   beatOn, beatPoll, beatStart, beatSince, mPoll, sightPoll,
                   sightAt, badReport, badMark>>          \* nothing else moves

\* The monitor looks again: if the poll it marked is no longer shown, it
\* takes its mark back (with MarkNamesPoll only that poll's mark).
RecheckStep ==
    /\ mpc = "marked"                                   \* it marked a poll
    /\ IF beatOn /\ beatPoll = mPoll                    \* still shown
       THEN UNCHANGED mark                              \* the mark stands
       ELSE mark' = IF ~MarkNamesPoll \/ mark = mPoll   \* its own, or any on main
                    THEN None                           \* comes down
                    ELSE mark                           \* another poll's stays
    /\ mpc' = "rest"                                    \* the monitor rests
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, closing, openHook,
                   beatOn, beatPoll, beatStart, beatSince, mPoll, sightPoll,
                   sightAt, badReport, badMark>>          \* nothing else moves

\* Every poll is done, the clock has stopped and the monitor rests:
\* nothing more happens.
Finished ==
    /\ done = MaxPolls /\ time = MaxTime /\ mpc = "rest" /\ ~closing \* all is over
    /\ UNCHANGED vars                                   \* and stays so

\* One step of the worker, the monitor, or the clock.
Next == \/ Tick \/ BeginTask \/ BeginHook \/ Close \/ EndTask \/ AfterTask
        \/ EndHook \/ TurnEnd \/ Sweep \/ Sight \/ Read \/ Mark
        \/ RecheckStep \/ Finished

\* Every behaviour starts at Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* The monitor only ever reports a poll that is the worker's, past the limit.
INV_ReportTrue == ~badReport

\* A task is remembered as reported only for a poll of its own past the limit.
INV_MarkTrue == ~badMark

\* At rest, the monitor leaves no mark but for a poll shown or one the worker
\* will still look at.
INV_NotLeftStuck ==
    (mpc = "rest" /\ mark # None) =>
        \/ beatOn /\ beatPoll = mark                    \* its poll is shown
        \/ wpc = "after" /\ done = mark                 \* its task poll's look is owed
        \/ openHook = mark                              \* its hook poll's look is owed

====
