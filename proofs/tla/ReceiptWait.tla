---- MODULE ReceiptWait ----
\* The story, in plain words.
\*
\* The vera.rs provider sends a signed request to a Vera node and then waits
\* for the node to serve the request's receipt. The node serves a receipt
\* once the block that holds the request is finalized, and at that same
\* moment it publishes the block's header. The provider's light client
\* verifies each header and counts the newest verified height.
\*
\* So the provider does not wait on a fixed timer between asks. Each turn of
\* its loop (until_receipt in crates/engine/vera/src/vera_rs/receipt_wait.rs,
\* driven by resolve_pending in
\* crates/engine/vera/src/vera_rs/provider/submission.rs and by
\* ProviderChain::send_and_confirm in
\* crates/engine/vera/src/vera_rs/follow/chain.rs):
\*   1. reads the newest verified height, `seen`;
\*   2. asks the node for the receipt (a fresh request skips this the first
\*      time: it has not been sent yet), and is done if there is one;
\*   3. sends the request, unless a send is known to have reached the node;
\*   4. waits until the light client verifies a height above `seen`, or
\*      until a fallback interval passes, whichever comes first; and starts
\*      again.
\*
\* Who acts: the loop, the node (it finalizes blocks and puts the request in
\* one), the light client (it verifies headers, when its header stream is
\* up), and time (it passes while the loop waits).
\*
\* What must never happen:
\*   1. the light client has verified the block that holds the request, so
\*      the receipt is there to read, and the loop still sits waiting while
\*      time passes - INV_Prompt;
\*   2. the request is in a finalized block and the loop never reads its
\*      receipt, even though the header stream may be down for good
\*      - LIVE_Confirms.
\*
\* The Rust code each guard models:
\*   EventWake       Heads::above for AcpLightClient in receipt_wait.rs: the
\*                   wait is AcpLightClient::wait_for_height(seen + 1), which
\*                   the light client wakes at each verified header;
\*   SeenBeforeSend  until_receipt: `seen` is read at the top of the turn,
\*                   before the ask and the send, so a header verified while
\*                   either is on its way is above `seen` and ends the wait
\*                   at once;
\*   Bounded         until_receipt: every wait is given the fallback
\*                   (AcpTuning::receipt_fallback), so a turn ends even when
\*                   no header comes.
\*
\* Not modeled here: what PendingSubmission.tla models of the same loop
\* (a send that fails and is sent again, the node's dedupe, the deadline
\* over the whole loop, the journal). Time counts only while the loop waits:
\* an ask and a send are taken as instant next to the fallback.

EXTENDS Naturals  \* numbers, for heights and ticks

CONSTANTS
  EventWake,       \* TRUE: a verified height above `seen` ends the wait
  SeenBeforeSend,  \* TRUE: `seen` is read before the ask and the send
  Bounded,         \* TRUE: a wait ends after Fallback ticks without a header
  MaxHeight,       \* the highest block the node finalizes in a run
  Fallback         \* how many ticks a wait lasts without a header

VARIABLES
  phase,     \* where the loop is: "turn", "asking", "sending", "sent", "waiting" or "done"
  askNext,   \* TRUE when this turn asks for the receipt before it sends
  sent,      \* TRUE once a send is known to have reached the node
  seen,      \* the verified height the current wait waits to pass
  head,      \* the newest height the light client verified
  chain,     \* the newest height the node finalized
  landed,    \* the height of the block that holds the request, 0 while none does
  elapsed,   \* ticks the current wait has lasted
  lag        \* 1 once time passed in a wait after the request's block was verified

vars == <<phase, askNext, sent, seen, head, chain, landed, elapsed, lag>>  \* every variable

TypeOK ==
  /\ phase \in {"turn", "asking", "sending", "sent", "waiting", "done"}  \* a known step
  /\ askNext \in BOOLEAN                                                 \* a flag
  /\ sent \in BOOLEAN                                                    \* a flag
  /\ seen \in 0..MaxHeight                                               \* a height
  /\ head \in 0..MaxHeight                                               \* a height
  /\ chain \in 0..MaxHeight                                              \* a height
  /\ landed \in 0..MaxHeight                                             \* a height, or none
  /\ elapsed \in 0..Fallback                                             \* a short count
  /\ lag \in 0..1                                                        \* a flag as a number

Init ==
  /\ phase = "turn"            \* the loop starts a turn
  /\ askNext \in BOOLEAN       \* a recovered request asks first; a fresh one sends first
  /\ sent = FALSE              \* nothing is known to have been sent
  /\ seen = 0                  \* no wait has begun
  /\ head = 0                  \* the light client verified the first height
  /\ chain = 0                 \* and the node finalized nothing past it
  /\ landed = 0                \* the request is in no block
  /\ elapsed = 0               \* no time has passed
  /\ lag = 0                   \* and none was lost

\* The receipt is there to read: the request's block is finalized.
Served == landed # 0 /\ chain >= landed

\* The light client verified the request's block: the receipt is there and
\* the provider could know it without asking on a timer.
Visible == landed # 0 /\ head >= landed

\* ---- The loop ----

\* A turn begins. With SeenBeforeSend the loop reads the verified height
\* now, before it asks or sends.
Turn ==
  /\ phase = "turn"                                          \* a turn begins
  /\ seen' = IF SeenBeforeSend THEN head ELSE seen           \* read now, or later
  /\ phase' = IF askNext THEN "asking" ELSE "sending"        \* ask first, or send first
  /\ UNCHANGED <<askNext, sent, head, chain, landed, elapsed, lag>>

