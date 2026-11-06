---- MODULE EmbedderStream ----
\* The story. An application embedding a node registers its own stream
\* protocols. Peers open streams on the node's connections, each starting
\* with a tag. The node reads the request under the inbound budget, then
\* hands the stream to the application's handler, which runs on a task of
\* its own. A handler may finish, or it may hang forever.
\*
\* What can happen: a peer opens streams, some with a tag nobody registered;
\* handlers finish, hang or run out of time; the node revokes a peer; the
\* node shuts down.
\*
\* What must never happen: a stream with an unregistered tag reaches a
\* handler; a peer holds more than its share of the budget; a running
\* handler holds budget room (so a hung handler locks its peer out); a
\* revoked peer keeps an open connection; a handler outlives shutdown; or a
\* handler runs forever.
\*
\* Code this models:
\*   crates/p2p/src/iroh/endpoint_streams.rs admit_stream (Arrive, Busy,
\*     the tag check), handle_connection_streams (Handoff: the lease is
\*     dropped before serve_protocol_stream), serve_protocol_stream (Return,
\*     Deadline, Cut);
\*   crates/p2p/src/iroh/stream_protocols.rs StreamProtocols::get (the tag
\*     check);
\*   crates/p2p/src/iroh/endpoint_commands.rs handle_deny_peer (Revoke);
\*   crates/p2p/src/iroh/task_registry.rs TaskRegistry::close (Shutdown).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Streams,        \* every stream any peer opens in this run
  Peers,          \* every peer that opens a stream
  PeerOf,         \* the peer that opened each stream
  Registered,     \* the streams whose tag some protocol registered
  Stuck,          \* the streams whose handler never returns
  Share,          \* how many requests one peer may hold room for at once
  TagMode,        \* "Check": unknown tags are reset; "Any": nothing is checked
  LeaseMode,      \* "ReleaseOnHandoff": room goes back before the handler runs; "HoldUntilReturn": it is held until the handler returns
  DeadlineMode,   \* "Deadline": every handler has one; "None": a handler may run forever
  RevokeMode,     \* "Close": a revocation closes the peer's connection; "BarOnly": it only stops new ones
  ShutdownMode    \* "Abort": shutdown aborts every stream task; "None": tasks keep running

VARIABLES
  phase,          \* where each stream is: idle, reading, serving, ended or reset
  leased,         \* whether each stream holds room in the budget
  open,           \* whether each peer's connection is open
  revoked,        \* whether each peer was revoked
  down            \* whether the node has shut down

\* All the variables, for stuttering steps.
vars == <<phase, leased, open, revoked, down>>

\* The phases a stream can be in.
Phases == {"idle", "reading", "serving", "ended", "reset"}

\* The streams of peer p holding budget room right now.
Held(p) == {s \in Streams : leased[s] /\ PeerOf[s] = p}

\* Whether peer p may take room for one more request.
Room(p) == Cardinality(Held(p)) < Share

\* Nothing has happened yet: every connection open, no room taken.
Init == /\ phase = [s \in Streams |-> "idle"]
        \* no stream holds room
        /\ leased = [s \in Streams |-> FALSE]
        \* every peer is connected
        /\ open = [p \in Peers |-> TRUE]
        \* no peer is revoked
        /\ revoked = [p \in Peers |-> FALSE]
        \* the node is running
        /\ down = FALSE

\* A stream with a tag nobody registered is reset before anything is read.
Unknown(s) == /\ phase[s] = "idle"
              \* only when the node checks tags
              /\ TagMode = "Check"
              \* the stream's tag is not a registered one
              /\ s \notin Registered
              \* it is reset with the unknown-service code
              /\ phase' = [phase EXCEPT ![s] = "reset"]
              \* nothing else moves
              /\ UNCHANGED <<leased, open, revoked, down>>

\* A stream whose tag is known (or not checked) takes room and starts
\* reading its request, on a live connection of a running node.
Arrive(s) == /\ phase[s] = "idle"
             \* the tag passed the check, or there is no check
             /\ (TagMode = "Any" \/ s \in Registered)
             \* the node is running
             /\ ~down
             \* the connection carrying it is open
             /\ open[PeerOf[s]]
             \* its peer has room left in its share
             /\ Room(PeerOf[s])
             \* the stream reads its request
             /\ phase' = [phase EXCEPT ![s] = "reading"]
             \* holding room for it
             /\ leased' = [leased EXCEPT ![s] = TRUE]
             \* nothing else moves
             /\ UNCHANGED <<open, revoked, down>>

\* No room frees up before the admit deadline: the stream is reset busy.
Busy(s) == /\ phase[s] = "idle"
           \* its peer's share is full
           /\ ~Room(PeerOf[s])
           \* it is reset with the busy code
           /\ phase' = [phase EXCEPT ![s] = "reset"]
           \* nothing else moves
           /\ UNCHANGED <<leased, open, revoked, down>>

\* The request is read and the handler starts on the stream's own task.
Handoff(s) == /\ phase[s] = "reading"
              \* the connection is still open
              /\ open[PeerOf[s]]
              \* the handler is serving
              /\ phase' = [phase EXCEPT ![s] = "serving"]
              \* the guard: the room goes back now, unless the bug keeps it
              /\ leased' = [leased EXCEPT ![s] = (LeaseMode = "HoldUntilReturn")]
              \* nothing else moves
              /\ UNCHANGED <<open, revoked, down>>

