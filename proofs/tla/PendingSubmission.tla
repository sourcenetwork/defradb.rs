---- MODULE PendingSubmission ----
\* The story, in plain words.
\*
\* The vera.rs provider changes Vera's state by submitting a signed request.
\* Its worker journals the request before anything is sent, and keeps it
\* until a verified receipt is acknowledged. Between the two, one loop drives
\* the request to its receipt (resolve_pending in
\* crates/vera/src/vera_rs/provider/submission.rs):
\*   1. ask the node for the receipt. If it has one, the loop is done;
\*   2. if it has none and no send is known to have reached the node, send
\*      the request;
\*   3. sleep, and start again.
\* The whole loop runs under one deadline (sync_timeout). When the deadline
\* passes the caller is told the confirmation is unavailable, and the request
\* stays in the journal: the next call finds it there and runs the loop again
\* (recover_pending, send_tx_with_sequence).
\*
\* A send can end in four ways. The node admits the request and says so. The
\* node admits it but the answer is lost, so the provider sees a timeout. The
\* request never reaches the node, and the provider sees the same timeout.
\* Or the node refuses it because it already holds a request with that
\* sequence; the provider takes a refusal as "it may be there" and waits.
\* The provider cannot tell the two timeouts apart, so it sends again, and
\* may send a request the node already holds. The node executes a request
\* once however often it is sent: its sequence is spent by the first.
\*
\* What must never happen:
\*   1. the request takes effect twice - INV_AtMostOneEffect;
\*   2. the loop reports a confirmation with no effect behind it
\*      - INV_ConfirmedHasEffect;
\*   3. the loop gives up while the deadline has not passed
\*      - INV_NoEarlyGiveUp;
\*   4. the request leaves the journal before it is confirmed
\*      - INV_RetainedUntilConfirmed;
\*   5. the loop neither confirms nor reaches its deadline
\*      - LIVE_ConfirmsUnlessDeadline.
\*
\* The Rust code each guard models:
\*   Resends          resolve_pending: a send that fails with a retryable
\*                    error leaves awaiting_receipt false, so the next turn
\*                    sends again (ClientError::retryable in
\*                    crates/vera/src/vera_rs/client.rs; an in-process call
\*                    that elapsed is classified the same way,
\*                    crates/vera-embedded/src/submission.rs);
\*   KeepsTrying      resolve_pending: a retryable error from a send or from
\*                    a receipt read does not end the loop;
\*   RetainsOnTimeout resolve_pending and NativeWorker::acknowledge in
\*                    crates/vera/src/vera_rs/worker.rs: only an acknowledged
\*                    receipt clears the journal;
\*   NodeDedupes      vera.rs itself: a native sequence is spent once. It is
\*                    the assumption the loop's resend rests on, and
\*                    Red_NoDedupe shows what the resend would do without it.
\*
\* Not modeled: a refusal of a request the node does not hold. The loop then
\* waits out its deadline and keeps the request, which is INV_NoEarlyGiveUp
\* and INV_RetainedUntilConfirmed again. Time is one deadline that passes or
\* does not; the 200 ms sleep between turns is not counted.

EXTENDS Naturals  \* numbers, for counting what the node holds and executes

CONSTANTS
  Resends,           \* TRUE: a send that timed out is sent again
  KeepsTrying,       \* TRUE: a retryable failure does not end the loop
  RetainsOnTimeout,  \* TRUE: the journal keeps the request past a deadline
  NodeDedupes        \* TRUE: the node executes a request once however often sent

VARIABLES
  loop,       \* "asking", "sending", "confirmed" or "unavailable"
  awaiting,   \* TRUE once the loop believes a send reached the node
  held,       \* how many copies of the request the node admitted (0..2)
  effects,    \* how many times the request took effect (0..2)
  journal,    \* TRUE while the worker still holds the request
  deadline    \* TRUE once the loop's deadline passed

vars == <<loop, awaiting, held, effects, journal, deadline>>  \* every variable

TypeOK ==
  /\ loop \in {"asking", "sending", "confirmed", "unavailable"}  \* a known step
  /\ awaiting \in BOOLEAN                                        \* a flag
  /\ held \in 0..2                                               \* a small count
  /\ effects \in 0..2                                            \* a small count
  /\ journal \in BOOLEAN                                         \* a flag
  /\ deadline \in BOOLEAN                                        \* a flag

Init ==
  /\ loop = "asking"      \* the loop begins by asking for the receipt
  /\ awaiting = FALSE     \* nothing is known to have been sent
  /\ held = 0             \* the node holds no copy
  /\ effects = 0          \* nothing took effect
  /\ journal = TRUE       \* the worker journaled the request first
  /\ deadline = FALSE     \* the deadline lies ahead

\* One more copy admitted, counted up to two: enough to tell one from many.
More(n) == IF n < 2 THEN n + 1 ELSE n

\* The loop's next step after a send it cannot be sure of: ask again, or,
\* without KeepsTrying, give up.
AfterTimeout == IF KeepsTrying THEN "asking" ELSE "unavailable"

\* ---- The loop ----

\* The node has a receipt: the loop acknowledges it and clears the journal.
ReceiptFound ==
  /\ loop = "asking"             \* the loop asks
  /\ effects > 0                 \* the request took effect
  /\ loop' = "confirmed"         \* the loop is done
  /\ journal' = FALSE            \* the worker lets the request go
  /\ UNCHANGED <<awaiting, held, effects, deadline>>

