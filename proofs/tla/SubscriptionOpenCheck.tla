------------------------- MODULE SubscriptionOpenCheck -------------------------
\* The story. A client opens a subscription. The server must tell it, as the
\* stream's first result, when the subscription's filter is invalid, and must
\* never lose an update that commits once the subscription is registered: the
\* stream (HTTP) or the subscription ID (FFI) reaches the client only after
\* that, so nothing the client can see happens between its request and the
\* registration, and an update committed in that gap is no more promised than
\* one committed before the request. So the server first subscribes to the
\* update events, and only then runs the request once through the plain path,
\* which checks the filter. A bad filter sends one error result and completes
\* the stream. A good one keeps streaming every update. The check can also
\* fail for a reason that passes (a store read fails, a provider answers
\* late): that says nothing about the request, so the stream is kept, and the
\* next update runs the request again, which reports a bad filter then as the
\* stream's last result. Go ends the stream on any failed check; this does
\* not. While the check runs the server may start closing; the check is raced
\* against closing, so the stream still ends.
\*
\* Who acts: the client's open request, a writer that commits updates, the
\* check, and the server's shutdown.
\* What must never happen: a bad filter streams a result or stays silent; a
\* good one is refused; an update committed after the registration is missed;
\* a bad request kept past a failure that passed is found bad by an update and
\* stays silent; closing leaves a stream open.
\*
\* This models `graphql_sse` in
\* `crates/surfaces/http/src/handlers/graphql/query.rs` (the `open_failure`
\* check before its event loop, which keeps the stream on
\* `OpenFailure::Transient`, and the event loop, which sends an update's error
\* and ends), `QueryError::is_invalid_request` in
\* `crates/engine/query/src/error.rs`, `subscribe` in
\* `crates/entry-points/ffi/src/query/exec.rs` (its open check, and its update
\* loop, which sends an update `update_refused` in
\* `crates/engine/query/src/subscription.rs` finds invalid and ends), and
\* `SubscriptionRequest::open` in
\* `tools/integration-test/src/actions/query/subscription.rs`.
EXTENDS Naturals

\* Bug names the guard a red run removes: "none", "skip_check",
\* "check_then_subscribe", "check_ignores_closing", "transient_refuses" or
\* "update_drops_invalid".
CONSTANT Bug

\* phase is where the open is: "asked", "subscribed", "checked", "done".
\* checked_early is TRUE once the check ran before the subscription existed.
\* valid says whether the client's filter is one its input type defines.
\* flaky is TRUE when the check meets a failure that passes, so it cannot tell
\* whether the filter is good.
\* commits counts committed updates (at most one); promised is TRUE when that
\* update committed while the subscription was registered and open (or, in
\* the red "check_then_subscribe" run, once its early check let it open), so
\* the client was promised it.
\* seen counts the updates the subscription received.
\* sent is what the stream sent: "nothing", "error", "results".
\* closing is TRUE once the server started shutting down.
\* hung is TRUE when the check is still running and will not finish alone.
VARIABLES valid, flaky, phase, checked_early, commits, promised, seen, sent, closing, hung
\* Every variable, for unchanged clauses.
vars == <<valid, flaky, phase, checked_early, commits, promised, seen, sent, closing, hung>>

\* Nothing has happened when the client asks; its filter is good or bad, and
\* its check will or will not meet a failure that passes.
Init == /\ valid \in BOOLEAN /\ flaky \in BOOLEAN
        /\ phase = "asked" /\ checked_early = FALSE
        /\ commits = 0 /\ promised = FALSE
        /\ seen = 0 /\ sent = "nothing" /\ closing = FALSE
        /\ hung = FALSE

\* The server subscribes to update events. The red "check_then_subscribe"
\* run checks first, so it only subscribes after the check.
Subscribe ==
  \* Only an open that was just asked subscribes.
  /\ phase = "asked"
  \* The correct order subscribes first.
  /\ Bug # "check_then_subscribe"
  \* Subscribed now; nothing else moves.
  /\ phase' = "subscribed"
  /\ UNCHANGED <<valid, flaky, checked_early, commits, promised, seen, sent, closing, hung>>

\* The red "check_then_subscribe" run checks before subscribing.
CheckFirst ==
  \* Only that red run takes this path.
  /\ Bug = "check_then_subscribe" /\ phase = "asked" /\ ~checked_early
  \* The check ran; the subscription does not exist yet.
  /\ checked_early' = TRUE
  /\ UNCHANGED <<valid, flaky, phase, commits, promised, seen, sent, closing, hung>>

\* That red run subscribes after its check.
SubscribeLate ==
  \* After its early check.
  /\ Bug = "check_then_subscribe" /\ phase = "asked" /\ checked_early
  \* A bad filter was found by the check: the stream ends with the error.
  \* A check that met a failure that passes found nothing, so it keeps it.
  /\ IF valid \/ flaky THEN phase' = "checked" /\ UNCHANGED sent
                       ELSE phase' = "done" /\ sent' = "error"
  /\ UNCHANGED <<valid, flaky, checked_early, commits, promised, seen, closing, hung>>

