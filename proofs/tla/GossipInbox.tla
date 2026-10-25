---- MODULE GossipInbox ----
\* The story: one task, the gossip driver, runs the gossip protocol. Two
\* kinds of input reach it. Control input comes from our own endpoint loop:
\* subscribe, unsubscribe, publish, and what happens to connections. Received
\* input comes from peers, as fast as they care to send it. The driver hands
\* what it hears to the event consumer through a bounded event lane, and waits
\* when that lane is full. The consumer, in turn, may ask the endpoint loop for
\* something (a publish) and wait for the reply before it reads more events.
\*
\* Who acts: peers sending us gossip, the consumer asking the endpoint, the
\* endpoint loop handing that request to the driver, the driver, and the
\* consumer reading events.
\*
\* What must never happen: the endpoint loop waits for the driver while the
\* driver waits for the consumer while the consumer waits for the endpoint
\* loop. That cycle stops the node: no command, no accept, no gossip. What
\* must always happen: every request the consumer makes is answered.
\*
\* Code it models (crates/p2p/src/iroh):
\*   gossip/inbox.rs          Inbox::push, Inbox::offer, Inbox::next  (two lanes)
\*   gossip/driver.rs         read_stream    (offers what a peer sends; drops it when full)
\*   gossip/driver.rs         Driver::emit   (waits for room on the event lane)
\*   gossip/mod.rs            GossipHandle::publish  (never waits)
\*   endpoint_commands.rs     handle_command (Publish answered inline)
\* The records task takes its inputs the same way, so this model checks it too:
\*   records/task.rs          RecordsHandle::push_change, push, next  (two lanes)
EXTENDS Naturals

CONSTANTS
  Cap,        \* room for received messages (Shared: room for every input)
  EventCap,   \* room on the event lane
  MaxRecv,    \* how many messages peers send in the run
  MaxCmds,    \* how many requests the consumer makes in the run
  InboxMode   \* "Lanes" (control never waits; received dropped when full)
              \* | "Shared" (one bounded queue; everyone waits for room)
              \* | "LossyShared" (one bounded queue; received dropped when full, control waits)

\* Every bound is a natural number and there is some room.
ASSUME Cap \in Nat /\ Cap > 0
ASSUME EventCap \in Nat /\ EventCap > 0
ASSUME MaxRecv \in Nat /\ MaxCmds \in Nat
ASSUME InboxMode \in {"Lanes", "Shared", "LossyShared"}

VARIABLES
  ctl,        \* control inputs waiting for the driver
  recv,       \* received messages waiting for the driver
  events,     \* events waiting for the consumer
  holding,    \* the driver holds an event it could not hand on yet
  consumer,   \* "reading" events, or "waiting" for the endpoint's reply
  endpoint,   \* "idle", or "handing" a request to the driver
  sent,       \* messages peers have sent so far
  cmds        \* requests the consumer has made so far

vars == <<ctl, recv, events, holding, consumer, endpoint, sent, cmds>>

\* One bounded queue for both kinds of input.
SharedQueue == InboxMode \in {"Shared", "LossyShared"}

\* There is room for one more input in the shared queue.
SharedRoom == ctl + recv < Cap

TypeOK ==
  /\ ctl \in Nat
  /\ recv \in 0..Cap
  /\ events \in 0..EventCap
  /\ holding \in BOOLEAN
  /\ consumer \in {"reading", "waiting"}
  /\ endpoint \in {"idle", "handing"}
  /\ sent \in 0..MaxRecv
  /\ cmds \in 0..MaxCmds

\* Nothing queued, nothing held, nobody waiting.
Init ==
  /\ ctl = 0
  /\ recv = 0
  /\ events = 0
  /\ holding = FALSE
  /\ consumer = "reading"
  /\ endpoint = "idle"
  /\ sent = 0
  /\ cmds = 0

\* A peer sends a message. With Shared the reader waits for room (the step is
\* not taken until there is some). Otherwise a full lane drops the message.
Receive ==
  /\ sent < MaxRecv
  /\ sent' = sent + 1
  /\ IF InboxMode = "Shared"
     THEN /\ SharedRoom
          /\ recv' = recv + 1
     ELSE IF InboxMode = "LossyShared"
          \* Dropped when the shared queue is full.
          THEN recv' = IF SharedRoom THEN recv + 1 ELSE recv
          \* Dropped when its own lane is full.
          ELSE recv' = IF recv < Cap THEN recv + 1 ELSE recv
  /\ UNCHANGED <<ctl, events, holding, consumer, endpoint, cmds>>

\* The consumer asks the endpoint for something and waits for the reply.
Ask ==
  /\ consumer = "reading" /\ endpoint = "idle"
  /\ cmds < MaxCmds
  /\ cmds' = cmds + 1
  /\ consumer' = "waiting"
  /\ endpoint' = "handing"
  /\ UNCHANGED <<ctl, recv, events, holding, sent>>

\* The endpoint loop hands the request to the driver, then replies. With a
\* shared queue it must wait for room first; with lanes it never waits.
Hand ==
  /\ endpoint = "handing"
  \* Only a shared queue can make the endpoint wait.
  /\ SharedQueue => SharedRoom
  /\ ctl' = ctl + 1
  /\ endpoint' = "idle"
  /\ consumer' = "reading"
  /\ UNCHANGED <<recv, events, holding, sent, cmds>>

\* The driver takes control input first, and only when it holds no event.
TakeControl ==
  /\ ~holding /\ ctl > 0
  /\ ctl' = ctl - 1
  /\ UNCHANGED <<recv, events, holding, consumer, endpoint, sent, cmds>>

\* The driver takes a received message; it turns into an event to hand on.
TakeReceived ==
  /\ ~holding /\ ctl = 0 /\ recv > 0
  /\ recv' = recv - 1
  /\ holding' = TRUE
  /\ UNCHANGED <<ctl, events, consumer, endpoint, sent, cmds>>

\* The driver hands its event on once the event lane has room.
Emit ==
  /\ holding /\ events < EventCap
  /\ events' = events + 1
  /\ holding' = FALSE
  /\ UNCHANGED <<ctl, recv, consumer, endpoint, sent, cmds>>

\* The consumer reads an event, unless it is waiting for a reply.
Read ==
  /\ consumer = "reading" /\ events > 0
  /\ events' = events - 1
  /\ UNCHANGED <<ctl, recv, holding, consumer, endpoint, sent, cmds>>

Next == Receive \/ Ask \/ Hand \/ TakeControl \/ TakeReceived \/ Emit \/ Read

\* Every task keeps going while it can; peers and the consumer need not.
Fairness ==
  /\ WF_vars(Hand)
  /\ WF_vars(TakeControl)
  /\ WF_vars(TakeReceived)
  /\ WF_vars(Emit)
  /\ WF_vars(Read)

Spec == Init /\ [][Next]_vars /\ Fairness

INV_TypeOK == TypeOK

\* Every request the consumer makes is answered: the endpoint loop never ends
\* up waiting on the driver for good.
LIVE_RequestAnswered == (consumer = "waiting") ~> (consumer = "reading")
====
