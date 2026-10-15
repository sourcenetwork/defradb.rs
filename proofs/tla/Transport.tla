---- MODULE Transport ----
\* The link under the PushLog head hint, made explicit.
\*
\* Every other model in this corpus abstracts delivery: `Connected(m, n)` is a
\* predicate and a block moves in one atomic action.  Here a request is an
\* object on a wire, its reply is a second object on a second wire, and the two
\* fail independently -- which is what Go's two-stream protocol actually does
\* (`crates/p2p/src/two_stream/mod.rs`: the sender closes the request stream,
\* the receiver opens a NEW stream back).
\*
\* Policy knobs isolate the counterexamples:
\*   LinkMode     = "Ordered"  | "Reorder"      -- can a later head land first
\*   DupMode      = "Dedup"    | "Duplicate"    -- multipath delivers two copies
\*   ReplyMode    = "NewStream"| "SameStream"   -- Go two-stream vs bidirectional
\*   RouteMode    = "Routable" | "RelayOnly"    -- can the receiver dial back
\*   TimeoutMode  = "Ambiguous"| "Definite"     -- is a timeout "maybe delivered"
\*   RegisterMode = "HeadCurrent" | "LastArrived" -- monotone vs arrival order
\*
\* The green configuration is the hostile link with the shipped design: a link
\* that reorders, duplicates, needs a reverse route for every reply, and times
\* out ambiguously, against a monotone receiver and a rederiving sender.
EXTENDS Naturals, FiniteSets

CONSTANTS
  MaxHead,
  MaxLoss,
  LinkMode,
  DupMode,
  ReplyMode,
  RouteMode,
  TimeoutMode,
  RegisterMode

ASSUME MaxHead \in Nat /\ MaxHead >= 2
ASSUME MaxLoss \in Nat
ASSUME LinkMode \in {"Ordered", "Reorder"}
ASSUME DupMode \in {"Dedup", "Duplicate"}
ASSUME ReplyMode \in {"NewStream", "SameStream"}
ASSUME RouteMode \in {"Routable", "RelayOnly"}
ASSUME TimeoutMode \in {"Ambiguous", "Definite"}
ASSUME RegisterMode \in {"HeadCurrent", "LastArrived"}

Heads == 1..MaxHead

VARIABLES
  localHead,    \* sender's current head; monotone, rederived at retry
  marker,       \* sender's durable scope obligation (a presence marker)
  awaiting,     \* head of the live attempt, 0 = none (single-flight)
  inflight,     \* request copies still on the wire, per head
  arrived,      \* heads the receiver has seen at least once (ghost)
  replyReady,   \* a reply for `awaiting` is on the return wire
  registered,   \* receiver's durable registered head
  merged,       \* receiver's merged head
  obligations,  \* registrations created, per head
  losses

vars == <<localHead, marker, awaiting, inflight, arrived, replyReady,
          registered, merged, obligations, losses>>

Copies == IF DupMode = "Duplicate" THEN 2 ELSE 1

\* Go's two-stream reply needs a fresh dial back to the sender.  A relay-only
\* path authenticates the origin but gives no reverse route.
ReplyRoutable == RouteMode = "Routable" \/ ReplyMode = "SameStream"

TypeOK ==
  /\ localHead \in 0..MaxHead
  /\ marker \in BOOLEAN
  /\ awaiting \in 0..MaxHead
  /\ inflight \in [Heads -> 0..2]
  /\ arrived \subseteq Heads
  /\ replyReady \in BOOLEAN
  /\ registered \in 0..MaxHead
  /\ merged \in 0..MaxHead
  /\ obligations \in [Heads -> 0..3]
  /\ losses \in 0..MaxLoss

Init ==
  /\ localHead = 0
  /\ marker = FALSE
  /\ awaiting = 0
  /\ inflight = [h \in Heads |-> 0]
  /\ arrived = {}
  /\ replyReady = FALSE
  /\ registered = 0
  /\ merged = 0
  /\ obligations = [h \in Heads |-> 0]
  /\ losses = 0

\* A local write advances the head and dirties the scope marker.
Write ==
  /\ localHead < MaxHead
  /\ localHead' = localHead + 1
  /\ marker' = TRUE
  /\ UNCHANGED <<awaiting, inflight, arrived, replyReady, registered, merged,
                 obligations, losses>>

\* One attempt at a time, and it always carries the CURRENT head: the sender
\* holds a marker, not a payload, so a retry rederives rather than replays.
Send ==
  /\ marker
  /\ localHead > 0
  /\ awaiting = 0
  /\ awaiting' = localHead
  /\ inflight' = [inflight EXCEPT ![localHead] = Copies]
  /\ UNCHANGED <<localHead, marker, arrived, replyReady, registered, merged,
                 obligations, losses>>

