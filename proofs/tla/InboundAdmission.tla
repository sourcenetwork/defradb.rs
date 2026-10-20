---- MODULE InboundAdmission ----
\* The story: peers open streams to our node over their connections, and each
\* stream carries one message. Before we read a message's body into memory we
\* ask for room in a byte budget. The body stays "ours" (it holds budget) until
\* the code that handles it is done. Handlers run from queues, one queue per
\* dispatch lane, and each lane has a fixed number of handler slots.
\*
\* Who acts: hostile connections send bulk messages whose handlers never
\* finish, and one also starts a control message whose body never completes:
\* either the sender stops, or it trickles one byte at a time, often enough
\* that the idle timer never fires. One healthy connection sends a control
\* message, and sends it again whenever we reset it.
\*
\* What must never happen: the bytes a class keeps in memory go over that
\* class's budget, or one connection holds more than its share of a class.
\* What must always happen: the healthy message is handled, and no stream
\* waits for budget forever.
\*
\* Code it models (crates/p2p/src):
\*   iroh/endpoint_streams.rs  admit_stream               (tag, length, admit, body read until idle and a total deadline)
\*   iroh/inbound_budget.rs    body_deadline              (idle deadline plus length over the minimum rate)
\*   iroh/inbound_budget.rs    InboundBudget::admit       (class budget + connection share)
\*   sync/dispatch_queue.rs    dispatch_queue             (one queue per dispatch lane)
\*   sync/event_dispatcher.rs  run_event_dispatcher       (holds the lease until handled)
\*
\* Every byte size is 1 here: what matters is how many bodies are held, not how
\* big each one is. The Lean proof ByteBudget covers the arithmetic on sizes.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Streams,          \* every stream in the run
  ConnOf,           \* [Streams -> connection] the connection that opened the stream
  ClassOf,          \* [Streams -> class] the priority class its tag maps to
  LaneOf,           \* [Streams -> lane] the dispatch lane its event goes to
  Classes,          \* the priority classes
  Lanes,            \* the dispatch lanes
  Healthy,          \* the stream we promise to handle
  Stalls,           \* streams whose sender stops halfway through the body
  Trickles,         \* streams whose sender sends a byte now and then, below the minimum rate
  StuckClasses,     \* classes whose handlers never finish
  Budget,           \* [Classes -> Nat] bodies a class may hold at once
  Share,            \* bodies one connection may hold in one class at once
  Slots,            \* [Lanes -> Nat] handlers that may run at once per lane
  BudgetMode,       \* "PerClass" | "Shared"
  ShareMode,        \* "PerConnection" | "None"
  DrainMode,        \* "PerLane" | "Shared"
  LeaseMode,        \* "UntilHandled" | "UntilQueued"
  ReadMode,         \* "AdmitFirst" | "ReadFirst"
  DeadlineMode,     \* "Deadline" | "None"   (waiting for budget)
  ReadDeadlineMode  \* "Deadline" | "IdleOnly" | "None"   (reading the body)

\* The healthy stream is a real stream, in a class whose handlers finish, and
\* its sender does not stall.
ASSUME Healthy \in Streams /\ ClassOf[Healthy] \notin StuckClasses /\ Healthy \notin Stalls /\ Healthy \notin Trickles
\* A connection may hold at least one body, and never more than the class.
ASSUME \A c \in Classes : 1 <= Share /\ Share <= Budget[c]
ASSUME BudgetMode \in {"PerClass", "Shared"}
ASSUME ShareMode \in {"PerConnection", "None"}
ASSUME DrainMode \in {"PerLane", "Shared"}
ASSUME LeaseMode \in {"UntilHandled", "UntilQueued"}
ASSUME ReadMode \in {"AdmitFirst", "ReadFirst"}
ASSUME DeadlineMode \in {"Deadline", "None"}
ASSUME ReadDeadlineMode \in {"Deadline", "IdleOnly", "None"}

VARIABLES
  state,  \* [Streams -> stage] where each stream is in its life
  queue   \* [queue key -> Seq(Streams)] bodies read and waiting for a handler

vars == <<state, queue>>

\* Stages, in order: "new" (tag not read), "waiting" (length read, asking for
\* budget), "reading" (budget held, body arriving), "queued", "handling",
\* "done" (handler returned), "reset" (we gave up and reset the stream).
Stages == {"new", "waiting", "reading", "queued", "handling", "done", "reset"}

\* Per-lane queues use the lane as the key; a shared drain uses one key.
QueueKeys == IF DrainMode = "PerLane" THEN Lanes ELSE {"all"}
KeyOf(s) == IF DrainMode = "PerLane" THEN LaneOf[s] ELSE "all"

\* A stream holds budget while its body is ours. With UntilQueued the lease is
\* dropped as soon as the body is queued, which is the bug that RED plants.
Holds(s) ==
  IF LeaseMode = "UntilHandled"
  THEN state[s] \in {"reading", "queued", "handling"}
  ELSE state[s] = "reading"

\* A body is in memory from the moment it is read until its handler returns.
\* With ReadFirst the body is read before asking for budget, so a waiting
\* stream already has its body in memory.
Resident(s) ==
  \/ state[s] \in {"reading", "queued", "handling"}
  \/ (ReadMode = "ReadFirst" /\ state[s] = "waiting")

\* How many bodies a set of streams holds budget for, or has in memory.
HeldIn(S) == Cardinality({s \in S : Holds(s)})
ResidentIn(S) == Cardinality({s \in S : Resident(s)})

InClass(c) == {s \in Streams : ClassOf[s] = c}
ByConnInClass(k, c) == {s \in InClass(c) : ConnOf[s] = k}

\* Every connection that opens a stream in this run.
Conns == {ConnOf[s] : s \in Streams}