\* A writer commits one update. A subscription that exists receives it;
\* before the subscription exists it is gone.
Commit ==
  \* One update is enough to show a loss.
  /\ commits = 0
  \* It is committed.
  /\ commits' = 1
  \* The client was promised it while the subscription was registered and
  \* still open; the red run promises it from its early check on, before it
  \* registers anything. Before the registration nothing was promised.
  /\ promised' = (phase \in {"subscribed", "checked"} \/ (checked_early /\ phase = "asked"))
  \* Received only if the subscription already exists.
  /\ seen' = IF phase \in {"subscribed", "checked"} THEN 1 ELSE 0
  /\ UNCHANGED <<valid, flaky, phase, checked_early, sent, closing, hung>>

\* The check runs the request once through the plain path. It may hang (a
\* slow store); a hung check only ends when closing wins the race.
Check ==
  \* The check follows the subscription.
  /\ phase = "subscribed" /\ ~hung
  \* The red "skip_check" run treats every filter as valid. A failure that
  \* passes keeps the stream, whatever the filter; the red
  \* "transient_refuses" run ends the stream on it, as Go does.
  /\ LET good == valid \/ Bug = "skip_check"
         refused == IF flaky THEN Bug = "transient_refuses" ELSE ~good IN
       IF refused THEN phase' = "done" /\ sent' = "error"
                  ELSE phase' = "checked" /\ UNCHANGED sent
  /\ UNCHANGED <<valid, flaky, checked_early, commits, promised, seen, closing, hung>>

\* The check stalls instead of answering.
Hang ==
  \* Only a check that has not answered can stall.
  /\ phase = "subscribed" /\ ~hung
  /\ hung' = TRUE
  /\ UNCHANGED <<valid, flaky, phase, checked_early, commits, promised, seen, sent, closing>>

\* The server starts shutting down.
Close ==
  \* It happens once.
  /\ ~closing
  /\ closing' = TRUE
  /\ UNCHANGED <<valid, flaky, phase, checked_early, commits, promised, seen, sent, hung>>

\* Closing wins the race against a check that has not answered, and the
\* stream completes. The red "check_ignores_closing" run awaits the check
\* alone, so a hung check keeps the stream open.
CloseDuringCheck ==
  \* A check is in flight and closing has started.
  /\ phase = "subscribed" /\ closing
  \* The correct race.
  /\ Bug # "check_ignores_closing"
  \* The stream completes with nothing more to send.
  /\ phase' = "done"
  /\ UNCHANGED <<valid, flaky, checked_early, commits, promised, seen, sent, closing, hung>>

\* A kept subscription runs the request again for each update it received. A
\* good one streams the result and ends on closing. A bad one, kept past a
\* failure that passed, is found bad by that run: it sends the error and
\* completes. The red "skip_check" run checks nothing there either. The red
\* "update_drops_invalid" run skips the bad request's error as if the update
\* had no rows, and keeps the stream open with nothing to tell the client.
Stream ==
  \* Only after the check kept the stream.
  /\ phase = "checked"
  \* Whether this run accepts the request, and whether a bad one is reported.
  /\ LET good == valid \/ Bug = "skip_check"
         reports == ~good /\ Bug # "update_drops_invalid" IN
       \* Each received update becomes a result, or the bad request's error;
       \* the red run sends nothing for a bad one.
       /\ sent' = IF seen > 0 THEN (IF good THEN "results"
                                    ELSE IF reports THEN "error" ELSE sent)
                              ELSE sent
       \* Closing, or the bad request's error, completes the stream.
       /\ phase' = IF closing \/ (seen > 0 /\ reports) THEN "done" ELSE phase
  /\ UNCHANGED <<valid, flaky, checked_early, commits, promised, seen, closing, hung>>

\* Every step the system can take.
Next == \/ Subscribe \/ CheckFirst \/ SubscribeLate \/ Commit \/ Check
        \/ Hang \/ Close \/ CloseDuringCheck \/ Stream

\* Steps that keep being possible eventually happen; the check may still hang.
Spec == Init /\ [][Next]_vars
        /\ WF_vars(Subscribe) /\ WF_vars(CheckFirst) /\ WF_vars(SubscribeLate)
        /\ WF_vars(Close) /\ WF_vars(CloseDuringCheck) /\ WF_vars(Stream)

\* A bad filter never streams a result.
INV_BadNeverStreams == ~valid => sent # "results"

\* A bad filter's open that finished without a shutdown sent the error.
INV_BadIsReported == (~valid /\ phase = "done" /\ ~closing) => sent = "error"

\* A good filter is never refused, even when its check met a failure that
\* passes.
INV_GoodNeverRefused == valid => sent # "error"

\* An update committed while the subscription was registered and open
\* reaches a valid subscription.
INV_NoLostUpdate == (valid /\ promised) => seen = 1

\* Once the server closes, every stream ends, even behind a hung check.
LIVE_ClosingEnds == closing ~> (phase = "done")

\* A bad request kept past a failure that passed, once an update reaches it,
\* is eventually reported to the client.
LIVE_BadUpdateReported == (~valid /\ seen > 0 /\ phase = "checked") ~> (sent = "error")
================================================================================
