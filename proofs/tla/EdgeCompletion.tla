---- MODULE EdgeCompletion ----
\* The story, in plain words.
\*
\* A piece of network work runs as a task on the network runtime, and its
\* caller awaits its completion (crates/edge/src/task.rs Task::poll). Three
\* things can end the work:
\*   * the work finishes and hands back its value;
\*   * the caller's handle aborts it (Task::abort, Service::stop,
\*     Service::stopped, TaskSet::abort_all): the handle first records that it
\*     aborted, then asks the runtime to cancel the task;
\*   * the runtime stops (crates/edge/src/platform/native.rs, the shutdown):
\*     it first marks itself stopped (Shared::stop), then ends every task it
\*     still holds, a task never woken included (nuclei 0.5.4 and later).
\* The caller's poll sees how the task ended. A task the runtime cancelled
\* says Cancelled, whoever asked for it.
\*
\* What the caller must be told:
\*   1. work its own handle stopped ends Cancelled - INV_AbortSaysCancelled;
\*   2. work the runtime's stop cut off, never aborted by its handle, ends
\*      Closed: the runtime stopped accepting work - INV_StopSaysClosed;
\*   3. work that finished ends with its value - INV_DoneSaysValue.
\*
\* The Rust code each guard models:
\*   MapStop      Task::poll: a join that ends Cancelled becomes Closed when
\*                the runtime is stopped and the handle's `aborted` flag is
\*                unset;
\*   FlagFirst    Task::abort: `aborted` is set before `join.abort()`;
\*   StopFirst    the shutdown: Shared::stop before the runtime ends its tasks.

EXTENDS Naturals  \* nothing but plain values here

CONSTANTS
  MapStop,    \* TRUE: a cancel with the runtime stopped and no abort answers Closed
  FlagFirst,  \* TRUE: the handle records its abort before it cancels the task
  StopFirst   \* TRUE: the runtime marks itself stopped before it ends its tasks

VARIABLES
  task,       \* "running", "done" or "cancelled"
  cause,      \* who cancelled it: "none", "abort" or "stop" (a ghost: the code cannot see it)
  aborted,    \* the handle's `aborted` flag
  stopped,    \* the runtime's stopped mark (Shared::is_stopped)
  abortPc,    \* the handle's abort: "idle", "half" (one of its two steps done) or "done"
  stopPc,     \* the runtime's stop: "idle", "half" or "done"
  answer      \* what the caller's poll answered: "none", "value", "cancelled" or "closed"

vars == <<task, cause, aborted, stopped, abortPc, stopPc, answer>>  \* every variable

Init ==                     \* at the start:
  /\ task = "running"       \* the work runs
  /\ cause = "none"         \* nobody cancelled it
  /\ aborted = FALSE        \* the handle aborted nothing
  /\ stopped = FALSE        \* the runtime runs
  /\ abortPc = "idle"       \* no abort begun
  /\ stopPc = "idle"        \* no stop begun
  /\ answer = "none"        \* the caller has no answer yet

\* The runtime cancels the task if it still runs, for `who`.
Cancel(who) ==
  IF task = "running" THEN /\ task' = "cancelled" /\ cause' = who  \* cancelled now
  ELSE UNCHANGED <<task, cause>>                                    \* it ended already

\* The work finishes on its own.
Finish ==
  /\ task = "running"       \* still running
  /\ task' = "done"         \* it hands back its value
  /\ UNCHANGED <<cause, aborted, stopped, abortPc, stopPc, answer>>  \* the rest stays

\* The handle's abort, first of its two steps: the flag, or the cancel when FlagFirst is broken.
AbortFirst ==
  /\ abortPc = "idle"       \* the handle aborts now
  /\ abortPc' = "half"      \* one step done
  /\ IF FlagFirst
       THEN /\ aborted' = TRUE /\ UNCHANGED <<task, cause>>   \* the flag first
       ELSE /\ Cancel("abort") /\ UNCHANGED aborted           \* MUTATION: the cancel first
  /\ UNCHANGED <<stopped, stopPc, answer>>                     \* the rest stays

\* The handle's abort, second step: whichever of the two is left.
AbortSecond ==
  /\ abortPc = "half"       \* after the first step
  /\ abortPc' = "done"      \* the abort complete
  /\ IF FlagFirst
       THEN /\ Cancel("abort") /\ UNCHANGED aborted           \* then the cancel
       ELSE /\ aborted' = TRUE /\ UNCHANGED <<task, cause>>   \* then the flag
  /\ UNCHANGED <<stopped, stopPc, answer>>                     \* the rest stays

\* The runtime's stop, first of its two steps: the mark, or the cancel of every task when StopFirst is broken.
StopFirstStep ==
  /\ stopPc = "idle"        \* the runtime stops now
  /\ stopPc' = "half"       \* one step done
  /\ IF StopFirst
       THEN /\ stopped' = TRUE /\ UNCHANGED <<task, cause>>   \* the mark first
       ELSE /\ Cancel("stop") /\ UNCHANGED stopped            \* MUTATION: the cancel first
  /\ UNCHANGED <<aborted, abortPc, answer>>                     \* the rest stays

\* The runtime's stop, second step.
StopSecond ==
  /\ stopPc = "half"        \* after the first step
  /\ stopPc' = "done"       \* the stop complete
  /\ IF StopFirst
       THEN /\ Cancel("stop") /\ UNCHANGED stopped            \* then every task ends
       ELSE /\ stopped' = TRUE /\ UNCHANGED <<task, cause>>   \* then the mark
  /\ UNCHANGED <<aborted, abortPc, answer>>                     \* the rest stays

\* The caller's poll, once the task ended (Task::poll on a finished join).
Poll ==
  /\ answer = "none"        \* not answered yet
  /\ task # "running"       \* the join is ready
  /\ answer' = CASE task = "done" -> "value"                    \* the work's value
                 [] MapStop /\ stopped /\ ~aborted -> "closed"  \* the runtime stopped, no abort
                 [] OTHER -> "cancelled"                        \* the cancel as it came
  /\ UNCHANGED <<task, cause, aborted, stopped, abortPc, stopPc>>  \* the rest stays

Next == Finish \/ AbortFirst \/ AbortSecond \/ StopFirstStep \/ StopSecond \/ Poll  \* any, in any order
Spec == Init /\ [][Next]_vars                                                     \* every behaviour

TypeOK ==
  /\ task \in {"running", "done", "cancelled"}
  /\ cause \in {"none", "abort", "stop"}
  /\ aborted \in BOOLEAN
  /\ stopped \in BOOLEAN
  /\ abortPc \in {"idle", "half", "done"}
  /\ stopPc \in {"idle", "half", "done"}
  /\ answer \in {"none", "value", "cancelled", "closed"}

\* Work its own handle cancelled is never reported as the runtime's stop.
INV_AbortSaysCancelled == (answer # "none" /\ cause = "abort") => answer = "cancelled"
\* Work the runtime's stop cut off, its handle never aborting, is reported Closed.
INV_StopSaysClosed == (answer # "none" /\ cause = "stop" /\ abortPc = "idle") => answer = "closed"
\* Work that finished is reported with its value.
INV_DoneSaysValue == (answer # "none" /\ task = "done") => answer = "value"
====
