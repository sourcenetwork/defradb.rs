---- MODULE EmbeddedVeraStop ----
\* The story, in plain words.
\*
\* A node can run a Vera node in its own process and ask it for every
\* document access decision. Requests arrive while the node admits them, and
\* each one waits for a decision. The decision comes through a transport: a
\* hold on the Vera node's method table. That table keeps the Vera node's
\* state alive, and with it the lock on the Vera node's directory. A provider
\* built on the transport can outlive the Vera node.
\*
\* The Vera node ends in one of two ways. It can stop on its own (an actor
\* exits, its watchdog trips). Or the node's shutdown stops it
\* (defra-node Node::shutdown in crates/defra-node/src/lib.rs):
\*   1. stop admitting requests and drain those in flight, up to a deadline
\*      (NodeEdge::drain_requests). A request still waiting at the deadline
\*      is given up on, and shutdown goes on without it;
\*   2. tell the Vera node to stop, close its transports, and wait for its
\*      thread up to a deadline (vera-embedded HostedNode::stop);
\*   3. drain the rest of the node's network work and go on to close the
\*      store.
\* A Vera node told to stop may never finish: its runtime can hang while it
\* is dropped.
\*
\* After a shutdown the host may build the next node over the same directory.
\* Its Vera node waits for the directory's lock. While the earlier Vera node
\* still holds it, that wait would never end, so the start has a deadline
\* (vera-embedded HostedNode::start) and fails when it passes.
\*
\* What must never happen:
\*   1. a request is given a decision read from a state past its freshness
\*      limit. Document ACP remembers a decision under the verified state it
\*      was read at, and repeats it while that state is the light client's
\*      newest and still fresh. Once the Vera node is gone no newer state
\*      comes, so the remembered decision may be repeated until the state
\*      ages out, and never after; a decision not asked before is refused at
\*      once - INV_NoDecisionPastFreshness;
\*   2. a request the node admitted is refused because shutdown stopped the
\*      Vera node under it - INV_AdmittedRequestsDecided. The one exception
\*      is a request that outlived the request drain's deadline: shutdown
\*      stopped waiting for it, and it is refused, never answered from memory;
\*   3. a node that shut down and whose Vera node ended still holds the Vera
\*      node's directory, so the next node over that directory waits forever
\*      for its lock - INV_StoppedNodeFreesDirectory. A Vera node that hangs
\*      past the stop deadline has not ended, and keeps its directory;
\*   4. shutdown never returns - LIVE_ShutdownReturns;
\*   5. the next node's start neither succeeds nor fails
\*      - LIVE_NextStartReturns.
\*
\* The Rust code each guard models:
\*   FreshDecisions    crates/vera/src/vera_rs/provider.rs decision_state,
\*                     which names a state to remember decisions under only
\*                     while the light client holds it fresh, and
\*                     crates/vera-embedded/src/call.rs call, which refuses a
\*                     call whose link is closed or whose node has stopped.
\*                     Regressions:
\*                     a_remembered_decision_is_not_given_past_the_freshness_limit
\*                     in crates/vera-embedded/tests/stopped_node_decisions.rs
\*                     and once_the_node_stops_the_provider_verifies_no_new_decision in
\*                     crates/vera-embedded/tests/hosted_node.rs;
\*   StopAfterRequests crates/defra-node/src/lib.rs Node::shutdown, which
\*                     stops the Vera node after the request drain and P2P's
\*                     shutdown;
\*   StopDeadline      crates/vera-embedded/src/host.rs HostedNode::stop, a
\*                     wait bounded by the network runtime's clock;
\*   StartDeadline     crates/vera-embedded/src/host.rs HostedNode::start, a
\*                     wait bounded by the network runtime's clock.
\*                     Regression:
\*                     a_start_over_a_held_directory_fails_at_its_deadline in
\*                     crates/vera-embedded/tests/hosted_node.rs;
\*   ClosesLink        crates/vera-embedded/src/host.rs HostedNode::stop and
\*                     its Drop, which close crates/vera-embedded/src/link.rs
\*                     Link. Regression:
\*                     a_stopped_nodes_deployment_is_reopened_by_the_next in
\*                     crates/vera-embedded/tests/hosted_node.rs.

EXTENDS Naturals  \* numbers, for counting the waiting request

