---- MODULE WorkerHeartbeat ----
\* The story, in plain words.
\*
\* A worker thread takes turns: it polls one of its tasks, then its I/O hook
\* (the regolith IoQueue, which reads blocks and finishes commits for this
\* thread), then a task again, and so on. Before each poll it writes a
\* heartbeat saying when the poll started and whether it polls the task or
\* the hook; when the poll returns it writes that it is between polls.
\*
\* A monitor on another thread looks at the heartbeat now and then. When a
\* poll has run past the limit it reports it, by the name the heartbeat
\* gives, and marks the worker stuck so no new work is routed to it. The
\* monitor's read, its mark, and its second look (the recheck) are three
\* separate steps: the worker can move on between any two of them. After the
\* recheck, a mark for a poll that already ended is taken back.
\*
\* When a poll returns, the worker looks for a mark for that very poll: a
\* task poll that was marked is remembered as reported, a hook poll's mark is
\* simply taken off.
\*
\* The bug that was on main: the hook wrote no heartbeat of its own, and the
\* task's heartbeat stayed up until the end of the whole turn. A slow hook
\* poll after a short task was reported under the task's name, and its mark
\* was taken by the task's next poll, which was then remembered as having
\* stalled. The mark was also a bare flag, so a mark the monitor raised just
\* after a poll ended could be taken by the next poll.
\*
\* What must never happen:
\*   1. the monitor reports a poll that is not the one running, or not past
\*      the limit - INV_ReportTrue;
\*   2. a task is remembered as reported for a poll of its own that did not
\*      run past the limit - INV_MarkTrue;
\*   3. with the monitor at rest and the worker between polls, a mark is
\*      left up, so the idle worker is routed around for good
\*      - INV_NotLeftStuck.
\*
\* The Rust code each guard models (crates/foundation/workers/src):
\*   HookBeat        the hook poll's own heartbeat, and the task poll's
\*                   heartbeat taken down as the poll returns
\*                   (worker/run.rs HookState::poll, worker/local.rs
\*                   Local::poll, worker/heartbeat.rs Heartbeat::begin_hook);
\*   MarkHoldsStart  the mark holds the start of the poll it is for, and the
\*                   worker takes only its own poll's mark
\*                   (worker/shared.rs WorkerShared::mark_stuck,
\*                   WorkerShared::unmark; worker/local.rs Local::after_poll);
\*   HookClears      a hook poll takes its own mark off as it returns
\*                   (worker/local.rs Local::after_hook);
\*   Recheck         the monitor's second look after its mark
\*                   (pool/monitor.rs scan).

EXTENDS Naturals

\* Limit:    how long a poll may run before it counts as stuck.
\* MaxTime:  the last tick of the clock in one behaviour.
\* MaxPolls: how many polls the worker makes in one behaviour; even ones
\*           are task polls, odd ones hook polls.
\* HookBeat, MarkHoldsStart, HookClears, Recheck: the guards above; a RED
\* configuration turns one or more off.
CONSTANTS Limit, MaxTime, MaxPolls, HookBeat, MarkHoldsStart, HookClears, Recheck

\* time:      the pool's clock.
\* done:      how many polls have finished.
\* wpc:       where the worker is: "between" polls, "running" a poll, or
\*            "after" it, looking for its mark.
\* runStart:  when the current poll started.
\* ranFor:    how long the current poll ran, once it returned.
\* beatOn:    the heartbeat shows a poll in progress.
\* beatStart: when the poll the heartbeat shows started.
\* beatWho:   whether the heartbeat shows the task or the hook.
\* mark:      the monitor's mark: the start of the poll it is for, or None.
\* mpc:       where the monitor is: at "rest", having "read" a long poll,
\*            or having "marked" it and not yet rechecked.
\* mStart:    the start of the poll the monitor read.
\* badReport: the monitor reported something that was not true.
\* badMark:   a short task poll was remembered as reported.
VARIABLES time, done, wpc, runStart, ranFor, beatOn, beatStart, beatWho,
          mark, mpc, mStart, badReport, badMark

