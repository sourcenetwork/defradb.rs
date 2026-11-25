---- MODULE FirstHeader ----
\* The story, in plain words.
\*
\* A node runs a Vera node in its own process. The node's light client
\* learns what Vera has finalized from a header stream: it subscribes, and
\* the Vera node then sends the header of every block that finalizes after
\* that moment, and nothing from before. The light client checks each header
\* against the block's finalization certificate and its trusted consensus
\* key, refuses one older than its freshness limit, and only then takes it
\* as its head. The node does not start until the light client has a head.
\*
\* A fresh deployment finalizes its first block while it starts, before the
\* light client subscribes. A lone validator then proposes its next block
\* only when a request comes or its heartbeat fires, ten seconds later. With
\* nothing streamed for the block that is already there, the start waited
\* those ten seconds.
\*
\* The transport between the two (vera-embedded EmbeddedTransport, the
\* subscribe_headers function in crates/engine/vera-embedded/src/acp.rs) now
\* starts the stream with the header of the newest finalized block, read once
\* the subscription stands (crates/engine/vera-embedded/src/acp/head.rs
\* finalized). A head older than the light client's freshness limit is not
\* sent (head.rs recent): the light client would refuse it, and the node's
\* next block brings a fresh one. The headers the node queued before the
\* head was read are passed on behind it (acp.rs prepend_head): verification
\* of the fetched head can fail transiently. A later term is also passed on: a
\* switch of the deployment's active device voids blocks above its cut, and
\* the light client reads such a header below its head to learn its head
\* was voided (acp-light-client header_sync.rs run_header_loop).
\*
\* The transport is not trusted: what it sends may be a forged header, and
\* the light client checks the first header exactly as it checks the others
\* (acp-light-client header_sync.rs authenticate_header).
\*
\* What must never happen:
\*   1. the light client takes as its head a header no finalized block
\*      certifies - INV_OnlyFinalized;
\*   2. the stream has delivered everything it holds, no further block has
\*      come, and the light client is not on the newest finalized block
\*      although that block is fresh: it waits for a block nothing has asked
\*      for - INV_Current;
\*   3. the transport sends a head the light client must refuse as too old
\*      - INV_SentHeadFresh.
\*
\* The Rust code each guard models:
\*   HeadFirst           the head that starts the stream (acp.rs
\*                       subscribe_headers, head.rs finalized);
\*   FreshOnly           a head past the freshness limit is not sent
\*                       (head.rs recent);
\*   SubscribeFirst      the head is read only after the subscription is
\*                       accepted, so a block that finalizes in between is
\*                       streamed (acp.rs subscribe_headers);
\*   KeepLaterTerms      a queued header of a later term than the head is
\*                       passed on even at or below it (acp.rs prepend_head);
\*   Verify              the light client's check of every header against
\*                       its trusted key and freshness limit
\*                       (acp-light-client header_sync.rs authenticate_header,
\*                       term::certify, observe_revision).

EXTENDS Naturals, Sequences

\* MaxHeight: the most blocks the Vera node finalizes in one behaviour.
\* MaxTerm:   the most switches of the active device in one behaviour.
\* HeadFirst, FreshOnly, SubscribeFirst, KeepLaterTerms, Verify: the guards
\* above; a RED configuration turns one off.
CONSTANTS MaxHeight, MaxTerm, HeadFirst, FreshOnly, SubscribeFirst,
          KeepLaterTerms, Verify

\* finalized: the height of the Vera node's newest finalized block (0: none).
\* term:      the term that block was finalized in.
\* staleTo:   blocks at or below this height are past the freshness limit:
\*            those the node finalized long before the story starts.
\* pc:        where the transport is: "idle" before it subscribes, "read" once
\*            it read the head ahead of subscribing (the RED order),
\*            "subscribed" once the subscription stands, "streaming" once the
\*            head is queued.
\* head:      the header read for the start of the stream, or none.
\* queue:     headers waiting for the light client, oldest first.
\* lc:        the light client's head, or none.
\* sentStale: the transport sent a head past the freshness limit.
VARIABLES finalized, term, staleTo, pc, head, queue, lc, sentStale

