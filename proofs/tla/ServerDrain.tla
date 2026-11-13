---- MODULE ServerDrain ----
\* The story, in plain words.
\*
\* A node's servers (HTTP in crates/net/src/server.rs, Postgres in
\* crates/pg-compat/src/lib.rs) serve each connection as a task on the network
\* edge. A connection takes one request at a time (an HTTP/2 stream, a
\* Postgres message, a COPY up to its end), works on it, and answers it.
\* Shutdown runs the edge's rule: stop accepting, drain the work in flight up
\* to a deadline, then stop the rest.
\*
\* So when the edge starts closing:
\*   * the accept loop takes no new connection;
\*   * a connection busy with a request finishes it and answers, then closes;
\*   * an idle connection closes at once;
\*   * a subscription stream (a GraphQL subscription or the event stream
\*     over SSE), whose response never ends on its own, ends with its
\*     `complete` event, so the drain does not wait for it;
\*   * once every connection closed, the server returns; if the drain's
\*     deadline passes first, what is left is stopped then.
\*
\* What must never happen:
\*   1. a request in flight when closing began is dropped before the drain's
\*      deadline - INV_NoDropBeforeDeadline;
\*   2. a connection starts a new request after closing began
\*      - INV_NoNewWorkWhenClosing;
\*   3. the server never returns - LIVE_ServerReturns;
\*   4. the drain's deadline stops a subscription stream: an open stream
\*      holds shutdown for the whole deadline - INV_DeadlineStopsNoStream.
\*      The deadline may still stop an ordinary request that is stuck or
\*      just slow, as the code does (edge::Scope::drain logs it).
\*
\* The Rust code each guard models:
\*   Drained       the server's tasks are plain edge tasks the drain waits for
\*                 (crates/net/src/rt.rs Executor::execute; PgServer::run's
\*                 TaskSet in crates/pg-compat/src/lib.rs), not services the
\*                 edge stops as it begins closing;
\*   BetweenOnly   a connection looks at `closing()` between requests, never
\*                 inside one (net::server until_closing; wire::serve in
\*                 crates/pg-compat/src/wire/process.rs);
\*   CheckClosing  a connection does look, so it closes after its request;
\*   Deadline      the drain gives up at its deadline (edge::Scope::drain);
\*   EndStreams    a subscription stream ends once its edge closes
\*                 (crates/http/src/handlers/closing.rs next_unless_closing,
\*                 used by graphql_sse in
\*                 crates/http/src/handlers/graphql/query.rs and events_sse in
\*                 crates/http/src/handlers/events.rs).

EXTENDS Naturals

CONSTANTS
  Conns,         \* the connections
  Hangs,         \* the connections whose request may never finish (a stuck client or handler)
  Streams,       \* the connections whose request is a subscription stream, open until something ends it
  Drained,       \* TRUE: the drain waits for each connection's task
  BetweenOnly,   \* TRUE: closing is looked at only between requests
  CheckClosing,  \* TRUE: a connection looks at closing after each request
  Deadline,      \* TRUE: the drain has a deadline
  EndStreams     \* TRUE: a subscription stream ends once the edge began closing

VARIABLES
  conn,      \* each connection: "idle", "busy" (a request in flight) or "closed"
  closing,   \* the edge began closing
  expired,   \* the drain's deadline passed
  dropped,   \* (record) a request was dropped while in flight
  late,      \* (record) a request started after closing began
  returned,  \* the server returned
  cut        \* (record) the connections whose request the deadline stopped

vars == <<conn, closing, expired, dropped, late, returned, cut>>  \* every variable

\* Whether the request on `c` ends without the deadline once closing began:
\* an ordinary request that does not hang finishes, and a subscription
\* stream ends only if it looks at closing.
WillEnd(c) == c \notin Hangs /\ (c \notin Streams \/ EndStreams)

Init ==                                   \* at the start:
  /\ conn = [c \in Conns |-> "idle"]      \* every connection open and idle
  /\ closing = FALSE                      \* the edge accepts work
  /\ expired = FALSE                      \* no deadline passed
  /\ dropped = FALSE                      \* nothing dropped
  /\ late = FALSE                         \* nothing started late
  /\ returned = FALSE                     \* the server runs
  /\ cut = {}                             \* the deadline stopped nothing

\* A client sends a request on an idle connection, which starts working on it.
\* With CheckClosing broken, a connection keeps taking requests after closing began.
Start(c) ==
  /\ conn[c] = "idle"                                   \* between requests
  /\ ~returned                                          \* the server still runs
  /\ (~closing \/ ~CheckClosing)                        \* MUTATION no_check: keeps serving while closing
  /\ conn' = [conn EXCEPT ![c] = "busy"]                \* the request is in flight
  /\ late' = (late \/ closing)                          \* recorded if closing began
  /\ UNCHANGED <<closing, expired, dropped, returned, cut>>  \* the rest stays