vars == <<time, done, wpc, runStart, ranFor, beatOn, beatStart, beatWho,
          mark, mpc, mStart, badReport, badMark>>

\* No mark: a value no poll can start at.
None == MaxTime + 1

\* What the worker's poll number n polls: the task, then the hook, in turn.
Kind(n) == IF n % 2 = 0 THEN "task" ELSE "hook"

\* Every value each variable may take.
TypeOK ==
    /\ time \in 0..MaxTime                           \* a tick of the clock
    /\ done \in 0..MaxPolls                          \* polls finished
    /\ wpc \in {"between", "running", "after"}       \* a worker step
    /\ runStart \in 0..MaxTime                       \* a start time
    /\ ranFor \in 0..MaxTime                         \* a duration
    /\ beatOn \in BOOLEAN                            \* a poll shown or not
    /\ beatStart \in 0..MaxTime                      \* a start time
    /\ beatWho \in {"task", "hook"}                  \* who is shown
    /\ mark \in 0..None                              \* a start, or None
    /\ mpc \in {"rest", "read", "marked"}            \* a monitor step
    /\ mStart \in 0..MaxTime                         \* a start time
    /\ badReport \in BOOLEAN                         \* a false report seen
    /\ badMark \in BOOLEAN                           \* a false mark taken

\* At first nothing runs, nothing is shown, and nothing is marked.
Init ==
    /\ time = 0                 \* the clock starts
    /\ done = 0                 \* no poll yet
    /\ wpc = "between"          \* the worker is between polls
    /\ runStart = 0             \* no poll started
    /\ ranFor = 0               \* no poll ran
    /\ beatOn = FALSE           \* the heartbeat shows nothing
    /\ beatStart = 0            \* no start shown
    /\ beatWho = "task"         \* an unused default
    /\ mark = None              \* no mark
    /\ mpc = "rest"             \* the monitor waits
    /\ mStart = 0               \* nothing read
    /\ badReport = FALSE        \* nothing reported yet
    /\ badMark = FALSE          \* nothing marked yet

\* Time passes, wherever the worker and the monitor are.
Tick ==
    /\ time < MaxTime                                   \* the clock has room
    /\ time' = time + 1                                 \* one tick on
    /\ UNCHANGED <<done, wpc, runStart, ranFor, beatOn, beatStart, beatWho,
                   mark, mpc, mStart, badReport, badMark>> \* nothing else moves

\* The worker starts its next poll and writes its heartbeat. Without
\* HookBeat (main) the hook writes none, and the task's heartbeat stays up.
Begin ==
    /\ wpc = "between" /\ done < MaxPolls                \* another poll is due
    /\ wpc' = "running"                                  \* it runs
    /\ runStart' = time                                  \* starting now
    /\ IF Kind(done) = "task" \/ HookBeat                \* this poll writes a heartbeat
       THEN /\ beatOn' = TRUE                            \* a poll is shown
            /\ beatStart' = time                         \* that started now
            /\ beatWho' = Kind(done)                     \* and is this one
       ELSE UNCHANGED <<beatOn, beatStart, beatWho>>     \* main: the task's stays up
    /\ UNCHANGED <<time, done, ranFor, mark, mpc, mStart, badReport, badMark>> \* nothing else moves

\* The poll returns. With HookBeat the heartbeat comes down now; on main
\* only the hook's return, the end of the turn, takes it down.
End ==
    /\ wpc = "running"                                   \* a poll is running
    /\ wpc' = "after"                                    \* it has returned
    /\ ranFor' = time - runStart                         \* this long
    /\ IF HookBeat \/ Kind(done) = "hook"                \* the heartbeat comes down
       THEN beatOn' = FALSE                              \* nothing shown
       ELSE UNCHANGED beatOn                             \* main: the task's stays up
    /\ UNCHANGED <<time, done, runStart, beatStart, beatWho, mark, mpc,
                   mStart, badReport, badMark>>           \* nothing else moves

