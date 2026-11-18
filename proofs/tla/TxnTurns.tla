---- MODULE TxnTurns ----
\* The story, in plain words.
\*
\* A C caller works on an explicit transaction through its handle: queries,
\* document calls, and finally a commit or a discard. A call may return
\* before its work runs (the asynchronous query), so the caller's next call
\* can arrive while earlier work is still waiting. The calls must still run
\* in the order they were made, one at a time: a later query must never
\* overtake an earlier one, and a commit must never land before work the
\* caller asked for first.
\*
\* So each transaction hands out turns. A call takes the next ticket when it
\* names the transaction, in call order. Its work starts only when the
\* transaction is serving its ticket, and finishing serves the next one. A
\* call that gives up before its turn (it failed a check after taking its
\* ticket) drops its ticket: the ticket is marked cancelled, and whoever
\* reaches it skips it, so the transaction never waits for work that will
\* never come.
\*
\* Two parties can reach a cancelled ticket at once: the call finishing just
\* before it, and the call dropping it. Each looks for the mark and removes
\* it in one step; only the one that removes it serves the next ticket, so
\* the ticket is skipped exactly once. The dropping call looks again after
\* marking, because the finisher may have looked before the mark was there.
\*
\* What must never happen:
\*   1. two calls run at once - INV_OneRunning;
\*   2. a call runs while a call with an earlier ticket has yet to run or be
\*      dropped (it overtook it; a commit landed early) - INV_NoOvertake;
\*   3. the transaction stops serving with calls still waiting (a dropped
\*      ticket nobody skipped) - Progress.
\*
\* Each guard has a RED configuration that removes it:
\*   * Recheck: a dropping call looks for its turn again after marking
\*     (Red_NoRecheck);
\*   * SkipOnTurn: a dropped ticket is skipped only when its turn comes, never
\*     at once (Red_CancelSkipsNow).
\*
\* Rust: crates/ffi/src/state/turns.rs Turns::take, Ticket::wait, Ticket's
\* Drop, Turns::finish; the callers that take tickets: crates/ffi/src/target.rs
\* resolve_target, crates/ffi/src/request.rs Request::resolve,
\* crates/ffi/src/query/exec.rs ExecArgs::read and defra_query_execute_async,
\* crates/ffi/src/txn/lifecycle.rs defra_transaction_commit and
\* defra_transaction_discard.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Calls,       \* the calls the caller makes on the one transaction
  Recheck,     \* TRUE: a dropping call looks for its turn again after marking
  SkipOnTurn   \* TRUE: a dropped ticket is skipped only when its turn comes

\* How many calls there are, the most tickets ever handed out.
N == Cardinality(Calls)

\* Where a call can be:
\*   "idle"       not made yet;
\*   "waiting"    took a ticket, waits for its turn;
\*   "running"    its turn came, its work runs;
\*   "advance"    finished, serving the next ticket and skipping dropped ones;
\*   "marked"     dropped its ticket and marked it, about to look again;
\*   "done"       finished or dropped, nothing left to do.
Pcs == {"idle", "waiting", "running", "advance", "marked", "done"}

VARIABLES
  pc,         \* where each call is
  ticket,     \* each call's ticket number (0 until it takes one)
  next,       \* the next ticket number to hand out
  serving,    \* the ticket whose turn it is
  cancelled,  \* tickets dropped and not yet skipped
  ran         \* the calls whose work has run

vars == <<pc, ticket, next, serving, cancelled, ran>>

\* Every variable has the kind of value it should.
TypeOK ==
  /\ pc \in [Calls -> Pcs]
  /\ ticket \in [Calls -> 0..N]
  /\ next \in 0..N
  /\ serving \in 0..N
  /\ cancelled \subseteq 0..N
  /\ ran \subseteq Calls

\* Nothing has happened: no ticket handed out, ticket 0's turn.
Init ==
  /\ pc = [c \in Calls |-> "idle"]
  /\ ticket = [c \in Calls |-> 0]
  /\ next = 0
  /\ serving = 0
  /\ cancelled = {}
  /\ ran = {}

