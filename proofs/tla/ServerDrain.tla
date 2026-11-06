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
\*   * once every connection closed, the server returns; if the drain's
\*     deadline passes first, what is left is stopped then.
\*
\* What must never happen:
\*   1. a request in flight when closing began is dropped before the drain's
\*      deadline - INV_NoDropBeforeDeadline;
\*   2. a connection starts a new request after closing began
\*      - INV_NoNewWorkWhenClosing;
\*   3. the server never returns - LIVE_ServerReturns.
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
\*   Deadline      the drain gives up at its deadline (edge::Scope::drain).

EXTENDS Naturals

CONSTANTS
  Conns,         \* the connections
  Hangs,         \* the connections whose request may never finish (a stuck client or handler)
  Drained,       \* TRUE: the drain waits for each connection's task
  BetweenOnly,   \* TRUE: closing is looked at only between requests
  CheckClosing,  \* TRUE: a connection looks at closing after each request
  Deadline       \* TRUE: the drain has a deadline

VARIABLES
  conn,      \* each connection: "idle", "busy" (a request in flight) or "closed"
  closing,   \* the edge began closing
  expired,   \* the drain's deadline passed
  dropped,   \* (record) a request was dropped while in flight
  late,      \* (record) a request started after closing began
  returned   \* the server returned

vars == <<conn, closing, expired, dropped, late, returned>>  \* every variable

Init ==                                   \* at the start:
  /\ conn = [c \in Conns |-> "idle"]      \* every connection open and idle
  /\ closing = FALSE                      \* the edge accepts work
  /\ expired = FALSE                      \* no deadline passed
  /\ dropped = FALSE                      \* nothing dropped
  /\ late = FALSE                         \* nothing started late
  /\ returned = FALSE                     \* the server runs

\* A client sends a request on an idle connection, which starts working on it.
\* With CheckClosing broken, a connection keeps taking requests after closing began.
Start(c) ==
  /\ conn[c] = "idle"                                   \* between requests
  /\ ~returned                                          \* the server still runs
  /\ (~closing \/ ~CheckClosing)                        \* MUTATION no_check: keeps serving while closing
  /\ conn' = [conn EXCEPT ![c] = "busy"]                \* the request is in flight
  /\ late' = (late \/ closing)                          \* recorded if closing began
  /\ UNCHANGED <<closing, expired, dropped, returned>>  \* the rest stays

\* The request finishes and is answered. A hung one may never get here.
Finish(c) ==
  /\ conn[c] = "busy"                                   \* in flight
  /\ conn' = [conn EXCEPT ![c] = "idle"]                \* answered
  /\ UNCHANGED <<closing, expired, dropped, late, returned>>  \* the rest stays

\* An idle connection sees the edge closing and closes.
Close(c) ==
  /\ conn[c] = "idle"                                   \* between requests
  /\ closing /\ CheckClosing                            \* it looks, and the edge is closing
  /\ conn' = [conn EXCEPT ![c] = "closed"]              \* closed
  /\ UNCHANGED <<closing, expired, dropped, late, returned>>  \* the rest stays

\* The edge begins closing. As a service, every task is stopped at once, a
\* request in flight with it; a check inside a request cuts it off too.
BeginClosing ==
  /\ ~closing                                           \* once
  /\ closing' = TRUE                                    \* the edge stops accepting
  /\ IF Drained /\ BetweenOnly
       THEN UNCHANGED <<conn, dropped>>                 \* the tasks run on
       ELSE /\ dropped' = (dropped \/ \E c \in Conns : conn[c] = "busy")  \* MUTATION service / check_inside
            /\ conn' = [c \in Conns |-> "closed"]       \* every task stopped
  /\ UNCHANGED <<expired, late, returned>>              \* the rest stays

\* The drain's deadline passes, once closing began; what is still open is stopped then.
Expire ==
  /\ closing /\ Deadline /\ ~expired                    \* after closing, with a deadline
  /\ expired' = TRUE                                    \* the deadline passed
  /\ conn' = [c \in Conns |-> "closed"]                 \* the rest stopped, now allowed
  /\ UNCHANGED <<closing, dropped, late, returned>>     \* the rest stays

\* The server returns once every connection closed.
Return ==
  /\ closing /\ ~returned                               \* draining
  /\ \A c \in Conns : conn[c] = "closed"                \* nothing left
  /\ returned' = TRUE                                   \* the server returns
  /\ UNCHANGED <<conn, closing, expired, dropped, late>>  \* the rest stays

Next ==
  \/ \E c \in Conns : Start(c) \/ Finish(c) \/ Close(c)
  \/ BeginClosing \/ Expire \/ Return

\* Requests on connections that do not hang always finish; closing may begin at any moment or
\* never; the deadline, once closing began, always comes; the server returns when it can.
Fairness ==
  /\ \A c \in Conns \ Hangs : WF_vars(Finish(c))
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

\* A request in flight is only ever stopped by the drain's deadline.
INV_NoDropBeforeDeadline == dropped => expired
\* No request starts once the edge began closing.
INV_NoNewWorkWhenClosing == ~late
\* Once the edge began closing, the server returns.
LIVE_ServerReturns == closing ~> returned
====
