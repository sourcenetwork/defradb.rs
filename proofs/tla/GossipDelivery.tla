---- MODULE GossipDelivery ----
\* The story: a node publishes messages on a gossip topic. Two peers
\* subscribe to it, and the node is connected to both, but gossip keeps only
\* one of them as its neighbour: "near". The other, "far", hears gossip
\* relayed through near, and takes a relayed message because it is connected
\* to the publisher. Gossip (iroh-gossip's plumtree) only carries a broadcast
\* up to a budget, and drops a larger one with nothing but a log line. What
\* it does carry, our writer puts on one stream, one frame per message, and
\* our frames are larger than gossip's own by the topic they carry. The
\* writer refuses a frame over its limit. A stream can also fail under the
\* writer at any time.
\*
\* Who acts: the publisher, the gossip protocol queueing frames, the writer
\* sending them, the direct PushLog pushes for what gossip cannot carry, and
\* the network failing the stream.
\*
\* What must never happen: a message is lost for a subscriber and nobody
\* hears of it. For every subscriber, every message is delivered, or still
\* on its way, or reported: to its publisher as an error, or in the log as a
\* dropped message. And every broadcast gossip queues fits a frame.
\*
\* Code it models (crates/p2p/src/iroh):
\*   gossip/frame.rs          MAX_BROADCAST, FRAME_LIMIT, encode_gossip_frame
\*   gossip/mod.rs            GossipHandle::publish   (refuses over the budget)
\*   gossip/driver.rs         Driver::new (config, FRAME_LIMIT), write_stream, write_queue
\*   endpoint_commands.rs     handle_command (Publish falls back, PublishRaw errors)
\*   direct_push.rs           peers (every connected peer), push, push_one
EXTENDS Naturals, Sequences

CONSTANTS
  FrameLimit,     \* the largest frame any release writes or reads
  TopicLen,       \* bytes our frame adds: the topic
  Overhead,       \* bytes a broadcast adds to its content in gossip's check
  Size,           \* each message's content size
  Control,        \* messages the protocol emits itself, never size-checked
  LimitMatched,   \* guard: gossip's size leaves room for the topic
  Fallback,       \* guard: a publish over the budget goes by direct push
  PushConnected,  \* guard: the direct push goes to every connected peer
  SkipOversize,   \* guard: the writer skips a frame it cannot send
  ReportLoss      \* guard: a failed stream reports what it took with it

\* The messages: the domain of Size.
Msgs == DOMAIN Size

\* The subscribers: the gossip neighbour, and a connected peer gossip
\* reaches only through it.
Subscribers == {"near", "far"}

\* Every size is a number, and the guards are switches.
ASSUME FrameLimit \in Nat /\ TopicLen \in Nat /\ Overhead \in Nat
ASSUME \A m \in Msgs : Size[m] \in Nat
ASSUME Control \subseteq Msgs
ASSUME {LimitMatched, Fallback, PushConnected, SkipOversize, ReportLoss} \subseteq BOOLEAN

\* iroh-gossip's max_message_size. With the guard it is the frame less the
\* topic; without it, the frame limit itself, which forgets the topic our
\* frame carries.
MaxMessage == IF LimitMatched THEN FrameLimit - TopicLen ELSE FrameLimit

\* The largest content gossip broadcasts: its budget is max_message_size - 2.
Budget == MaxMessage - 2 - Overhead

\* A message's frame: the topic, the message tag, the overhead, the content.
FrameSize(m) == TopicLen + 1 + Overhead + Size[m]

\* The largest frame the writer sends: the one every release reads.
Limit == FrameLimit

\* Who a direct push goes to. With the guard, every connected peer; without
\* it, only gossip's neighbour, which leaves far out.
Pushed == IF PushConnected THEN Subscribers ELSE {"near"}

VARIABLES
  published,  \* messages handed out so far
  queue,      \* frames waiting on the writer to near, in order
  direct,     \* <<message, peer>> pushes on their way
  delivered,  \* <<message, peer>>: the peer has the message
  reported,   \* messages whose loss someone was told about
  up,         \* the writer's stream is open
  failures    \* how many times the stream has failed

vars == <<published, queue, direct, delivered, reported, up, failures>>

\* The messages in the writer's queue.
Queued == {queue[i] : i \in 1..Len(queue)}

TypeOK ==
  /\ published \subseteq Msgs
  /\ queue \in Seq(Msgs)
  /\ direct \subseteq Msgs \X Subscribers
  /\ delivered \subseteq Msgs \X Subscribers
  /\ reported \subseteq Msgs
  /\ up \in BOOLEAN
  /\ failures \in 0..1

\* Nothing published, an open stream, nothing failed.
Init ==
  /\ published = {}
  /\ queue = <<>>
  /\ direct = {}
  /\ delivered = {}
  /\ reported = {}
  /\ up = TRUE
  /\ failures = 0

\* A caller publishes `m`. One gossip fits is queued for the writer. One over
\* the budget is refused before it is queued; with the guard it goes by
\* direct push, without it gossip drops it silently.
Publish(m) ==
  /\ m \in Msgs \ (published \cup Control)
  /\ published' = published \cup {m}
  \* Fits the budget: gossip queues its frame.
  /\ IF Size[m] <= Budget
     THEN /\ queue' = Append(queue, m)
          /\ UNCHANGED direct
     \* Over the budget, with the fallback: a push to each peer it goes to.
     ELSE IF Fallback
          THEN /\ direct' = direct \cup {<<m, p>> : p \in Pushed}
               /\ UNCHANGED queue
          \* Over the budget, no fallback: gossip drops it and tells nobody.
          ELSE UNCHANGED <<queue, direct>>
  /\ UNCHANGED <<delivered, reported, up, failures>>

\* The protocol sends a control message of its own (a shuffle, an IHave):
\* gossip does not size these against the broadcast budget.
Emit(m) ==
  /\ m \in Control \ published
  /\ published' = published \cup {m}
  /\ queue' = Append(queue, m)
  /\ UNCHANGED <<direct, delivered, reported, up, failures>>

\* A direct push lands: that peer has the message.
DirectDone(m, p) ==
  /\ <<m, p>> \in direct
  /\ direct' = direct \ {<<m, p>>}
  /\ delivered' = delivered \cup {<<m, p>>}
  /\ UNCHANGED <<published, queue, reported, up, failures>>

\* A direct push is refused or fails: the publisher gets the error.
DirectFail(m, p) ==
  /\ <<m, p>> \in direct
  /\ direct' = direct \ {<<m, p>>}
  /\ reported' = reported \cup {m}
  /\ UNCHANGED <<published, queue, delivered, up, failures>>

\* The writer takes the next frame. One within the limit reaches near, which
\* relays it to far. One over it is skipped and logged with the guard;
\* without it the stream ends, and every frame behind it goes too,
\* unreported.
Write ==
  /\ up /\ queue /= <<>>
  /\ LET m == Head(queue) IN
     \* Within the limit: sent, and relayed on.
     IF FrameSize(m) <= Limit
     THEN /\ delivered' = delivered \cup {<<m, p>> : p \in Subscribers}
          /\ queue' = Tail(queue)
          /\ UNCHANGED <<reported, up>>
     \* Over it, with the guard: skipped, with a warning; the rest carry on.
     ELSE IF SkipOversize
          THEN /\ reported' = reported \cup {m}
               /\ queue' = Tail(queue)
               /\ UNCHANGED <<delivered, up>>
          \* Over it, no guard: the stream ends and the queue with it.
          ELSE /\ up' = FALSE
               /\ queue' = <<>>
               /\ UNCHANGED <<delivered, reported>>
  /\ UNCHANGED <<published, direct, failures>>

\* The network fails the stream once while frames wait. With the guard the
\* writer reports what it drops; without it they are gone without a word.
Fail ==
  /\ up /\ queue /= <<>> /\ failures = 0
  /\ failures' = 1
  /\ up' = FALSE
  /\ queue' = <<>>
  \* What was queued is reported only with the guard.
  /\ reported' = IF ReportLoss THEN reported \cup Queued ELSE reported
  /\ UNCHANGED <<published, direct, delivered>>

\* The next send opens a new stream.
Reopen ==
  /\ ~up
  /\ up' = TRUE
  /\ UNCHANGED <<published, queue, direct, delivered, reported, failures>>

Next ==
  \/ \E m \in Msgs : Publish(m) \/ Emit(m)
  \/ \E m \in Msgs, p \in Subscribers : DirectDone(m, p) \/ DirectFail(m, p)
  \/ Write \/ Fail \/ Reopen

Spec == Init /\ [][Next]_vars

INV_TypeOK == TypeOK

\* Promise: no message is lost for any subscriber without someone hearing of
\* it. For each one, each message is delivered, still queued or pushed to
\* it, or reported.
INV_NoSilentLoss ==
  \A m \in published, p \in Subscribers :
    \/ <<m, p>> \in delivered \cup direct
    \/ m \in reported \cup Queued

\* Promise: every broadcast gossip queues fits one of our frames, so a
\* message gossip accepts can never end the stream.
INV_BroadcastFits ==
  \A m \in Queued \ Control : FrameSize(m) <= Limit
====