vars == <<finalized, term, staleTo, pc, head, queue, lc, sentStale>>

\* A header names a height, a term, whether a finalized block certifies it,
\* whether it is fresh, and whether it is the head the transport read.
Header(h, t, genuine, fresh, first) ==
    [height |-> h, term |-> t, genuine |-> genuine, fresh |-> fresh, first |-> first]

\* No header: nothing yet.
None == Header(0, 0, TRUE, TRUE, FALSE)

\* The header of the node's newest finalized block.
Newest(first) == Header(finalized, term, TRUE, finalized > staleTo, first)

\* Every header the model can make.
Headers == [height : 0..MaxHeight + 1, term : 0..MaxTerm, genuine : BOOLEAN,
            fresh : BOOLEAN, first : BOOLEAN]

\* Every value each variable may take.
TypeOK ==
    /\ finalized \in 0..MaxHeight                                  \* a height the node reached
    /\ term \in 0..MaxTerm                                         \* the newest block's term
    /\ staleTo \in 0..MaxHeight                                    \* what is past the limit
    /\ pc \in {"idle", "read", "subscribed", "streaming"}           \* a transport step
    /\ head \in Headers                                            \* a header or none
    /\ queue \in Seq(Headers)                                      \* headers in line
    /\ lc \in Headers                                              \* a header or none
    /\ sentStale \in BOOLEAN                                       \* a stale head was sent

\* The deployment may have finalized its first block just now (a fresh
\* start), long ago (a restart, the block past the freshness limit), or not
\* yet; the light client knows nothing yet.
Init ==
    /\ finalized \in 0..1          \* the first block is there already, or not yet
    /\ term = 0                    \* no switch yet
    /\ staleTo \in {0, finalized}  \* that block is fresh, or long past
    /\ pc = "idle"                 \* the transport has not subscribed
    /\ head = None                 \* no head read
    /\ queue = <<>>                \* nothing streamed
    /\ lc = None                   \* the light client has no head
    /\ sentStale = FALSE           \* nothing sent

\* What the subscription holds from now on: `h` appended once it stands.
Streamed(h) == IF pc \in {"subscribed", "streaming"} THEN Append(queue, h) ELSE queue

\* A block finalizes: a request came or the heartbeat fired. It is fresh,
\* and once the subscription stands its header is streamed.
Finalize ==
    /\ finalized < MaxHeight                                          \* room for one more
    /\ finalized' = finalized + 1                                     \* the block finalizes
    /\ queue' = Streamed(Header(finalized + 1, term, TRUE, TRUE, FALSE)) \* streamed
    /\ UNCHANGED <<term, staleTo, pc, head, lc, sentStale>>           \* nothing else moves

\* The active device switches: the next term voids the newest block, and its
\* first block finalizes at that height. Its header is fresh and streamed.
Switch ==
    /\ term < MaxTerm /\ finalized > 0                                \* a block to void
    /\ term' = term + 1                                               \* the next term
    /\ queue' = Streamed(Header(finalized, term + 1, TRUE, TRUE, FALSE)) \* streamed
    /\ staleTo' = IF staleTo >= finalized THEN finalized - 1 ELSE staleTo \* it is new
    /\ UNCHANGED <<finalized, pc, head, lc, sentStale>>               \* nothing else moves

\* The right order: subscribe first. The Vera node streams from here on.
Subscribe ==
    /\ pc = "idle"                                                    \* not subscribed yet
    /\ SubscribeFirst                                                 \* the right order
    /\ pc' = "subscribed"                                             \* the subscription stands
    /\ UNCHANGED <<finalized, term, staleTo, head, queue, lc, sentStale>>