\* An ordered link cannot hand over a head while an older copy is still out.
CanDeliver(h) ==
  \/ LinkMode = "Reorder"
  \/ \A g \in Heads : (g < h) => inflight[g] = 0

IsNewWork(h) ==
  IF RegisterMode = "HeadCurrent" THEN h > registered ELSE h # registered

Deliver(h) ==
  /\ inflight[h] > 0
  /\ CanDeliver(h)
  /\ inflight' = [inflight EXCEPT ![h] = @ - 1]
  /\ arrived' = arrived \cup {h}
  /\ registered' = IF RegisterMode = "HeadCurrent"
                   THEN (IF h > registered THEN h ELSE registered)
                   ELSE h
  /\ obligations' = IF IsNewWork(h)
                    THEN [obligations EXCEPT ![h] = @ + 1]
                    ELSE obligations
  /\ UNCHANGED <<localHead, marker, awaiting, replyReady, merged, losses>>

SendReply ==
  /\ awaiting # 0
  /\ awaiting \in arrived
  /\ ~replyReady
  /\ ReplyRoutable
  /\ replyReady' = TRUE
  /\ UNCHANGED <<localHead, marker, awaiting, inflight, arrived, registered,
                 merged, obligations, losses>>

\* The head-current guard: an ack retires the marker only if it acknowledges
\* the head the sender still considers current.
AckReply ==
  /\ replyReady
  /\ replyReady' = FALSE
  /\ marker' = IF awaiting = localHead THEN FALSE ELSE marker
  /\ awaiting' = 0
  /\ UNCHANGED <<localHead, inflight, arrived, registered, merged,
                 obligations, losses>>

\* A timeout fires only when the reply genuinely cannot arrive.  Copies already
\* on the wire are NOT recalled: a timed-out request may still be delivered.
Timeout ==
  /\ awaiting # 0
  /\ ~replyReady
  /\ (awaiting \notin arrived \/ ~ReplyRoutable)
  /\ awaiting' = 0
  /\ marker' = IF TimeoutMode = "Definite" THEN FALSE ELSE marker
  /\ UNCHANGED <<localHead, inflight, arrived, replyReady, registered, merged,
                 obligations, losses>>

Lose(h) ==
  /\ inflight[h] > 0
  /\ losses < MaxLoss
  /\ inflight' = [inflight EXCEPT ![h] = @ - 1]
  /\ losses' = losses + 1
  /\ UNCHANGED <<localHead, marker, awaiting, arrived, replyReady, registered,
                 merged, obligations>>

Merge ==
  /\ registered > merged
  /\ merged' = registered
  /\ UNCHANGED <<localHead, marker, awaiting, inflight, arrived, replyReady,
                 registered, obligations, losses>>

Next ==
  \/ Write \/ Send \/ SendReply \/ AckReply \/ Timeout \/ Merge
  \/ \E h \in Heads : Deliver(h) \/ Lose(h)

Spec ==
  /\ Init /\ [][Next]_vars
  /\ WF_vars(Write) /\ WF_vars(Send) /\ WF_vars(SendReply)
  /\ WF_vars(AckReply) /\ WF_vars(Timeout) /\ WF_vars(Merge)
  /\ \A h \in Heads : WF_vars(Deliver(h))

INV_TypeOK == TypeOK

\* Safety: the current head is never forgotten by both ends at once.  It is
\* registered, or someone still owes it -- the sender's marker, a live attempt,
\* or a copy the link has not yet dropped.  A copy still on the wire is not a
\* lost update: fair delivery will land it.
INV_NoLostUpdate ==
  \/ localHead = 0
  \/ registered >= localHead
  \/ marker
  \/ awaiting = localHead
  \/ inflight[localHead] > 0

\* A durable registration never moves backwards, whatever the link does to
\* arrival order.  `obligations[h] >= 1` witnesses that h was once registered,
\* so a later `registered < h` is a rollback.
INV_RegisteredMonotone ==
  \A h \in Heads : obligations[h] >= 1 => registered >= h

\* One head hint, one unit of work, however many copies the link delivers.
INV_ObligationIdempotent == \A h \in Heads : obligations[h] <= 1

\* Liveness: the write reaches the receiver's merged state.
LIVE_EventuallyMerged == <>[](merged = localHead)

\* Liveness: the sender stops retrying.  Needs the reply to be deliverable,
\* which under the two-stream protocol needs a reverse route.
LIVE_SenderQuiesces == <>[](marker = FALSE)
====