CONSTANTS
  FreshDecisions,     \* TRUE: a remembered decision is repeated only while fresh
  StopAfterRequests,  \* TRUE: shutdown drains requests before it stops Vera
  StopDeadline,       \* TRUE: the wait for Vera's thread ends at a deadline
  StartDeadline,      \* TRUE: the next node's wait for the directory ends at one
  ClosesLink          \* TRUE: stopping Vera closes its transports

VARIABLES
  phase,       \* which shutdown step the node is at
  vera,        \* "up", "stopping" (told to stop), "down" or "hung"
  endedBy,     \* what ended Vera: "nothing", "itself" or "shutdown"
  link,        \* TRUE while a transport still holds Vera's method table
  dirHeld,     \* TRUE while Vera's directory is locked
  admitting,   \* TRUE while the node admits requests
  inflight,    \* how many requests wait for a decision (0 or 1)
  overdue,     \* TRUE once the request drain gave up on a waiting request
  granted,     \* TRUE once a decision was given: what document ACP remembers
  fresh,       \* TRUE while the last verified state is within its freshness limit
  stale,       \* TRUE once a decision was given from a state past that limit
  next,        \* the next node's start: "none", "waiting", "started", "failed"
  vetoed       \* TRUE once shutdown's stop refused an admitted request

vars == <<phase, vera, endedBy, link, dirHeld, admitting, inflight, overdue,
          granted, fresh, stale, vetoed, next>>  \* every variable

Phases == {"Running", "Requests", "StopVera", "Drain", "Done"}  \* the steps

TypeOK ==
  /\ phase \in Phases                                  \* a known step
  /\ vera \in {"up", "stopping", "down", "hung"}       \* a known state
  /\ endedBy \in {"nothing", "itself", "shutdown"}     \* a known cause
  /\ link \in BOOLEAN                                  \* a flag
  /\ dirHeld \in BOOLEAN                               \* a flag
  /\ admitting \in BOOLEAN                             \* a flag
  /\ inflight \in 0..1                                 \* at most one request
  /\ overdue \in BOOLEAN                               \* a flag
  /\ granted \in BOOLEAN                               \* a flag
  /\ fresh \in BOOLEAN                                 \* a flag
  /\ stale \in BOOLEAN                                 \* a flag
  /\ vetoed \in BOOLEAN                                \* a flag
  /\ next \in {"none", "waiting", "started", "failed"} \* a known state

Init ==
  /\ phase = "Running"     \* the node serves
  /\ vera = "up"           \* its Vera node runs
  /\ endedBy = "nothing"   \* nothing ended it
  /\ link = TRUE           \* the provider holds its transport
  /\ dirHeld = TRUE        \* Vera holds its directory
  /\ admitting = TRUE      \* requests are admitted
  /\ inflight = 0          \* none waits yet
  /\ overdue = FALSE       \* the drain gave up on none
  /\ granted = FALSE       \* no decision was given yet
  /\ fresh = TRUE          \* the running node keeps the state fresh
  /\ stale = FALSE         \* none from a state past its limit
  /\ vetoed = FALSE        \* shutdown refused none
  /\ next = "none"         \* no next node yet

\* Whether a call through the transport reaches a running Vera node.
Reachable == link /\ vera = "up"

\* ---- What requests do ----

\* A request arrives and waits for its decision.
Arrive ==
  /\ admitting                   \* the node takes requests
  /\ inflight = 0                \* one at a time is enough to show the order
  /\ inflight' = 1               \* it waits
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, granted,
                 stale, vetoed, overdue, next, fresh>>

\* Vera answers and the provider verifies the answer.
Decide ==
  /\ inflight = 1                \* a request waits
  /\ Reachable                   \* Vera runs and the transport reaches it
  /\ inflight' = 0               \* the request is answered
  /\ granted' = TRUE             \* a decision now exists to remember
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, stale,
                 vetoed, overdue, next, fresh>>

\* Whether document ACP repeats a decision it remembers: one was given, and
\* the state it was read at is still fresh, or freshness is not asked about.
Remembered == granted /\ (fresh \/ ~FreshDecisions)