\* The planted wrong order: the head is read before the subscription.
ReadEarly ==
    /\ pc = "idle"                                                    \* not subscribed yet
    /\ ~SubscribeFirst                                                \* the planted order
    /\ head' = Newest(TRUE)                                           \* the head as it is now
    /\ pc' = "read"                                                   \* read, not subscribed
    /\ UNCHANGED <<finalized, term, staleTo, queue, lc, sentStale>>

\* The wrong order goes on: the subscription comes after the read.
SubscribeLate ==
    /\ pc = "read"                                                    \* the head was read first
    /\ pc' = "subscribed"                                             \* now it stands
    /\ UNCHANGED <<finalized, term, staleTo, head, queue, lc, sentStale>>

\* The transport puts the head in front of the stream: read now in the right
\* order, or the one read early in the wrong one; with FreshOnly only a
\* fresh one. Without HeadFirst it puts nothing there, as before.
QueueHead ==
    /\ pc = "subscribed"                                              \* the subscription stands
    /\ LET read == IF SubscribeFirst THEN Newest(TRUE) ELSE head      \* the head it has
           sent == HeadFirst /\ read.height > 0 /\ (read.fresh \/ ~FreshOnly) \* sent?
       IN /\ queue' = IF sent THEN <<read>> \o queue ELSE queue       \* it goes first
          /\ head' = IF sent THEN read ELSE None                      \* the filter's measure
          /\ sentStale' = (sentStale \/ (sent /\ ~read.fresh))        \* a stale one sent
    /\ pc' = "streaming"                                              \* the stream is handed over
    /\ UNCHANGED <<finalized, term, staleTo, lc>>                     \* nothing else moves

\* The transport is not trusted: it may slip in a header no block certifies,
\* at any height and term.
Forge ==
    /\ pc \in {"subscribed", "streaming"}                             \* a stream to slip into
    /\ Len(queue) < 2                                                 \* keeps the model small
    /\ \E h \in 1..MaxHeight + 1, t \in 0..MaxTerm :                   \* anywhere
          queue' = Append(queue, Header(h, t, FALSE, TRUE, FALSE))    \* nothing certifies it
    /\ UNCHANGED <<finalized, term, staleTo, pc, head, lc, sentStale>>

\* The transport keeps queued headers. The red mode drops a queued header
\* at or below the prefetched head, including a later-term takeover.
Dropped(h) ==
    /\ ~KeepLaterTerms /\ ~h.first /\ head.height > 0                 \* planted bug
    /\ h.height <= head.height                                        \* at or below it

\* Whether the light client takes `h` over its head: a later term, or a
\* greater height in the same term; with Verify only a certified, fresh one.
Takes(h) ==
    /\ (h.term > lc.term \/ (h.term = lc.term /\ h.height > lc.height)) \* newer
    /\ ((h.genuine /\ h.fresh) \/ ~Verify)                             \* checked

\* The light client is handed the next header, unless the transport drops it.
Deliver ==
    /\ queue # <<>>                                                   \* a header waits
    /\ LET next == Head(queue)                                        \* the oldest one
       IN lc' = IF ~Dropped(next) /\ Takes(next) THEN next ELSE lc    \* taken or not
    /\ queue' = Tail(queue)                                           \* it leaves the line
    /\ UNCHANGED <<finalized, term, staleTo, pc, head, sentStale>>    \* nothing else moves

\* Any one of the steps above.
Next ==
    \/ Finalize        \* a block finalizes
    \/ Switch          \* the active device switches
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
\* the newest finalized block, in its term, unless that block is past the
\* freshness limit: then nothing fresh exists to stand on yet.
INV_Current ==
    (pc = "streaming" /\ queue = <<>> /\ finalized > staleTo)           \* drained, fresh
      => (lc.height = finalized /\ lc.term = term)                      \* current

\* The transport never sends a head the light client must refuse as too old.
INV_SentHeadFresh == ~sentStale

====