\* The node has no receipt yet and no send is known to have arrived: send.
NoReceiptSend ==
  /\ loop = "asking"             \* the loop asks
  /\ effects = 0                 \* no receipt yet
  /\ ~awaiting                   \* and nothing believed sent
  /\ loop' = "sending"           \* so it sends
  /\ UNCHANGED <<awaiting, held, effects, journal, deadline>>

\* The read of the receipt fails in a way that may pass. With KeepsTrying the
\* loop asks again; without, it gives up.
ReceiptReadFails ==
  /\ loop = "asking"             \* the loop asks
  /\ loop' = AfterTimeout        \* again, or not at all
  /\ UNCHANGED <<awaiting, held, effects, journal, deadline>>

\* The node admits the request and its answer comes back.
SendAdmitted ==
  /\ loop = "sending"            \* a send is under way
  /\ held = 0                    \* the node does not hold it yet
  /\ held' = 1                   \* now it does
  /\ awaiting' = TRUE            \* and the loop knows
  /\ loop' = "asking"            \* back to asking
  /\ UNCHANGED <<effects, journal, deadline>>

\* The node admits the request but the answer is lost: the loop sees a
\* timeout. With Resends it will send again; without, it only waits.
SendAdmittedUnheard ==
  /\ loop = "sending"            \* a send is under way
  /\ held' = More(held)          \* the node takes a copy
  /\ awaiting' = ~Resends        \* believed sent only if the loop never resends
  /\ loop' = AfterTimeout        \* ask again, or give up
  /\ UNCHANGED <<effects, journal, deadline>>

\* The request never reaches the node: the same timeout, nothing admitted.
SendLost ==
  /\ loop = "sending"            \* a send is under way
  /\ awaiting' = ~Resends        \* believed sent only if the loop never resends
  /\ loop' = AfterTimeout        \* ask again, or give up
  /\ UNCHANGED <<held, effects, journal, deadline>>

\* The node refuses a request whose sequence it already holds or spent. The
\* loop takes that as "it may be there" and waits for the receipt.
SendRefused ==
  /\ loop = "sending"            \* a send is under way
  /\ NodeDedupes                 \* a node that dedupes
  /\ held > 0                    \* already holds this one
  /\ awaiting' = TRUE            \* the loop waits
  /\ loop' = "asking"            \* back to asking
  /\ UNCHANGED <<held, effects, journal, deadline>>

\* The deadline passes: the caller is told the confirmation is unavailable.
\* The journal keeps the request when RetainsOnTimeout.
DeadlinePasses ==
  /\ loop \in {"asking", "sending"}      \* the loop is still running
  /\ deadline' = TRUE                    \* time is up
  /\ loop' = "unavailable"               \* the caller is told
  /\ journal' = (journal /\ RetainsOnTimeout)  \* kept, or dropped
  /\ UNCHANGED <<awaiting, held, effects>>

\* A later call finds the request in the journal and runs the loop again,
\* with a new deadline and no memory of what was sent.
CalledAgain ==
  /\ loop = "unavailable"        \* the last call gave up
  /\ journal                     \* the request is still there
  /\ loop' = "asking"            \* the loop starts over
  /\ awaiting' = FALSE           \* it remembers no send
  /\ deadline' = FALSE           \* and has a new deadline
  /\ UNCHANGED <<held, effects, journal>>

\* ---- The node ----

\* The node executes a request it holds. A node that dedupes executes it
\* once; one that does not executes every copy.
Execute ==
  /\ effects < (IF NodeDedupes THEN (IF held > 0 THEN 1 ELSE 0) ELSE held)
  /\ effects' = effects + 1      \* one more effect
  /\ UNCHANGED <<loop, awaiting, held, journal, deadline>>

\* Any one thing happens.
Next ==
  \/ ReceiptFound \/ NoReceiptSend \/ ReceiptReadFails     \* asking
  \/ SendAdmitted \/ SendAdmittedUnheard \/ SendLost       \* sending
  \/ SendRefused                                           \* refused
  \/ DeadlinePasses \/ CalledAgain                         \* time and callers
  \/ Execute                                               \* the node

\* Fairness: the loop takes its turns; a node that holds a request executes
\* it; of the sends the loop keeps making, one is eventually admitted and
\* heard, or refused as a duplicate. Whether the deadline passes and whether
\* the caller calls again are not promised.
Spec ==
  /\ Init                                                  \* start asking
  /\ [][Next]_vars                                         \* then take steps
  /\ WF_vars(ReceiptFound) /\ WF_vars(NoReceiptSend)       \* the loop moves
  /\ WF_vars(Execute)                                      \* the node moves
  /\ SF_vars(SendAdmitted \/ SendRefused)                  \* a send gets through

\* ---- What must hold ----

\* The request takes effect at most once, however often it was sent.
INV_AtMostOneEffect == effects <= 1

\* A confirmation is the receipt of an effect.
INV_ConfirmedHasEffect == (loop = "confirmed") => effects > 0

\* The loop reports the confirmation unavailable only once its deadline passed.
INV_NoEarlyGiveUp == (loop = "unavailable") => deadline

\* The worker keeps the request until its receipt is acknowledged.
INV_RetainedUntilConfirmed == journal \/ loop = "confirmed"

\* A loop whose deadline never passes confirms the request.
LIVE_ConfirmsUnlessDeadline == <>(loop = "confirmed" \/ deadline)

====