\* The sum of every class budget: the pool when classes share one.
TotalBudget == LET RECURSIVE Sum(_)
                   Sum(S) == IF S = {} THEN 0
                             ELSE LET c == CHOOSE x \in S : TRUE
                                  IN Budget[c] + Sum(S \ {c})
               IN Sum(Classes)

\* Room for one more body of stream s: its class budget (or the shared pool)
\* has a free slot, and its connection has not used up its share of that class.
CanAdmit(s) ==
  LET c == ClassOf[s] IN
  /\ IF BudgetMode = "PerClass"
     THEN HeldIn(InClass(c)) < Budget[c]
     ELSE HeldIn(Streams) < TotalBudget
  /\ ShareMode = "PerConnection" => HeldIn(ByConnInClass(ConnOf[s], c)) < Share

\* How many handlers of lane l are running now.
Handling(l) == Cardinality({s \in Streams : LaneOf[s] = l /\ state[s] = "handling"})

TypeOK ==
  /\ state \in [Streams -> Stages]
  /\ queue \in [QueueKeys -> Seq(Streams)]

Init ==
  /\ state = [s \in Streams |-> "new"]
  /\ queue = [k \in QueueKeys |-> <<>>]

\* The tag and the four-byte length arrive. Nothing big is allocated yet,
\* unless ReadMode is ReadFirst, in which case Resident already counts it.
ReadHeader(s) ==
  /\ state[s] = "new"
  /\ state' = [state EXCEPT ![s] = "waiting"]
  /\ UNCHANGED queue

\* Budget is free: take it and start reading the body.
Admit(s) ==
  /\ state[s] = "waiting"
  /\ CanAdmit(s)
  /\ state' = [state EXCEPT ![s] = "reading"]
  /\ UNCHANGED queue

\* The admit deadline fires while the stream still has no room: reset it.
Timeout(s) ==
  /\ DeadlineMode = "Deadline"
  /\ state[s] = "waiting"
  /\ ~CanAdmit(s)
  /\ state' = [state EXCEPT ![s] = "reset"]
  /\ UNCHANGED queue

\* The whole body arrived; hand it to its lane's queue. A stalling or
\* trickling sender never gets here.
BodyDone(s) ==
  /\ state[s] = "reading"
  /\ s \notin Stalls
  /\ s \notin Trickles
  /\ state' = [state EXCEPT ![s] = "queued"]
  /\ queue' = [queue EXCEPT ![KeyOf(s)] = Append(@, s)]

\* A read deadline fires and resets the stream, which gives its budget back.
\* The idle deadline fires on a body that stopped arriving (no byte for a
\* while). The total deadline, derived from the declared length, also fires on
\* a body that trickles in below the minimum rate. A sender that keeps above
\* the minimum rate finishes in time, so only these two kinds miss it.
ReadTimeout(s) ==
  /\ ReadDeadlineMode # "None"
  /\ state[s] = "reading"
  /\ \/ s \in Stalls
     \/ (ReadDeadlineMode = "Deadline" /\ s \in Trickles)
  /\ state' = [state EXCEPT ![s] = "reset"]
  /\ UNCHANGED queue

\* The healthy sender sends its message again after a reset.
Retry(s) ==
  /\ s = Healthy
  /\ state[s] = "reset"
  /\ state' = [state EXCEPT ![s] = "new"]
  /\ UNCHANGED queue

\* The dispatcher takes the head of a queue when that head's lane has a free
\* handler. With one shared queue, a head that cannot start blocks everything
\* behind it.
Dequeue(k) ==
  /\ queue[k] # <<>>
  /\ LET s == Head(queue[k]) IN
     /\ Handling(LaneOf[s]) < Slots[LaneOf[s]]
     /\ state' = [state EXCEPT ![s] = "handling"]
     /\ queue' = [queue EXCEPT ![k] = Tail(@)]

\* A handler returns and its lease is dropped. Handlers of stuck classes
\* never return.
Finish(s) ==
  /\ state[s] = "handling"
  /\ ClassOf[s] \notin StuckClasses
  /\ state' = [state EXCEPT ![s] = "done"]
  /\ UNCHANGED queue

Next ==
  \/ \E s \in Streams :
       \/ ReadHeader(s) \/ Admit(s) \/ Timeout(s) \/ BodyDone(s)
       \/ ReadTimeout(s) \/ Retry(s) \/ Finish(s)
  \/ \E k \in QueueKeys : Dequeue(k)

\* The scheduler is fair: a step that stays possible is eventually taken.
Fairness ==
  /\ \A s \in Streams :
       /\ WF_vars(ReadHeader(s))
       /\ WF_vars(Admit(s))
       /\ WF_vars(Timeout(s))
       /\ WF_vars(BodyDone(s))
       /\ WF_vars(ReadTimeout(s))
       /\ WF_vars(Retry(s))
       /\ WF_vars(Finish(s))
  /\ \A k \in QueueKeys : WF_vars(Dequeue(k))

Spec == Init /\ [][Next]_vars /\ Fairness

INV_TypeOK == TypeOK

\* The bodies a class keeps in memory never exceed that class's budget.
INV_ResidentWithinBudget ==
  \A c \in Classes : ResidentIn(InClass(c)) <= Budget[c]

\* One connection never holds more than its share of a class.
INV_ShareBound ==
  \A k \in Conns, c \in Classes : HeldIn(ByConnInClass(k, c)) <= Share

\* The healthy message is eventually handled, whatever the hostile
\* connections do.
LIVE_HealthyHandled == <>(state[Healthy] = "done")

\* No stream waits for budget forever: it is admitted or reset.
LIVE_WaitEnds == \A s \in Streams : (state[s] = "waiting") ~> (state[s] # "waiting")
====