\* The transport reaches no running Vera node. A remembered decision is given
\* again; otherwise the request is refused, which is shutdown's doing when
\* shutdown stopped the node under it.
Unreachable ==
  /\ inflight = 1                                    \* a request waits
  /\ ~Reachable                                      \* nothing to ask
  /\ inflight' = 0                                   \* the request ends
  /\ stale' = (stale \/ (Remembered /\ ~fresh))      \* repeated past the limit
  /\ vetoed' = (vetoed \/ (~Remembered /\ endedBy = "shutdown" /\ ~overdue))
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, granted, overdue, next, fresh>>

\* ---- What Vera does on its own ----

\* Vera stops by itself. Its runtime is gone, but a transport still holding
\* its method table keeps the directory locked.
Fail ==
  /\ vera = "up"                 \* it was running
  /\ vera' = "down"              \* it is not any more
  /\ endedBy' = "itself"         \* nobody asked
  /\ dirHeld' = link             \* held while a transport holds the table
  /\ UNCHANGED <<phase, link, admitting, inflight, overdue, granted, stale, vetoed, next, fresh>>

\* With no Vera node running no newer state arrives, and the last verified
\* state passes its freshness limit.
Ages ==
  /\ vera # "up"                 \* nothing renews the state
  /\ fresh                       \* it was fresh
  /\ fresh' = FALSE              \* it is not any more
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, inflight, overdue,
                 granted, stale, vetoed, next>>

\* A Vera node told to stop finishes: its thread ends.
Settle ==
  /\ vera = "stopping"           \* it was told
  /\ vera' = "down"              \* it ended
  /\ dirHeld' = link             \* held while a transport holds the table
  /\ UNCHANGED <<phase, endedBy, link, admitting, inflight, overdue, granted, stale,
                 vetoed, next, fresh>>

\* A Vera node told to stop never finishes.
Hang ==
  /\ vera = "stopping"           \* it was told
  /\ vera' = "hung"              \* it will not end
  /\ UNCHANGED <<phase, endedBy, link, dirHeld, admitting, inflight, overdue, granted,
                 stale, vetoed, next, fresh>>

\* ---- The shutdown sequence ----

\* The caller begins a shutdown. With StopAfterRequests the request drain
\* comes first and closes admission; without it Vera is stopped at once,
\* while requests are still admitted.
Begin ==
  /\ phase = "Running"                                        \* not begun
  /\ phase' = IF StopAfterRequests THEN "Requests" ELSE "StopVera"
  /\ admitting' = ~StopAfterRequests                          \* closed, or not
  /\ UNCHANGED <<vera, endedBy, link, dirHeld, inflight, overdue, granted, stale,
                 vetoed, next, fresh>>

\* The request drain ends once no request waits.
RequestsDrained ==
  /\ phase = "Requests"          \* draining requests
  /\ inflight = 0                \* none waits
  /\ phase' = "StopVera"         \* Vera is next
  /\ UNCHANGED <<vera, endedBy, link, dirHeld, admitting, inflight, overdue, granted,
                 stale, vetoed, next, fresh>>

\* The request drain reaches its deadline with a request still waiting. It
\* gives up on that request and shutdown goes on.
RequestsOverdue ==
  /\ phase = "Requests"          \* draining requests
  /\ inflight = 1                \* one still waits
  /\ overdue' = TRUE             \* the drain gives up on it
  /\ phase' = "StopVera"         \* Vera is next
  /\ UNCHANGED <<vera, endedBy, link, dirHeld, admitting, inflight, granted,
                 stale, vetoed, next, fresh>>

\* Shutdown tells Vera to stop and, with ClosesLink, closes its transports. A
\* Vera node that already ended is not told again, but its transports close
\* all the same, and with them goes the hold on its directory.
TellStop ==
  /\ phase = "StopVera"                                  \* its turn
  /\ endedBy # "shutdown"                                \* not told yet
  /\ endedBy' = "shutdown"                               \* shutdown ends it
  /\ link' = (link /\ ~ClosesLink)                       \* closed, or kept
  /\ vera' = IF vera = "up" THEN "stopping" ELSE vera    \* told, if running
  /\ dirHeld' = IF vera = "down" THEN link' ELSE dirHeld \* freed if it ended
  /\ UNCHANGED <<phase, admitting, inflight, overdue, granted, stale, vetoed, next, fresh>>

