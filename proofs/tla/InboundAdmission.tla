---- MODULE InboundAdmission ----
\* The story: peers open streams to our node and each stream carries one
\* message. Before we read a message's body into memory we ask for room in a
\* byte budget. The body stays "ours" (it holds budget) until the code that
\* handles it is done. Handlers run from queues, one queue per kind of work.
\*
\* Who acts: four peers. Three are hostile: their bulk messages land in
\* handlers that never finish, and one of them also starts control messages and
\* then stops sending halfway through the body. One peer is healthy and sends a
\* single control message (an ack, say).
\*
\* What must never happen: the bytes we hold in memory for a class go over that
\* class's budget. What must always happen: the healthy control message is
\* handled, and no stream waits for budget forever.
\*
\* Code it models (crates/p2p/src):
\*   iroh/endpoint_streams.rs  handle_connection_streams  (tag, length, admit, read)
\*   iroh/inbound_budget.rs    InboundBudget::admit       (class budget + connection share)
\*   sync/dispatch_queue.rs    DispatchQueue              (one queue per dispatch class)
\*   sync/event_dispatcher.rs  run_event_dispatcher       (holds the lease until handled)
\*
\* Every byte size is 1 here: what matters is how many bodies are held, not how
\* big each one is. The Lean proof ByteBudget covers the arithmetic on sizes.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Streams,       \* every stream in the run
  PeerOf,        \* [Streams -> peer] the peer that opened the stream
  ClassOf,       \* [Streams -> class] the class its tag maps to
  Classes,       \* the priority classes
  Healthy,       \* the stream we promise to handle
  Stalls,        \* streams whose sender stops halfway through the body
  StuckClasses,  \* classes whose handlers never finish
  Budget,        \* [Classes -> Nat] bodies a class may hold at once
  Share,         \* bodies one peer may hold in one class at once
  Slots,         \* [Classes -> Nat] handlers that may run at once per class
  BudgetMode,    \* "PerClass" | "Shared"
  ShareMode,     \* "PerPeer" | "None"
  DrainMode,     \* "PerClass" | "Shared"
  LeaseMode,     \* "UntilHandled" | "UntilQueued"
  ReadMode,      \* "AdmitFirst" | "ReadFirst"
  DeadlineMode   \* "Deadline" | "None"

\* The healthy stream is a real stream, in a class whose handlers finish.
ASSUME Healthy \in Streams /\ ClassOf[Healthy] \notin StuckClasses
\* A share smaller than a budget is what leaves room for someone else.
ASSUME \A c \in Classes : Share < Budget[c]
ASSUME BudgetMode \in {"PerClass", "Shared"}
ASSUME ShareMode \in {"PerPeer", "None"}
ASSUME DrainMode \in {"PerClass", "Shared"}
ASSUME LeaseMode \in {"UntilHandled", "UntilQueued"}
ASSUME ReadMode \in {"AdmitFirst", "ReadFirst"}
ASSUME DeadlineMode \in {"Deadline", "None"}

VARIABLES
  state,  \* [Streams -> stage] where each stream is in its life
  queue   \* [queue key -> Seq(Streams)] bodies read and waiting for a handler

vars == <<state, queue>>

\* Stages, in order: "new" (tag not read), "waiting" (length read, asking for
\* budget), "reading" (budget held, body arriving), "queued", "handling",
\* "done" (handler returned), "reset" (we gave up and reset the stream).
Stages == {"new", "waiting", "reading", "queued", "handling", "done", "reset"}

\* Per-class queues use the class as the key; a shared drain uses one key.
QueueKeys == IF DrainMode = "PerClass" THEN Classes ELSE {"all"}
KeyOf(s) == IF DrainMode = "PerClass" THEN ClassOf[s] ELSE "all"

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
ByPeerInClass(p, c) == {s \in InClass(c) : PeerOf[s] = p}

TotalBudget == LET RECURSIVE Sum(_)
                   Sum(S) == IF S = {} THEN 0
                             ELSE LET c == CHOOSE x \in S : TRUE
                                  IN Budget[c] + Sum(S \ {c})
               IN Sum(Classes)

\* Room for one more body of stream s: its class budget (or the shared pool)
\* has a free slot, and its peer has not used up its share of that class.
CanAdmit(s) ==
  LET c == ClassOf[s] IN
  /\ IF BudgetMode = "PerClass"
     THEN HeldIn(InClass(c)) < Budget[c]
     ELSE HeldIn(Streams) < TotalBudget
  /\ ShareMode = "PerPeer" => HeldIn(ByPeerInClass(PeerOf[s], c)) < Share

Handling(c) == Cardinality({s \in InClass(c) : state[s] = "handling"})

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

\* The deadline fires while the stream still has no room: reset it. The
\* sender sees a stream error and retries later.
Timeout(s) ==
  /\ DeadlineMode = "Deadline"
  /\ state[s] = "waiting"
  /\ ~CanAdmit(s)
  /\ state' = [state EXCEPT ![s] = "reset"]
  /\ UNCHANGED queue

\* The whole body arrived; hand it to its queue. A stalling sender never
\* gets here.
BodyDone(s) ==
  /\ state[s] = "reading"
  /\ s \notin Stalls
  /\ state' = [state EXCEPT ![s] = "queued"]
  /\ queue' = [queue EXCEPT ![KeyOf(s)] = Append(@, s)]

\* The dispatcher takes the head of a queue when that head's class has a
\* free handler. With one shared queue, a head that cannot start blocks
\* everything behind it.
Dequeue(k) ==
  /\ queue[k] # <<>>
  /\ LET s == Head(queue[k]) IN
     /\ Handling(ClassOf[s]) < Slots[ClassOf[s]]
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
  \/ \E s \in Streams : ReadHeader(s) \/ Admit(s) \/ Timeout(s) \/ BodyDone(s) \/ Finish(s)
  \/ \E k \in QueueKeys : Dequeue(k)

\* The scheduler is fair: a step that stays possible is eventually taken.
Fairness ==
  /\ \A s \in Streams :
       /\ WF_vars(ReadHeader(s))
       /\ WF_vars(Admit(s))
       /\ WF_vars(Timeout(s))
       /\ WF_vars(BodyDone(s))
       /\ WF_vars(Finish(s))
  /\ \A k \in QueueKeys : WF_vars(Dequeue(k))

Spec == Init /\ [][Next]_vars /\ Fairness

INV_TypeOK == TypeOK

\* The bodies a class keeps in memory never exceed that class's budget.
INV_ResidentWithinBudget ==
  \A c \in Classes : ResidentIn(InClass(c)) <= Budget[c]

\* The healthy control message is eventually handled, whatever the hostile
\* peers do.
LIVE_HealthyHandled == <>(state[Healthy] = "done")

\* No stream waits for budget forever: it is admitted or reset.
LIVE_WaitEnds == \A s \in Streams : (state[s] = "waiting") ~> (state[s] # "waiting")
====