\* The request finishes and is answered. A hung one may never get here, and
\* a subscription stream gets here on its own only by chance (a lost update,
\* a failed query).
Finish(c) ==
  /\ conn[c] = "busy"                                   \* in flight
  /\ conn' = [conn EXCEPT ![c] = "idle"]                \* answered
  /\ UNCHANGED <<closing, expired, dropped, late, returned, cut>>  \* the rest stays

\* A subscription stream sees the edge closing and ends: it sends its
\* `complete` event and its response ends, an answer and not a drop. Only a
\* stream looks at closing inside its request; it has no end of its own to
\* wait for. With EndStreams broken, it never does.
EndStream(c) ==
  /\ c \in Streams /\ conn[c] = "busy"                  \* a stream in flight
  /\ closing /\ EndStreams                              \* MUTATION stream_ignores_closing
  /\ conn' = [conn EXCEPT ![c] = "idle"]                \* answered with its complete event
  /\ UNCHANGED <<closing, expired, dropped, late, returned, cut>>  \* the rest stays

\* An idle connection sees the edge closing and closes.
Close(c) ==
  /\ conn[c] = "idle"                                   \* between requests
  /\ closing /\ CheckClosing                            \* it looks, and the edge is closing
  /\ conn' = [conn EXCEPT ![c] = "closed"]              \* closed
  /\ UNCHANGED <<closing, expired, dropped, late, returned, cut>>  \* the rest stays

\* The edge begins closing. As a service, every task is stopped at once, a
\* request in flight with it; a check inside a request cuts it off too.
BeginClosing ==
  /\ ~closing                                           \* once
  /\ closing' = TRUE                                    \* the edge stops accepting
  /\ IF Drained /\ BetweenOnly
       THEN UNCHANGED <<conn, dropped>>                 \* the tasks run on
       ELSE /\ dropped' = (dropped \/ \E c \in Conns : conn[c] = "busy")  \* MUTATION service / check_inside
            /\ conn' = [c \in Conns |-> "closed"]       \* every task stopped
  /\ UNCHANGED <<expired, late, returned, cut>>         \* the rest stays

\* The drain's deadline passes, once closing began; what is still open is stopped then.
\* An ordinary request may be slow or stuck, and the deadline may pass while
\* it is in flight and stop it. A stream that sees closing ends at once, far
\* inside the deadline (30 seconds), so the deadline never passes while such
\* a stream is still open; only a stream that will not end can be waiting.
Expire ==
  /\ closing /\ Deadline /\ ~expired                    \* after closing, with a deadline
  /\ \A c \in Streams : conn[c] = "busy" => ~WillEnd(c) \* a stream that will end is not waited out
  /\ expired' = TRUE                                    \* the deadline passed
  /\ cut' = {c \in Conns : conn[c] = "busy"}            \* the requests it stops
  /\ conn' = [c \in Conns |-> "closed"]                 \* the rest stopped, now allowed
  /\ UNCHANGED <<closing, dropped, late, returned>>     \* the rest stays

\* The server returns once every connection closed.
Return ==
  /\ closing /\ ~returned                               \* draining
  /\ \A c \in Conns : conn[c] = "closed"                \* nothing left
  /\ returned' = TRUE                                   \* the server returns
  /\ UNCHANGED <<conn, closing, expired, dropped, late, cut>>  \* the rest stays

Next ==
  \/ \E c \in Conns : Start(c) \/ Finish(c) \/ EndStream(c) \/ Close(c)
  \/ BeginClosing \/ Expire \/ Return

\* Ordinary requests on connections that do not hang always finish; a stream that looks at
\* closing ends; closing may begin at any moment or never; the deadline, once closing began
\* and no stream that will end is open, always comes; the server returns when it can.
Fairness ==
  /\ \A c \in Conns \ (Hangs \cup Streams) : WF_vars(Finish(c))
  /\ \A c \in Streams : WF_vars(EndStream(c))
  /\ \A c \in Conns : WF_vars(Close(c))
  /\ WF_vars(Expire)
  /\ WF_vars(Return)

Spec == Init /\ [][Next]_vars /\ Fairness

TypeOK ==
  /\ conn \in [Conns -> {"idle", "busy", "closed"}]
  /\ closing \in BOOLEAN
  /\ expired \in BOOLEAN
  /\ dropped \in BOOLEAN
  /\ late \in BOOLEAN
  /\ returned \in BOOLEAN
  /\ cut \subseteq Conns

\* A request in flight is only ever stopped by the drain's deadline.
INV_NoDropBeforeDeadline == dropped => expired
\* No request starts once the edge began closing.
INV_NoNewWorkWhenClosing == ~late
\* Once the edge began closing, the server returns.
LIVE_ServerReturns == closing ~> returned
\* The drain's deadline stops no subscription stream.
\* Expire's guard reads WillEnd, so this invariant catches a stream that
\* never looks at closing (Red_StreamIgnoresClosing). An EndStream that is
\* broken while EndStreams still says the stream ends is caught by
\* LIVE_ServerReturns instead: the stream stays open, Expire waits for it,
\* and the server never returns.
INV_DeadlineStopsNoStream == cut \cap Streams = {}
====
