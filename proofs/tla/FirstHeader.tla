---- MODULE FirstHeader ----
\* The story, in plain words.
\*
\* A node runs a Vera node in its own process. The node's light client
\* learns what Vera has finalized from a header stream: it subscribes, and
\* the Vera node then sends the header of every block that finalizes after
\* that moment, and nothing from before. The light client checks each header
\* against the block's finalization certificate and its trusted consensus
\* key, and only then takes it as its head. The node does not start until the
\* light client has a head.
\*
\* A fresh deployment finalizes its first block while it starts, before the
\* light client subscribes. A lone validator then proposes its next block
\* only when a request comes or its heartbeat fires, ten seconds later. With
\* nothing streamed for the block that is already there, the start waited
\* those ten seconds.
\*
\* The transport between the two (vera-embedded EmbeddedTransport, the
\* subscribe_headers function in crates/engine/vera-embedded/src/acp.rs) now starts the stream
\* with the header of the newest finalized block, read once the subscription
\* stands (crates/engine/vera-embedded/src/acp/head.rs finalized). The transport is
\* not trusted: what it sends may be a forged header, and the light client
\* checks the first header exactly as it checks the others (acp-light-client
\* header_sync.rs run_header_loop, authenticate_header).
\*
\* What must never happen:
\*   1. the light client takes as its head a header no finalized block
\*      certifies - INV_OnlyFinalized;
\*   2. the stream has delivered everything it holds, no further block has
\*      come, and the light client is still behind the newest finalized
\*      block: it waits for a block nothing has asked for - INV_Current.
\*
\* The Rust code each guard models:
\*   HeadFirst           the head that starts the stream (acp.rs
\*                       subscribe_headers, head.rs finalized);
\*   SubscribeFirst      the head is read only after the subscription is
\*                       accepted, so a block that finalizes in between is
\*                       streamed (acp.rs subscribe_headers);
\*   Verify              the light client's check of every header against
\*                       its trusted key (acp-light-client header_sync.rs
\*                       authenticate_header, term::certify).
\*
\* Not modelled: the head older than the light client's freshness limit is
\* not sent (head.rs recent). The light client refuses such a head anyway,
\* and the next block after a restart brings a fresh one; skipping it only
\* saves the refused read.

EXTENDS Naturals, Sequences

\* MaxHeight: the most blocks the Vera node finalizes in one behaviour.
\* HeadFirst, SubscribeFirst, Verify: the three guards above; a RED
\* configuration turns one off.
CONSTANTS MaxHeight, HeadFirst, SubscribeFirst, Verify

\* finalized: the height of the Vera node's newest finalized block (0: none).
\* pc:        where the transport is: "idle" before it subscribes, "read" once
\*            it read the head ahead of subscribing (the RED order),
\*            "subscribed" once the subscription stands, "streaming" once the
\*            head is queued.
\* head:      the header read for the start of the stream, or a sentinel
\*            record with height 0 while none is.
\* queue:     headers waiting for the light client, oldest first.
\* lc:        the light client's head: a header record, height 0 for none.
VARIABLES finalized, pc, head, queue, lc

vars == <<finalized, pc, head, queue, lc>>

\* A header names a height and whether a finalized block certifies it.
Header(h, genuine) == [height |-> h, genuine |-> genuine]

\* The empty head: nothing yet.
None == Header(0, TRUE)

\* Every value each variable may take.
TypeOK ==
    /\ finalized \in 0..MaxHeight                                  \* a height the node reached
    /\ pc \in {"idle", "read", "subscribed", "streaming"}           \* a transport step
    /\ head \in [height : 0..MaxHeight + 1, genuine : BOOLEAN]      \* a header or none
    /\ queue \in Seq([height : 0..MaxHeight + 1, genuine : BOOLEAN]) \* headers in line
    /\ lc \in [height : 0..MaxHeight + 1, genuine : BOOLEAN]        \* a header or none

\* The deployment may or may not have finalized its first block when the
\* story starts; the light client knows nothing yet.
Init ==
    /\ finalized \in 0..1     \* the first block is there already, or not yet
    /\ pc = "idle"            \* the transport has not subscribed
    /\ head = None            \* no head read
    /\ queue = <<>>           \* nothing streamed
    /\ lc = None              \* the light client has no head

