---- MODULE VerifiedRead ----
\* The story, in plain words.
\*
\* A node asks Vera questions about access: who owns this document, may this
\* reader read it, what does this policy say. It does not trust the server's
\* word: the light client checks every answer against a proof, at the newest
\* chain state it has authenticated (a "state root"). A background loop keeps
\* that newest root current: each time the chain finalizes a block, the root
\* moves on.
\*
\* A question takes a round trip. If a block is finalized while the answer is
\* on its way back, the answer was proven at a root that is no longer the
\* newest one. The light client refuses it ("ACP state changed during
\* verification; retry the request"): the access could have changed in that
\* block. The refusal is not a failed read, only a late one. The same question
\* asked again is answered at the newer root.
\*
\* Example: the node asks "may Bob read doc 7?" at root 4. Block 5 finalizes
\* while the proof travels. The proof says root 4, the newest is 5: refused.
\* Asked again, the proof comes back at root 5, which is still the newest:
\* accepted.
\*
\* The bug this fixes: nobody asked again. A query of a document's commit
\* history turned the refusal into "unable to verify access to commit
\* history", although the reader was allowed and nothing had failed.
\*
\* The node can also answer late. It proves an answer with the certificate of
\* the block it was read at, and it waits for that certificate only so long
\* (two seconds), or it has no proof slot free. Then it answers "busy" (the
\* JSON-RPC code -32002) instead of a proof. That is not a refusal either:
\* the same question asked a moment later is answered. The bug: nobody asked
\* again, so a policy created under load ended in "created policy proof:
\* ... (-32002): current record evidence deadline exceeded" although the
\* policy was on the chain.
\*
\* A busy answer is now asked again after a pause, for as long as the read's
\* patience lasts. Patience is time, not a count of asks. It runs from the
\* read's first ask and counts everything: each request's own round trip as
\* well as the pauses. A node that takes its whole two-second deadline to say
\* "busy" spends the patience in far fewer asks than one that says it at
\* once. Each pause is drawn at random between a shortest and a longest one
\* (half of BUSY_PAUSE and all of it), so readers the node turned away
\* together come back at different moments instead of filling its proof
\* slots again together. The read pauses only when the pause it drew still
\* ends inside its patience; otherwise the busy answer is its outcome.
\*
\* Example (time in steps): patience 15, a request takes 1 or 2 steps, a
\* pause 1 or 2. A fast busy node: asked at 0, busy at 1, pause, asked at 2,
\* busy at 3, ... about seven asks. A slow busy node: asked at 0, busy at 2,
\* pause, asked at 4, busy at 6, ... about four asks. Both stop before an
\* ask would start at or after step 15.
\*
\* What must never happen:
\*   * an answer accepted although a newer root existed when it was accepted
\*     (it could grant an access the newer state revokes);
\*   * a read that ends in an error only because a block finalized during one
\*     of its asks while it still had asks left, or only because the node
\*     answered busy while a pause still fit in its patience;
\*   * a read asked again and again without end: more than Attempts asks for
\*     overtaken answers, or a busy read asked again at or after its patience
\*     ran out.
\* And what must happen: a read ends with an answer whenever the chain
\* finalizes fewer blocks than the read may ask, and the node answers busy
\* few enough times that the read's patience is never spent.
\*
\* Code this models (defradb.rs, light client at backbone rev 502e604):
\*   crates/engine/vera/src/vera_rs/provider/verified.rs   verified, ATTEMPTS, overtaken,
\*       verified_within, Pace, WallPace, JITTER, BUSY_PATIENCE, BUSY_PAUSE, busy
\*   crates/building-blocks/inputs/src/retry.rs                     Jitter::pause (a pause drawn
\*       between half of BUSY_PAUSE and all of it)
\*   crates/engine/vera/src/vera_rs/provider.rs            create_policy, query_policy,
\*       query_object_owner, verify_access, create_access_decision (each read
\*       goes through `verified`)
\*   acp-light-client src/lib.rs                    verify_access, read_object_owner,
\*       read_key (minimum = fresh_state height), verify_access_decision
\*   acp-light-client src/header_sync.rs            run_header_loop (root moves),
\*       HeaderChain::accept_response (the refusal)
\*
\* Correspondence table (model -> Rust):
\*   root, Advance        header_sync.rs run_header_loop / observe_revision: the
\*                        newest authenticated module_state_root moves on
\*   Ask                  lib.rs: `fresh_state()?.height` is the minimum, the
\*                        proof request leaves (one call of `read` in verified)
\*   readAt               the revision the proof server answers at (>= minimum)
\*   Return, CheckRoot    header_sync.rs accept_response: `current.module_state_root
\*                        == root` else the refusal; lib.rs verify_access_decision
\*                        compares the record's root with fresh_state the same way
\*   Retry, attempt       verified.rs `verified_within`: an `overtaken` error with
\*                        asks left asks again; any other outcome is returned
\*   Capped, Attempts     verified.rs ATTEMPTS: after that many asks the refusal
\*                        is returned; and the patience check before a busy pause
\*   Busy, busy           verified.rs `busy`: the error holds a node answer with
\*                        code -32002 (vera.rs: evidence not ready by its deadline,
\*                        or no proof slot free)
\*   clock                `Pace::elapsed` (WallPace: time since the first ask)
\*   took, MaxTook        how long one request takes to come back; MaxTook is a
\*                        node that waits out its whole evidence deadline
\*   pause, PauseMin,     `Pace::pause` (WallPace: JITTER.pause(BUSY_PAUSE)): a
\*   PauseMax             pause drawn between half of BUSY_PAUSE and all of it;
\*                        the model checks every draw
\*   RetryBusy, Patience  verified.rs `verified_within`: a busy answer pauses and
\*                        asks again only if `pace.elapsed() + pause < patience`
\*                        (BUSY_PATIENCE, 30 s); the model uses smaller numbers
\*   CountTook            the check reads `elapsed`, which includes the requests'
\*                        own time, not only the time slept
\*   CheckDrawn           the check uses the pause that is then slept, not a
\*                        shorter one
\*   paused               the time slept in pauses, no Rust counterpart (what a
\*                        check that forgot the requests' time would read)
\*   MaxBusy              how many busy answers the node gives in a run, no Rust
\*                        counterpart (keeps the model finite)
\*   stale, late          history variables, no Rust counterpart
\*   overtaken            history variable: refusals in a row for this read
\*
\* The spreading itself (two readers drawing different pauses) is a property
\* of the random draws, not of one read's steps; it is tested in Rust
\* (verified/tests.rs busy_retries_of_two_readers_are_spread). The model
\* shows that whichever pause is drawn, the read stays inside its patience.
EXTENDS Naturals

CONSTANTS
    Attempts,     \* ATTEMPTS in verified.rs: how many times one read may be asked for overtaken answers
    MaxAdvances,  \* how many blocks the chain finalizes in a run (keeps the model finite)
    CheckRoot,    \* TRUE: an answer proven at an older root is refused (accept_response)
    Retry,        \* TRUE: a refused read is asked again while it has asks left (the fix)
    Capped,       \* TRUE: overtaken asks stop at Attempts, busy asks at the patience
    MaxBusy,      \* how many busy answers the node gives in a run (keeps the model finite)
    RetryBusy,    \* TRUE: a read the node answered busy is asked again (the second fix)
    Patience,     \* BUSY_PATIENCE: how long after its first ask a busy read may still be asked
    MaxTook,      \* the longest a request takes to come back (a slow node); the shortest is 1
    PauseMin,     \* the shortest pause the jitter draws (half of BUSY_PAUSE)
    PauseMax,     \* the longest pause the jitter draws (BUSY_PAUSE)
    CountTook,    \* TRUE: the patience counts the requests' own time, not only the pauses
    CheckDrawn    \* TRUE: the patience is checked against the pause that is then slept

VARIABLES
    root,       \* the newest authenticated state root, a number that only grows
    advances,   \* how many blocks finalized so far in this run
    pc,         \* the read: "idle" (about to be asked), "inflight", "answered" or "failed"
    readAt,     \* the root the answer on its way back was proven at
    attempt,    \* which ask this is among those after an overtaken answer, starting at 1
    overtaken,  \* how many of this read's asks were refused because a block finalized
    stale,      \* TRUE once an answer was accepted while a newer root existed
    busy,       \* how many busy answers the node gave this read
    clock,      \* time since the read's first ask, in steps
    paused,     \* the part of that time spent in pauses
    late        \* TRUE once a busy read was asked again at or after its patience ran out

vars == <<root, advances, pc, readAt, attempt, overtaken, stale, busy, clock, paused, late>>

\* The lengths a request can take: 1 step for a quick node up to MaxTook for a slow one.
Took == 1..MaxTook

\* The pauses the jitter can draw.
Pauses == PauseMin..PauseMax

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ root \in Nat                                         \* roots are numbers
    /\ advances \in 0..MaxAdvances                          \* never more blocks than the bound
    /\ pc \in {"idle", "inflight", "answered", "failed"}    \* the read's four places
    /\ readAt \in Nat                                       \* a root number
    /\ attempt \in Nat \ {0}                                \* asks are counted from 1
    /\ overtaken \in Nat                                    \* a count
    /\ stale \in BOOLEAN                                    \* a flag
    /\ busy \in 0..MaxBusy                                  \* never more busy answers than the bound
    /\ clock \in Nat                                        \* a time
    /\ paused \in Nat                                       \* a time
    /\ late \in BOOLEAN                                     \* a flag

\* At the start: root 0, no block yet, the read about to be asked for the first time.
Init ==
    /\ root = 0          \* the light client authenticated its first state
    /\ advances = 0      \* no block finalized yet
    /\ pc = "idle"       \* the read is about to be asked
    /\ readAt = 0        \* no answer yet
    /\ attempt = 1       \* this will be the first ask
    /\ overtaken = 0     \* nothing refused yet
    /\ stale = FALSE     \* nothing accepted yet
    /\ busy = 0          \* the node has not answered busy yet
    /\ clock = 0         \* the first ask starts the clock
    /\ paused = 0        \* no pause yet
    /\ late = FALSE      \* no ask after the patience yet

\* A block finalizes: the header loop authenticates a newer root. This can
\* happen at any moment, also while a read is on the wire.
Advance ==
    /\ advances < MaxAdvances             \* the run still has a block to finalize
    /\ root' = root + 1                   \* the newest root moves on
    /\ advances' = advances + 1           \* one more block finalized
    /\ UNCHANGED <<pc, readAt, attempt, overtaken, stale, busy, clock, paused, late>>  \* the read does not notice yet

\* The read is asked. The light client takes the newest root's height as the
\* lowest revision it will accept, and the proof request leaves.
Ask ==
    /\ pc = "idle"         \* the read is about to be asked
    /\ pc' = "inflight"    \* the request is on the wire
    /\ readAt' = root      \* the server cannot answer below the newest root
    /\ UNCHANGED <<root, advances, attempt, overtaken, stale, busy, clock, paused, late>>  \* nothing else moves

\* The answer comes back, `took` steps after it was asked, and is checked.
\* The server answered at some root between the one asked for and the newest
\* one now (`served`). If that is the newest root, the answer is accepted.
\* If a block finalized in between, the light client refuses it; the read is
\* then asked again while it has asks left, else the refusal is the read's
\* outcome. The round trip's time counts against the patience too.
Return ==
    /\ pc = "inflight"                                \* an answer is on its way back
    /\ \E took \in Took :                             \* how long the round trip took
        clock' = clock + took                         \* the clock moves by it
    /\ \E served \in readAt..root :                   \* the root the server proved it at
        IF served = root \/ ~CheckRoot                \* newest root (or the check is skipped)
        THEN /\ pc' = "answered"                      \* the read ends with this answer
             /\ stale' = (served # root)              \* remember if a newer root existed
             /\ readAt' = served                      \* the answer's root
             /\ UNCHANGED <<attempt, overtaken>>      \* no more asks
        ELSE /\ overtaken' = overtaken + 1            \* one more refusal in a row
             /\ readAt' = served                      \* the refused answer's root
             /\ stale' = stale                        \* nothing was accepted
             /\ IF Retry /\ (attempt < Attempts \/ ~Capped)  \* asks left (or no cap)
                THEN /\ pc' = "idle"                  \* ask the same read again
                     /\ attempt' = attempt + 1        \* the next ask
                ELSE /\ pc' = "failed"                \* the refusal goes to the caller
                     /\ attempt' = attempt            \* no further ask
    /\ UNCHANGED <<root, advances, busy, paused, late>>  \* the chain does not move in this step

\* The node answers busy, `took` steps after it was asked, instead of a
\* proof: the certificate of the block the read asked at was not ready by
\* the node's deadline, or no proof slot was free. Nothing is accepted. The
\* read draws a pause. If that pause still ends inside its patience, it
\* sleeps it and is asked again; else the busy answer is the read's outcome.
\* The overtaken count is left alone: the two kinds of asking again are
\* counted apart.
Busy ==
    /\ pc = "inflight"                                \* an answer is due
    /\ busy < MaxBusy                                 \* the run still has a busy answer to give
    /\ busy' = busy + 1                               \* one more busy answer
    /\ \E took \in Took, pause \in Pauses :           \* how long the request took; the pause drawn
        LET now == clock + took                       \* the time when the busy answer arrives
            \* what the patience check reads as time spent: all of it, or
            \* (the planted bug) only the pauses
            spent == IF CountTook THEN now ELSE paused
            \* the pause the check adds: the drawn one, or (the planted bug)
            \* the shortest one, although the drawn one is slept
            checked == IF CheckDrawn THEN pause ELSE PauseMin
        IN IF RetryBusy /\ (spent + checked < Patience \/ ~Capped)  \* the pause fits (or no cap)
           THEN /\ pc' = "idle"                       \* sleep the pause, then ask again
                /\ clock' = now + pause               \* the clock moves by the request and the pause
                /\ paused' = paused + pause           \* the time slept grows by the pause
                /\ late' = (late \/ now + pause >= Patience)  \* remember an ask past the patience
           ELSE /\ pc' = "failed"                     \* the busy answer goes to the caller
                /\ clock' = now                       \* the clock moves by the request only
                /\ UNCHANGED <<paused, late>>         \* no pause, no further ask
    /\ UNCHANGED <<root, advances, readAt, attempt, overtaken, stale>>  \* nothing else moves

\* Once the read ended, nothing more happens to it; blocks may still finalize.
Done ==
    /\ pc \in {"answered", "failed"}   \* the read ended
    /\ UNCHANGED vars                  \* stutter, so a finished run is not a deadlock

Next == Advance \/ Ask \/ Return \/ Busy \/ Done   \* one of these happens at each step

\* The read keeps going: a read that can be asked is asked, and an answer on
\* the wire comes back. Blocks and busy answers need no fairness: they may
\* stop at any time.
Spec == Init /\ [][Next]_vars /\ WF_vars(Ask) /\ WF_vars(Return)

\* An answer is only ever accepted at the newest authenticated root.
INV_NeverStale == ~stale

\* A read ends in an error only after the chain overtook every one of its
\* asks, or once even the longest pause would carry its next ask past the
\* patience.
INV_FailsOnlyWhenEveryAskOvertaken ==
    pc = "failed" => (overtaken = Attempts \/ clock + PauseMax >= Patience)

\* A read is never asked more than Attempts times for overtaken answers, and
\* a busy read is never asked again at or after its patience ran out.
INV_AttemptsBounded == attempt <= Attempts /\ ~late

\* When the chain finalizes fewer blocks than the read may ask, and the node
\* answers busy few enough times that the patience is never spent, the read
\* ends with an answer.
LIVE_Answers == <>(pc = "answered")
====