\* The connection closed while the request was being read: reset, room freed.
ReadFails(s) == /\ phase[s] = "reading"
                \* the connection is gone
                /\ ~open[PeerOf[s]]
                \* the stream is reset
                /\ phase' = [phase EXCEPT ![s] = "reset"]
                \* and its room is freed
                /\ leased' = [leased EXCEPT ![s] = FALSE]
                \* nothing else moves
                /\ UNCHANGED <<open, revoked, down>>

\* A handler that does not hang finishes, and anything it held goes back.
Return(s) == /\ phase[s] = "serving"
             \* only a handler that is not stuck
             /\ s \notin Stuck
             \* the stream ends normally
             /\ phase' = [phase EXCEPT ![s] = "ended"]
             \* holding nothing
             /\ leased' = [leased EXCEPT ![s] = FALSE]
             \* nothing else moves
             /\ UNCHANGED <<open, revoked, down>>

\* The protocol's deadline passes: the handler is dropped, the stream reset.
Deadline(s) == /\ phase[s] = "serving"
               \* only when handlers have deadlines
               /\ DeadlineMode = "Deadline"
               \* the stream is reset
               /\ phase' = [phase EXCEPT ![s] = "reset"]
               \* holding nothing
               /\ leased' = [leased EXCEPT ![s] = FALSE]
               \* nothing else moves
               /\ UNCHANGED <<open, revoked, down>>

\* The connection closed: the handler is dropped, the stream reset.
Cut(s) == /\ phase[s] = "serving"
          \* its connection is closed
          /\ ~open[PeerOf[s]]
          \* the stream is reset
          /\ phase' = [phase EXCEPT ![s] = "reset"]
          \* holding nothing
          /\ leased' = [leased EXCEPT ![s] = FALSE]
          \* nothing else moves
          /\ UNCHANGED <<open, revoked, down>>

\* The node revokes peer p: it is barred, and its connection closed.
Revoke(p) == /\ ~revoked[p]
             \* the bar is recorded
             /\ revoked' = [revoked EXCEPT ![p] = TRUE]
             \* the guard: the connection closes, unless the bug only bars new ones
             /\ open' = [open EXCEPT ![p] = (RevokeMode = "BarOnly" /\ open[p])]
             \* nothing else moves
             /\ UNCHANGED <<phase, leased, down>>

\* The node shuts down: every task is aborted, then the endpoint closes.
Shutdown == /\ ~down
            \* the node is down
            /\ down' = TRUE
            \* every connection closes with the endpoint
            /\ open' = [p \in Peers |-> FALSE]
            \* the guard: tasks still reading or serving are aborted, unless the bug leaves them
            /\ phase' = [s \in Streams |->
                           IF ShutdownMode = "Abort" /\ phase[s] \in {"reading", "serving"}
                           THEN "reset" ELSE phase[s]]
            \* an aborted task frees its room
            /\ leased' = [s \in Streams |->
                            IF ShutdownMode = "Abort" THEN FALSE ELSE leased[s]]
            \* revocations stay as they were
            /\ UNCHANGED revoked

\* Every step the system can take.
Next == \/ \E s \in Streams : Unknown(s) \/ Arrive(s) \/ Busy(s) \/ Handoff(s)
                             \/ ReadFails(s) \/ Return(s) \/ Deadline(s) \/ Cut(s)
        \/ \E p \in Peers : Revoke(p)
        \/ Shutdown

\* The handler's own exits are fair: a finishing handler finishes, a
\* deadline fires, a closed connection is noticed. Revocation and shutdown
\* may never happen, so they get no fairness.
Fairness == \A s \in Streams : /\ WF_vars(Return(s))
                               \* a deadline that can fire eventually does
                               /\ WF_vars(Deadline(s))
                               \* a closed connection is eventually noticed
                               /\ WF_vars(Cut(s))
                               \* a request that is read is eventually handed off
                               /\ WF_vars(Handoff(s))
                               \* a read on a dead connection eventually fails
                               /\ WF_vars(ReadFails(s))

\* The whole behaviour.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every variable holds a value of its kind.
INV_TypeOK == /\ phase \in [Streams -> Phases]
              \* room is held or not
              /\ leased \in [Streams -> BOOLEAN]
              \* a connection is open or not
              /\ open \in [Peers -> BOOLEAN]
              \* a peer is revoked or not
              /\ revoked \in [Peers -> BOOLEAN]
              \* the node is down or not
              /\ down \in BOOLEAN

\* A stream reaches reading, a handler or a normal end only if its tag was registered.
INV_OnlyRegisteredServed == \A s \in Streams :
                              phase[s] \in {"reading", "serving", "ended"} => s \in Registered

\* No peer holds room for more requests than its share.
INV_ShareBound == \A p \in Peers : Cardinality(Held(p)) <= Share

\* A running handler holds no budget room, so a hung one locks nobody out.
INV_ServingHoldsNoLease == \A s \in Streams : phase[s] = "serving" => ~leased[s]

\* A revoked peer has no open connection, so its open streams are cut.
INV_RevokedIsCut == \A p \in Peers : revoked[p] => ~open[p]

\* Once the node is down, no stream is still being read or served.
INV_ShutdownLeavesNone == down => \A s \in Streams : phase[s] \notin {"reading", "serving"}

\* Every handler that starts eventually stops.
LIVE_HandlerEnds == \A s \in Streams : (phase[s] = "serving") ~> (phase[s] \in {"ended", "reset"})
====