\* A call names the transaction and takes the next ticket (Turns::take).
Take(c) ==
  /\ pc[c] = "idle"
  /\ ticket' = [ticket EXCEPT ![c] = next]
  /\ next' = next + 1
  /\ pc' = [pc EXCEPT ![c] = "waiting"]
  /\ UNCHANGED <<serving, cancelled, ran>>

\* Its turn comes, and its work starts (Ticket::wait returns).
Start(c) ==
  /\ pc[c] = "waiting"
  /\ serving = ticket[c]
  /\ pc' = [pc EXCEPT ![c] = "running"]
  /\ ran' = ran \cup {c}
  /\ UNCHANGED <<ticket, next, serving, cancelled>>

\* Its work finishes, and the next ticket's turn comes (Turns::finish).
Finish(c) ==
  /\ pc[c] = "running"
  /\ serving' = ticket[c] + 1
  /\ pc' = [pc EXCEPT ![c] = "advance"]
  /\ UNCHANGED <<ticket, next, cancelled, ran>>

\* The finisher skips the ticket now served if it was dropped, removing the
\* mark in the same step; with none to skip, it is done.
Skip(c) ==
  /\ pc[c] = "advance"
  /\ IF serving \in cancelled
     THEN /\ cancelled' = cancelled \ {serving}
          /\ serving' = serving + 1
          /\ UNCHANGED pc
     ELSE /\ pc' = [pc EXCEPT ![c] = "done"]
          /\ UNCHANGED <<serving, cancelled>>
  /\ UNCHANGED <<ticket, next, ran>>

\* A waiting call gives up and drops its ticket (Ticket's Drop): it marks the
\* ticket cancelled, whoever's turn it is, and looks again next (LookAgain),
\* which also covers a ticket whose turn had already come. With SkipOnTurn
\* off it serves the next ticket at once instead, as if its turn had come.
Cancel(c) ==
  /\ pc[c] = "waiting"
  /\ IF SkipOnTurn
     THEN /\ cancelled' = cancelled \cup {ticket[c]}
          /\ pc' = [pc EXCEPT ![c] = "marked"]
          /\ UNCHANGED serving
     ELSE /\ serving' = serving + 1
          /\ pc' = [pc EXCEPT ![c] = "done"]
          /\ UNCHANGED cancelled
  /\ UNCHANGED <<ticket, next, ran>>

\* After marking, the dropping call looks again: if its turn came meanwhile
\* and the mark is still there, it removes the mark and serves the next
\* ticket itself. With Recheck off it never looks again.
LookAgain(c) ==
  /\ pc[c] = "marked"
  /\ IF Recheck /\ serving = ticket[c] /\ ticket[c] \in cancelled
     THEN /\ cancelled' = cancelled \ {ticket[c]}
          /\ serving' = serving + 1
          /\ pc' = [pc EXCEPT ![c] = "advance"]
     ELSE /\ pc' = [pc EXCEPT ![c] = "done"]
          /\ UNCHANGED <<serving, cancelled>>
  /\ UNCHANGED <<ticket, next, ran>>

Next ==
  \E c \in Calls :
    \/ Take(c) \/ Start(c) \/ Finish(c) \/ Skip(c) \/ Cancel(c) \/ LookAgain(c)

\* The protocol's own steps are fair: a call whose turn came starts, running
\* work finishes, and skipping and rechecking go on. Making and dropping calls
\* is the caller's choice and need never happen.
Fairness ==
  \A c \in Calls :
    /\ WF_vars(Start(c))
    /\ WF_vars(Finish(c))
    /\ WF_vars(Skip(c))
    /\ WF_vars(LookAgain(c))

Spec == Init /\ [][Next]_vars /\ Fairness

\* ---- what must hold ----

\* At most one call's work runs at a time.
INV_OneRunning ==
  Cardinality({c \in Calls : pc[c] = "running"}) <= 1

\* A running call never overtook one with an earlier ticket: no such call is
\* still waiting for its turn or still running. (It ran already, or dropped
\* its ticket.)
INV_NoOvertake ==
  \A c, d \in Calls :
    (pc[c] = "running" /\ pc[d] # "idle" /\ ticket[d] < ticket[c])
      => pc[d] \notin {"waiting", "running"}

\* Every call that waits for its turn stops waiting: it runs, or drops out.
Progress ==
  \A c \in Calls : (pc[c] = "waiting") ~> (pc[c] # "waiting")
====