\* The wait for Vera's thread ends: the thread finished, or the deadline
\* passed. Nothing is admitted from here on.
StopReturns ==
  /\ phase = "StopVera"                  \* waiting for Vera
  /\ endedBy = "shutdown"                \* it was told
  /\ vera = "down" \/ StopDeadline       \* it ended, or the wait gave up
  /\ phase' = "Drain"                    \* the rest of the network work
  /\ admitting' = FALSE                  \* the node's scope closes
  /\ UNCHANGED <<vera, endedBy, link, dirHeld, inflight, overdue, granted, stale,
                 vetoed, next, fresh>>

\* The node's drain ends once no request waits, and shutdown goes on to
\* close the store.
Drained ==
  /\ phase = "Drain"             \* draining the rest
  /\ inflight = 0                \* none waits
  /\ phase' = "Done"             \* shutdown returns
  /\ UNCHANGED <<vera, endedBy, link, dirHeld, admitting, inflight, overdue, granted,
                 stale, vetoed, next, fresh>>

\* ---- The next node ----

\* After the shutdown the host builds a node over the same directory. Its
\* Vera node waits for the directory.
StartNext ==
  /\ phase = "Done"              \* the first node shut down
  /\ next = "none"               \* no next node yet
  /\ next' = "waiting"           \* its Vera node waits for the lock
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, inflight,
                 overdue, granted, stale, vetoed, fresh>>

\* The directory is free: the next Vera node takes it and starts.
NextStarts ==
  /\ next = "waiting"            \* it waits
  /\ ~dirHeld                    \* nothing holds the directory
  /\ next' = "started"           \* it starts
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, inflight,
                 overdue, granted, stale, vetoed, fresh>>

\* The directory is still held when the start deadline passes: the start
\* fails and says so.
NextGivesUp ==
  /\ next = "waiting"            \* it waits
  /\ dirHeld                     \* the earlier Vera node holds the directory
  /\ StartDeadline               \* and the wait has a deadline
  /\ next' = "failed"            \* the start fails
  /\ UNCHANGED <<phase, vera, endedBy, link, dirHeld, admitting, inflight,
                 overdue, granted, stale, vetoed, fresh>>

\* Any one thing happens.
Next ==
  \/ Arrive \/ Decide \/ Unreachable            \* requests
  \/ Fail \/ Settle \/ Hang \/ Ages             \* Vera and its last state
  \/ Begin \/ RequestsDrained \/ RequestsOverdue  \* shutdown,
  \/ TellStop \/ StopReturns \/ Drained          \* step by step
  \/ StartNext \/ NextStarts \/ NextGivesUp      \* the next node

\* Fairness: a waiting request is answered or refused; a Vera node told to
\* stop ends or hangs; each shutdown step that can run does. Whether a
\* request arrives, whether Vera fails and when the caller shuts the node
\* down are not promised.
Spec ==
  /\ Init                                               \* start serving
  /\ [][Next]_vars                                      \* then take steps
  /\ WF_vars(Decide) /\ WF_vars(Unreachable)            \* requests move
  /\ WF_vars(Settle \/ Hang)                            \* Vera moves
  /\ WF_vars(RequestsDrained) /\ WF_vars(TellStop)      \* shutdown moves
  /\ WF_vars(StopReturns) /\ WF_vars(Drained)           \* to its end
  /\ WF_vars(NextStarts) /\ WF_vars(NextGivesUp)        \* the next start ends

\* ---- What must hold ----

\* No decision is given from a state past its freshness limit.
INV_NoDecisionPastFreshness == ~stale

\* Shutdown refuses no request the node admitted by stopping Vera under it,
\* unless the request outlived the request drain's deadline.
INV_AdmittedRequestsDecided == ~vetoed

\* Once the node shut down and its Vera node ended, the directory is free. A
\* Vera node that hung has not ended.
INV_StoppedNodeFreesDirectory ==
  (phase = "Done" /\ vera = "down") => ~dirHeld

\* A shutdown that began returns.
LIVE_ShutdownReturns == (phase # "Running") ~> (phase = "Done")

\* The next node's start ends: it starts, or it fails at its deadline.
LIVE_NextStartReturns == (next = "waiting") ~> (next \in {"started", "failed"})

====
