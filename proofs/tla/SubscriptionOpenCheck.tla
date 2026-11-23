------------------------- MODULE SubscriptionOpenCheck -------------------------
\* The story. A client opens a subscription. The server must tell it, as the
\* stream's first result, when the subscription's filter is invalid, and must
\* never lose an update that commits after the client asked. So the server
\* first subscribes to the update events, and only then runs the request once
\* through the plain path, which checks the filter. A bad filter sends one
\* error result and completes the stream. A good one keeps streaming every
\* update. While the check runs the server may start closing; the check is
\* raced against closing, so the stream still ends.
\*
\* Who acts: the client's open request, a writer that commits updates, the
\* check, and the server's shutdown.
\* What must never happen: a bad filter streams a result or stays silent; an
\* update committed after the open is missed; closing leaves a stream open.
\*
\* This models `graphql_sse` in
\* `crates/surfaces/http/src/handlers/graphql/query.rs` (the `open_failure`
\* check before its event loop), `subscribe` in
\* `crates/entry-points/ffi/src/query/exec.rs`, and `SubscriptionRequest::open`
\* in `tools/integration-test/src/actions/query/subscription.rs`.
EXTENDS Naturals

\* Bug names the guard a red run removes: "none", "skip_check",
\* "check_then_subscribe" or "check_ignores_closing".
CONSTANT Bug

\* phase is where the open is: "asked", "subscribed", "checked", "done".
\* checked_early is TRUE once the check ran before the subscription existed.
\* valid says whether the client's filter is one its input type defines.
\* commits counts committed updates (at most one); late is TRUE when that
\* update committed after the check ran, so the client was promised it.
\* seen counts the updates the subscription received.
\* sent is what the stream sent: "nothing", "error", "results".
\* closing is TRUE once the server started shutting down.
\* hung is TRUE when the check is still running and will not finish alone.
VARIABLES valid, phase, checked_early, commits, late, seen, sent, closing, hung
\* Every variable, for unchanged clauses.
vars == <<valid, phase, checked_early, commits, late, seen, sent, closing, hung>>

\* Nothing has happened when the client asks; its filter is good or bad.
Init == /\ valid \in BOOLEAN /\ phase = "asked" /\ checked_early = FALSE
        /\ commits = 0 /\ late = FALSE
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
  /\ UNCHANGED <<valid, checked_early, commits, late, seen, sent, closing, hung>>

\* The red "check_then_subscribe" run checks before subscribing.
CheckFirst ==
  \* Only that red run takes this path.
  /\ Bug = "check_then_subscribe" /\ phase = "asked" /\ ~checked_early
  \* The check ran; the subscription does not exist yet.
  /\ checked_early' = TRUE
  /\ UNCHANGED <<valid, phase, commits, late, seen, sent, closing, hung>>

\* That red run subscribes after its check.
SubscribeLate ==
  \* After its early check.
  /\ Bug = "check_then_subscribe" /\ phase = "asked" /\ checked_early
  \* A bad filter was found by the check: the stream ends with the error.
  /\ IF valid THEN phase' = "checked" /\ UNCHANGED sent
              ELSE phase' = "done" /\ sent' = "error"
  /\ UNCHANGED <<valid, checked_early, commits, late, seen, closing, hung>>

\* A writer commits one update. A subscription that exists receives it;
\* before the subscription exists it is gone.
Commit ==
  \* One update is enough to show a loss.
  /\ commits = 0
  \* It is committed.
  /\ commits' = 1
  \* The client was promised it once the check had run.
  /\ late' = (checked_early \/ phase \in {"checked", "done"})
  \* Received only if the subscription already exists.
  /\ seen' = IF phase \in {"subscribed", "checked"} THEN 1 ELSE 0
  /\ UNCHANGED <<valid, phase, checked_early, sent, closing, hung>>

\* The check runs the request once through the plain path. It may hang (a
\* slow store); a hung check only ends when closing wins the race.
Check ==
  \* The check follows the subscription.
  /\ phase = "subscribed" /\ ~hung
  \* The red "skip_check" run treats every filter as valid.
  /\ LET good == valid \/ Bug = "skip_check" IN
       IF good THEN phase' = "checked" /\ UNCHANGED sent
               ELSE phase' = "done" /\ sent' = "error"
  /\ UNCHANGED <<valid, checked_early, commits, late, seen, closing, hung>>

\* The check stalls instead of answering.
Hang ==
  \* Only a check that has not answered can stall.
  /\ phase = "subscribed" /\ ~hung
  /\ hung' = TRUE
  /\ UNCHANGED <<valid, phase, checked_early, commits, late, seen, sent, closing>>

\* The server starts shutting down.
Close ==
  \* It happens once.
  /\ ~closing
  /\ closing' = TRUE
  /\ UNCHANGED <<valid, phase, checked_early, commits, late, seen, sent, hung>>

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
  /\ UNCHANGED <<valid, checked_early, commits, late, seen, sent, closing, hung>>

\* A valid subscription streams the updates it received, and ends on closing.
Stream ==
  \* Only after a passing check.
  /\ phase = "checked"
  \* Each received update becomes a result; closing completes the stream.
  /\ sent' = IF seen > 0 THEN "results" ELSE sent
  /\ phase' = IF closing THEN "done" ELSE phase
  /\ UNCHANGED <<valid, checked_early, commits, late, seen, closing, hung>>

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

\* An update committed after the check ran reaches a valid subscription.
INV_NoLostUpdate == (valid /\ late /\ phase = "checked") => seen = 1

\* Once the server closes, every stream ends, even behind a hung check.
LIVE_ClosingEnds == closing ~> (phase = "done")
================================================================================