\* The loop asks and the node has the receipt: done.
Found ==
  /\ phase = "asking"          \* the loop asks
  /\ Served                    \* and the node serves the receipt
  /\ phase' = "done"           \* the loop is done
  /\ UNCHANGED <<askNext, sent, seen, head, chain, landed, elapsed, lag>>

\* The loop asks and the node has no receipt yet: send, or wait if a send
\* already reached the node.
Missing ==
  /\ phase = "asking"                               \* the loop asks
  /\ ~Served                                        \* and nothing is there yet
  /\ phase' = IF sent THEN "sent" ELSE "sending"    \* wait, or send first
  /\ UNCHANGED <<askNext, sent, seen, head, chain, landed, elapsed, lag>>

\* The loop sends and the node admits the request.
Send ==
  /\ phase = "sending"         \* the loop sends
  /\ sent' = TRUE              \* the node holds the request now
  /\ phase' = "sent"           \* the wait comes next
  /\ UNCHANGED <<askNext, seen, head, chain, landed, elapsed, lag>>

\* The wait begins. Without SeenBeforeSend the loop reads the verified
\* height only now, after the send: a header verified in between is no
\* longer above it.
StartWait ==
  /\ phase = "sent"                                          \* the send is done
  /\ seen' = IF SeenBeforeSend THEN seen ELSE head           \* kept, or read late
  /\ phase' = "waiting"                                      \* the loop waits
  /\ elapsed' = 0                                            \* its interval starts
  /\ UNCHANGED <<askNext, sent, head, chain, landed, lag>>

\* A verified height above `seen` ends the wait (only with EventWake).
Wake ==
  /\ phase = "waiting"         \* the loop waits
  /\ EventWake                 \* on headers
  /\ head > seen               \* and one above `seen` is verified
  /\ phase' = "turn"           \* a new turn
  /\ askNext' = TRUE           \* that asks
  /\ UNCHANGED <<sent, seen, head, chain, landed, elapsed, lag>>

\* The fallback interval passes with no header: the wait ends anyway (only
\* when the wait is Bounded).
Timer ==
  /\ phase = "waiting"         \* the loop waits
  /\ Bounded                   \* with a fallback
  /\ elapsed = Fallback        \* that has run out
  /\ phase' = "turn"           \* a new turn
  /\ askNext' = TRUE           \* that asks
  /\ UNCHANGED <<sent, seen, head, chain, landed, elapsed, lag>>

\* ---- Time ----

\* Time passes while the loop waits. A header above `seen` wakes a loop that
\* waits on headers before any time passes: the light client notifies the
\* waiter as it verifies the header. Time passing after the request's block
\* was verified is remembered in `lag`.
Tick ==
  /\ phase = "waiting"                          \* the loop waits
  /\ elapsed < Fallback                         \* its interval is not over
  /\ ~(EventWake /\ head > seen)                \* and no header has woken it
  /\ elapsed' = elapsed + 1                     \* a tick passes
  /\ lag' = IF Visible THEN 1 ELSE lag          \* lost, if the receipt was known to be there
  /\ UNCHANGED <<phase, askNext, sent, seen, head, chain, landed>>

\* ---- The node and the light client ----

\* The node puts the request in the next block. Heights are left for it:
\* the node finalizes empty blocks only while one height stays free.
Land ==
  /\ sent                      \* the node holds the request
  /\ landed = 0                \* in no block yet
  /\ chain < MaxHeight         \* and a height is free
  /\ landed' = chain + 1       \* the next block holds it
  /\ UNCHANGED <<phase, askNext, sent, seen, head, chain, elapsed, lag>>

\* The node finalizes its next block, which serves its receipts and
\* publishes its header at once.
Finalize ==
  /\ chain < MaxHeight                              \* heights remain
  /\ landed # 0 \/ chain + 1 < MaxHeight            \* one stays free for the request
  /\ chain' = chain + 1                             \* one more block
  /\ UNCHANGED <<phase, askNext, sent, seen, head, landed, elapsed, lag>>

\* The light client verifies the next published header. No fairness: its
\* header stream may be down for as long as a run lasts.
Verify ==
  /\ head < chain              \* a header it has not verified
  /\ head' = head + 1          \* now it has
  /\ UNCHANGED <<phase, askNext, sent, seen, chain, landed, elapsed, lag>>

\* Any one thing happens.
Next ==
  \/ Turn \/ Found \/ Missing \/ Send \/ StartWait   \* the loop's turn
  \/ Wake \/ Timer \/ Tick                          \* the wait
  \/ Land \/ Finalize \/ Verify                     \* the node and the light client

\* Fairness: the loop takes its steps, time passes, and the node holds and
\* finalizes the request. The light client is promised nothing.
Spec ==
  /\ Init                                                          \* start a turn
  /\ [][Next]_vars                                                 \* then take steps
  /\ WF_vars(Turn) /\ WF_vars(Found) /\ WF_vars(Missing)           \* the loop moves
  /\ WF_vars(Send) /\ WF_vars(StartWait)                           \* and sends
  /\ WF_vars(Wake) /\ WF_vars(Timer) /\ WF_vars(Tick)              \* waits end, time passes
  /\ WF_vars(Land) /\ WF_vars(Finalize)                            \* the node moves

\* ---- What must hold ----

\* No time passes in a wait once the light client verified the request's
\* block: the loop asks for the receipt as soon as it can know it is there.
INV_Prompt == lag = 0

\* The loop reads the receipt in the end, header stream or not.
LIVE_Confirms == <>(phase = "done")

====