\* A block finalizes: a request came or the heartbeat fired. Once the
\* subscription stands, its header is streamed.
Finalize ==
    /\ finalized < MaxHeight                                 \* room for one more block
    /\ finalized' = finalized + 1                            \* the block finalizes
    /\ queue' = IF pc \in {"subscribed", "streaming"}        \* a subscriber is there:
                  THEN Append(queue, Header(finalized + 1, TRUE)) \* its header is streamed
                  ELSE queue                                   \* nobody listens yet
    /\ UNCHANGED <<pc, head, lc>>                             \* nothing else moves

\* The right order: subscribe first. The Vera node streams from here on.
Subscribe ==
    /\ pc = "idle"                                  \* not subscribed yet
    /\ SubscribeFirst                               \* the transport keeps the right order
    /\ pc' = "subscribed"                           \* the subscription stands
    /\ UNCHANGED <<finalized, head, queue, lc>>     \* nothing else moves

\* The planted wrong order: the head is read before the subscription.
ReadEarly ==
    /\ pc = "idle"                                  \* not subscribed yet
    /\ ~SubscribeFirst                              \* the planted order
    /\ head' = Header(finalized, TRUE)              \* the head as it is now
    /\ pc' = "read"                                 \* read, not yet subscribed
    /\ UNCHANGED <<finalized, queue, lc>>           \* nothing else moves

\* The wrong order goes on: the subscription comes after the read.
SubscribeLate ==
    /\ pc = "read"                                  \* the head was read first
    /\ pc' = "subscribed"                           \* now the subscription stands
    /\ UNCHANGED <<finalized, head, queue, lc>>     \* nothing else moves

\* The transport puts the head in front of the stream: read now in the right
\* order, or the one read early in the wrong one. Without HeadFirst it puts
\* nothing there, as before.
QueueHead ==
    /\ pc = "subscribed"                                              \* the subscription stands
    /\ LET read == IF SubscribeFirst THEN Header(finalized, TRUE) ELSE head \* the head it has
       IN queue' = IF HeadFirst /\ read.height > 0                    \* a block to start with:
                     THEN <<read>> \o queue                           \* it goes first
                     ELSE queue                                        \* else nothing is added
    /\ pc' = "streaming"                                              \* the stream is handed over
    /\ UNCHANGED <<finalized, head, lc>>                              \* nothing else moves

\* The transport is not trusted: it may slip in a header no block certifies,
\* at any height.
Forge ==
    /\ pc \in {"subscribed", "streaming"}           \* there is a stream to slip into
    /\ Len(queue) < 2                               \* a short line keeps the model small
    /\ \E h \in 1..MaxHeight + 1 :                   \* at any height
          queue' = Append(queue, Header(h, FALSE))   \* a header nothing certifies
    /\ UNCHANGED <<finalized, pc, head, lc>>         \* nothing else moves

\* The light client takes the next header. With Verify it keeps only one a
\* finalized block certifies and that is newer than its head; without it,
\* any newer one.
Deliver ==
    /\ queue # <<>>                                                     \* a header waits
    /\ LET next == Head(queue)                                          \* the oldest one
       IN lc' = IF next.height > lc.height /\ (next.genuine \/ ~Verify) \* newer, and certified
                  THEN next                                              \* becomes the head
                  ELSE lc                                                \* else it is dropped
    /\ queue' = Tail(queue)                                             \* it leaves the line
    /\ UNCHANGED <<finalized, pc, head>>                                \* nothing else moves

\* Any one of the steps above.
Next ==
    \/ Finalize        \* a block finalizes
    \/ Subscribe       \* the transport subscribes
    \/ ReadEarly       \* (wrong order) the head is read first
    \/ SubscribeLate   \* (wrong order) then the subscription
    \/ QueueHead       \* the head goes in front of the stream
    \/ Forge           \* the transport slips in a forged header
    \/ Deliver         \* the light client takes the next header

\* Start, then take steps.
Spec == Init /\ [][Next]_vars

\* The light client's head is always a header a finalized block certifies.
INV_OnlyFinalized == lc.genuine

\* Once the stream has delivered all it holds, the light client stands on
\* the newest finalized block: nothing it waits for has yet to be asked for.
INV_Current ==
    (pc = "streaming" /\ queue = <<>>) => lc.height = finalized   \* drained means current

====