\* Whether the returned poll finds a mark to take: with MarkHoldsStart only
\* the mark for its own start, on main any mark at all.
Takes == IF MarkHoldsStart THEN mark = runStart ELSE mark # None

\* The worker looks for its mark. A task poll that takes one is remembered
\* as reported; it is a false memory if that poll was short. A hook poll
\* takes its mark off only with HookClears.
After ==
    /\ wpc = "after"                                     \* a poll has returned
    /\ IF Takes /\ (Kind(done) = "task" \/ HookClears)  \* a mark it takes off
       THEN mark' = None                                 \* the mark comes down
       ELSE UNCHANGED mark                               \* the mark stays as it is
    /\ badMark' = (badMark \/ (Kind(done) = "task" /\ Takes /\ ranFor <= Limit)) \* a short task blamed
    /\ done' = done + 1                                  \* one more poll done
    /\ wpc' = "between"                                  \* between polls again
    /\ UNCHANGED <<time, runStart, ranFor, beatOn, beatStart, beatWho, mpc,
                   mStart, badReport>>                    \* nothing else moves

\* The monitor reads a heartbeat past the limit and reports it under the
\* name it shows. The report is false unless that very poll is running.
Read ==
    /\ mpc = "rest"                                      \* the monitor is free
    /\ beatOn /\ time - beatStart > Limit                \* a poll shown past the limit
    /\ mpc' = "read"                                     \* it has read it
    /\ mStart' = beatStart                               \* remembering its start
    /\ badReport' = (badReport \/ ~(wpc = "running"      \* false unless a poll runs
                                    /\ Kind(done) = beatWho \* and it is the one shown
                                    /\ runStart = beatStart)) \* from the start shown
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, beatOn, beatStart,
                   beatWho, mark, badMark>>               \* nothing else moves

\* The monitor marks the worker stuck, for the poll it read; without its
\* recheck it is done.
Mark ==
    /\ mpc = "read"                                      \* it read a long poll
    /\ mark' = mStart                                    \* the mark names that poll
    /\ mpc' = IF Recheck THEN "marked" ELSE "rest"       \* recheck next, or rest
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, beatOn, beatStart,
                   beatWho, mStart, badReport, badMark>>  \* nothing else moves

\* The monitor looks again: if the poll it marked is no longer shown, it
\* takes its mark back (with MarkHoldsStart only that poll's mark).
RecheckStep ==
    /\ mpc = "marked"                                    \* it marked a poll
    /\ IF beatOn /\ beatStart = mStart                   \* that poll still runs
       THEN UNCHANGED mark                               \* the mark stands
       ELSE mark' = IF ~MarkHoldsStart \/ mark = mStart  \* its own mark, or any on main
                    THEN None                            \* comes down
                    ELSE mark                            \* another poll's stays
    /\ mpc' = "rest"                                     \* the monitor rests
    /\ UNCHANGED <<time, done, wpc, runStart, ranFor, beatOn, beatStart,
                   beatWho, mStart, badReport, badMark>>  \* nothing else moves

\* Every poll is done and the clock has stopped: nothing more happens.
Finished ==
    /\ done = MaxPolls /\ time = MaxTime /\ mpc = "rest" \* all is over
    /\ UNCHANGED vars                                    \* and stays so

\* One step of the worker, the monitor, or the clock.
Next == Tick \/ Begin \/ End \/ After \/ Read \/ Mark \/ RecheckStep \/ Finished

\* Every behaviour starts at Init and takes Next steps.
Spec == Init /\ [][Next]_vars

\* The monitor only ever reports the poll that runs, past the limit.
INV_ReportTrue == ~badReport

\* A task is remembered as reported only for a poll of its own past the limit.
INV_MarkTrue == ~badMark

\* An idle worker with the monitor at rest carries no mark.
INV_NotLeftStuck == (mpc = "rest" /\ wpc = "between") => mark = None

====
