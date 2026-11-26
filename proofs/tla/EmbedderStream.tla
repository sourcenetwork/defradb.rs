---- MODULE EmbedderStream ----
\* The story. An application embedding a node registers its own stream
\* protocols. Peers open streams on the node's connections, each starting
\* with a tag. The node reads the request under the inbound budget, then
\* lends the stream to the application's handler, which runs on a task of
\* its own. The handler only borrows the stream, so nothing of it can move
\* to another task: the stream, its request and the request's room end when
\* that task does. The request's room in the budget goes with the request:
\* the handler gives it back by releasing the request. A handler may finish,
\* may release its request and hang, or may keep its request and hang.
\*
\* What can happen: a peer opens streams, some with a tag nobody registered;
\* handlers finish, release their request, hang or run out of time; the node
\* revokes a peer; the node shuts down.
\*
\* What must never happen: a stream with an unregistered tag reaches a
\* handler; a peer holds more than its share of the budget; room is held for
\* a request nobody keeps any more (so a hung handler that let its request go
\* still locks its peer out); a revoked peer keeps an open connection; a
\* handler outlives shutdown; a handler runs forever, holding its room; a
\* stream whose handler was dropped at its deadline stays open because
\* the handler's last write was blocked on an opener that does not read; or
\* an opener's finish reads as delivered after the node cancelled the
\* stream, its data never taken.
\*
\* Code this models:
\*   crates/database/pinc/src/serve.rs Node::serve_stream (Arrive, the tag
\*     check), Node::admit (Busy), serve_protocol (Handoff: the lease moves
\*     into the ProtocolRequest; Return, Deadline, and Cut when the
\*     connection closes);
\*   crates/database/pinc/src/stream_protocols.rs StreamProtocols::get (the tag
\*     check), ProtocolRequest::release (DropRequest: releasing it frees the room),
\*     StreamHandler::handle (it borrows the stream, so Return, Deadline
\*     and Cut each end the stream with its task);
\*   crates/database/pinc/src/protocol_stream.rs pump_send (BlockWrite: a
\*     write waits on the opener's window; Deadline: the send half's drop
\*     signal races the write in flight and resets the stream, code 7),
\*     finish_outcome (OpenerFinish: only STOP_HANDLED, code 8, lets a
\*     finish hold), serve_protocol (Return stops the stream with code 8;
\*     Deadline and Cut drop the receive half, code 7);
\*   crates/database/pinc/src/endpoint.rs Node::deny (Revoke: every session of
\*     the peer ends within the call);
\*   crates/database/pinc/src/command.rs Command::Shutdown (Shutdown: the
\*     endpoint closes, and every connection with it).
EXTENDS Naturals, FiniteSets

CONSTANTS
  Streams,        \* every stream any peer opens in this run
  Peers,          \* every peer that opens a stream
  PeerOf,         \* the peer that opened each stream
  Registered,     \* the streams whose tag some protocol registered
  Stuck,          \* the streams whose handler never returns
  Share,          \* how many requests one peer may hold room for at once
  TagMode,        \* "Check": unknown tags are reset; "Any": nothing is checked
  LeaseMode,      \* "WithRequest": room goes back when the handler releases its request; "UntilReturn": it is held until the handler returns
  DeadlineMode,   \* "Deadline": every handler has one; "None": a handler may run forever
  RevokeMode,     \* "Close": a revocation closes the peer's connection; "BarOnly": it only stops new ones
  ShutdownMode,   \* "Abort": shutdown aborts every stream task; "None": tasks keep running
  Unread,         \* the streams whose opener stops reading, so a long write blocks
  DropMode,       \* "Signal": a dropped send half resets the stream mid-write; "None": the write in flight runs on
  FinishMode      \* "Handled": a returned handler stops with code 8, the one stop a finish holds on; "AnyCancel": it stops with 7, the cancel's code, and a finish holds on 7

VARIABLES
  phase,          \* where each stream is: idle, reading, serving, lingering, ended or reset
  leased,         \* whether each stream holds room in the budget
  kept,           \* whether each stream's handler still keeps its request
  writing,        \* whether each stream's send half is blocked in a write the opener does not read
  open,           \* whether each peer's connection is open
  revoked,        \* whether each peer was revoked
  down,           \* whether the node has shut down
  stop,           \* why the node stopped each stream's upload: none yet, handled, cancelled or refused
  fin             \* each opener's finish: none yet, sent before any stop, ok or failed

\* All the variables, for stuttering steps.
vars == <<phase, leased, kept, writing, open, revoked, down, stop, fin>>

\* The phases a stream can be in. "lingering": its handler was dropped at
\* the deadline, but the stream is still open under a write in flight.
Phases == {"idle", "reading", "serving", "lingering", "ended", "reset"}

\* The streams of peer p holding budget room right now.
Held(p) == {s \in Streams : leased[s] /\ PeerOf[s] = p}

\* Whether peer p may take room for one more request.
Room(p) == Cardinality(Held(p)) < Share

\* Stream s is over: its task freed everything, its request is gone.
Finish(s, how) == /\ phase' = [phase EXCEPT ![s] = how]
                  \* its room is back
                  /\ leased' = [leased EXCEPT ![s] = FALSE]
                  \* and no handler keeps its request
                  /\ kept' = [kept EXCEPT ![s] = FALSE]
                  \* and no write is in flight
                  /\ writing' = [writing EXCEPT ![s] = FALSE]
                  \* the upload stops: handled on a return, cancelled otherwise
                  /\ stop' = [stop EXCEPT ![s] = IF stop[s] # "none" THEN stop[s]
                                                ELSE IF how = "ended" THEN "handled" ELSE "cancelled"]

\* Nothing has happened yet: every connection open, no room taken.
Init == /\ phase = [s \in Streams |-> "idle"]
        \* no stream holds room
        /\ leased = [s \in Streams |-> FALSE]
        \* no handler holds a request
        /\ kept = [s \in Streams |-> FALSE]
        \* no write is in flight
        /\ writing = [s \in Streams |-> FALSE]
        \* every peer is connected
        /\ open = [p \in Peers |-> TRUE]
        \* no peer is revoked
        /\ revoked = [p \in Peers |-> FALSE]
        \* the node is running
        /\ down = FALSE
        \* no stream is stopped
        /\ stop = [s \in Streams |-> "none"]
        \* no opener has finished
        /\ fin = [s \in Streams |-> "none"]

\* A stream with a tag nobody registered is reset before anything is read.
Unknown(s) == /\ phase[s] = "idle"
              \* only when the node checks tags
              /\ TagMode = "Check"
              \* the stream's tag is not a registered one
              /\ s \notin Registered
              \* it is reset with the unknown-service code
              /\ phase' = [phase EXCEPT ![s] = "reset"]
              \* the opener's upload is refused
              /\ stop' = [stop EXCEPT ![s] = "refused"]
              \* nothing else moves
              /\ UNCHANGED <<leased, kept, writing, open, revoked, down, fin>>

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
             /\ UNCHANGED <<kept, writing, open, revoked, down, stop, fin>>

\* No room frees up before the admit deadline: the stream is reset busy.
Busy(s) == /\ phase[s] = "idle"
           \* its peer's share is full
           /\ ~Room(PeerOf[s])
           \* it is reset with the busy code
           /\ phase' = [phase EXCEPT ![s] = "reset"]
           \* the opener's upload is refused
           /\ stop' = [stop EXCEPT ![s] = "refused"]
           \* nothing else moves
           /\ UNCHANGED <<leased, kept, writing, open, revoked, down, fin>>

\* The request is read and the handler starts on the stream's own task,
\* holding the request and, with it, the request's room.
Handoff(s) == /\ phase[s] = "reading"
              \* the connection is still open
              /\ open[PeerOf[s]]
              \* the handler is serving
              /\ phase' = [phase EXCEPT ![s] = "serving"]
              \* it holds the request
              /\ kept' = [kept EXCEPT ![s] = TRUE]
              \* nothing else moves; the room stays with the request
              /\ UNCHANGED <<leased, writing, open, revoked, down, stop, fin>>

\* The handler releases its request, perhaps before hanging. Any handler may.
DropRequest(s) == /\ phase[s] = "serving"
                  \* it still holds the request
                  /\ kept[s]
                  \* now it does not
                  /\ kept' = [kept EXCEPT ![s] = FALSE]
                  \* the guard: the room goes with the request, unless the bug holds it until return
                  /\ leased' = [leased EXCEPT ![s] = (LeaseMode = "UntilReturn" /\ leased[s])]
                  \* nothing else moves
                  /\ UNCHANGED <<phase, writing, open, revoked, down, stop, fin>>

\* The handler writes more than the opener's window to an opener that has
\* stopped reading: the write waits for room that never comes.
BlockWrite(s) == /\ phase[s] = "serving"
                 \* only an opener that stops reading
                 /\ s \in Unread
                 \* the write is not blocked yet
                 /\ ~writing[s]
                 \* now it is
                 /\ writing' = [writing EXCEPT ![s] = TRUE]
                 \* nothing else moves
                 /\ UNCHANGED <<phase, leased, kept, open, revoked, down, stop, fin>>

\* The connection closed while the request was being read: reset, room freed.
ReadFails(s) == /\ phase[s] = "reading"
                \* the connection is gone
                /\ ~open[PeerOf[s]]
                \* the stream is reset, holding nothing
                /\ Finish(s, "reset")
                \* nothing else moves
                /\ UNCHANGED <<open, revoked, down, fin>>

\* A handler that does not hang finishes, and anything it held goes back.
Return(s) == /\ phase[s] = "serving"
             \* only a handler that is not stuck
             /\ s \notin Stuck
             \* nor blocked in a write
             /\ ~writing[s]
             \* the stream ends normally, holding nothing
             /\ Finish(s, "ended")
             \* nothing else moves
             /\ UNCHANGED <<open, revoked, down, fin>>

\* The protocol's deadline passes: the handler is dropped, the stream reset.
\* The guard: the send half's drop signal abandons a write in flight. With
\* no signal (the bug) a blocked write runs on, and the stream lingers open
\* with no handler, its room and request already gone.
Deadline(s) == /\ phase[s] = "serving"
               \* only when handlers have deadlines
               /\ DeadlineMode = "Deadline"
               \* reset at once, or left lingering under the write in flight
               /\ IF writing[s] /\ DropMode = "None"
                    THEN /\ phase' = [phase EXCEPT ![s] = "lingering"]
                         \* the handler's request and room went with it
                         /\ leased' = [leased EXCEPT ![s] = FALSE]
                         \* it keeps no request
                         /\ kept' = [kept EXCEPT ![s] = FALSE]
                         \* the write is still in flight
                         /\ UNCHANGED writing
                         \* its receive half went with the handler: cancelled
                         /\ stop' = [stop EXCEPT ![s] = "cancelled"]
                    ELSE Finish(s, "reset")
               \* nothing else moves
               /\ UNCHANGED <<open, revoked, down, fin>>

\* The connection closed: the handler (or a lingering write) is dropped, the stream reset.
Cut(s) == /\ phase[s] \in {"serving", "lingering"}
          \* its connection is closed
          /\ ~open[PeerOf[s]]
          \* the stream is reset, holding nothing
          /\ Finish(s, "reset")
          \* nothing else moves
          /\ UNCHANGED <<open, revoked, down, fin>>

\* The node revokes peer p: it is barred, and its connection closed.
Revoke(p) == /\ ~revoked[p]
             \* the bar is recorded
             /\ revoked' = [revoked EXCEPT ![p] = TRUE]
             \* the guard: the connection closes, unless the bug only bars new ones
             /\ open' = [open EXCEPT ![p] = (RevokeMode = "BarOnly" /\ open[p])]
             \* nothing else moves
             /\ UNCHANGED <<phase, leased, kept, writing, down, stop, fin>>

\* Whether shutdown aborts stream s: it is being read or served, or a write
\* lingers on it, and the guard is on.
Aborted(s) == ShutdownMode = "Abort" /\ phase[s] \in {"reading", "serving", "lingering"}

\* The node shuts down: every task is aborted, then the endpoint closes.
Shutdown == /\ ~down
            \* the node is down
            /\ down' = TRUE
            \* every connection closes with the endpoint
            /\ open' = [p \in Peers |-> FALSE]
            \* the guard: tasks still reading or serving are aborted, unless the bug leaves them
            /\ phase' = [s \in Streams |-> IF Aborted(s) THEN "reset" ELSE phase[s]]
            \* an aborted task frees its room
            /\ leased' = [s \in Streams |-> IF Aborted(s) THEN FALSE ELSE leased[s]]
            \* and its request
            /\ kept' = [s \in Streams |-> IF Aborted(s) THEN FALSE ELSE kept[s]]
            \* and any write in flight
            /\ writing' = [s \in Streams |-> IF Aborted(s) THEN FALSE ELSE writing[s]]
            \* and stops its upload as cancelled
            /\ stop' = [s \in Streams |-> IF Aborted(s) /\ stop[s] = "none" THEN "cancelled" ELSE stop[s]]
            \* revocations and finishes stay as they were
            /\ UNCHANGED <<revoked, fin>>

\* The code a stop of each kind carries. A refusal is busy, too large,
\* stalled or unknown (2 to 5); 4 stands for them all.
StopCode(why) == CASE why = "handled" -> IF FinishMode = "Handled" THEN 8 ELSE 7
                   [] why = "cancelled" -> 7
                   [] OTHER -> 4

\* The one stop code the opener's finish takes as its data taken.
HoldCode == IF FinishMode = "Handled" THEN 8 ELSE 7

\* The opener of an upload-only stream finishes it. A finish that goes out
\* before any stop arrives is only sent; one that meets a stop holds only on
\* the code a returned handler sends.
OpenerFinish(s) == /\ phase[s] # "idle"
                   \* it finishes once
                   /\ fin[s] = "none"
                   \* the outcome the pump reports
                   /\ fin' = [fin EXCEPT ![s] =
                        IF ~open[PeerOf[s]] THEN "failed"
                        ELSE IF stop[s] = "none" THEN "sent"
                        ELSE IF StopCode(stop[s]) = HoldCode THEN "ok" ELSE "failed"]
                   \* nothing else moves
                   /\ UNCHANGED <<phase, leased, kept, writing, open, revoked, down, stop>>

\* Every step the system can take.
Next == \/ \E s \in Streams : Unknown(s) \/ Arrive(s) \/ Busy(s) \/ Handoff(s)
                             \/ DropRequest(s) \/ BlockWrite(s) \/ ReadFails(s) \/ Return(s)
                             \/ Deadline(s) \/ Cut(s) \/ OpenerFinish(s)
        \/ \E p \in Peers : Revoke(p)
        \/ Shutdown

\* The handler's own exits are fair: a finishing handler finishes, a
\* deadline fires, a closed connection is noticed. Dropping the request,
\* revocation and shutdown may never happen, so they get no fairness.
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
              \* a request is kept or not
              /\ kept \in [Streams -> BOOLEAN]
              \* a write is blocked or not
              /\ writing \in [Streams -> BOOLEAN]
              \* a connection is open or not
              /\ open \in [Peers -> BOOLEAN]
              \* a peer is revoked or not
              /\ revoked \in [Peers -> BOOLEAN]
              \* the node is down or not
              /\ down \in BOOLEAN
              \* each upload is stopped for a reason, or not yet
              /\ stop \in [Streams -> {"none", "handled", "cancelled", "refused"}]
              \* each finish has an outcome, or none yet
              /\ fin \in [Streams -> {"none", "sent", "ok", "failed"}]

\* A stream reaches reading, a handler or a normal end only if its tag was registered.
INV_OnlyRegisteredServed == \A s \in Streams :
                              phase[s] \in {"reading", "serving", "ended"} => s \in Registered

\* No peer holds room for more requests than its share.
INV_ShareBound == \A p \in Peers : Cardinality(Held(p)) <= Share

\* Room is held only for a request still being read or still kept by its
\* handler, so a hung handler that let its request go locks nobody out.
INV_RoomIsTheRequest == \A s \in Streams :
                          leased[s] => (phase[s] = "reading" \/ (phase[s] = "serving" /\ kept[s]))

\* A revoked peer has no open connection, so its open streams are cut.
INV_RevokedIsCut == \A p \in Peers : revoked[p] => ~open[p]

\* Once the node is down, no stream is still being read or served.
INV_ShutdownLeavesNone == down => \A s \in Streams : phase[s] \notin {"reading", "serving", "lingering"}

\* No stream stays open past its handler's deadline: a handler dropped there
\* resets its stream at once, even when its last write is blocked on an
\* opener that does not read.
INV_DeadlineResets == \A s \in Streams : phase[s] # "lingering"

\* A finish holds only on a stream whose handler returned: one the node
\* cancelled before the opener finished never reads as delivered.
INV_CancelFailsFinish == \A s \in Streams : fin[s] = "ok" => stop[s] = "handled"

\* Every handler that starts eventually stops, and with it any room it kept.
LIVE_HandlerEnds == \A s \in Streams : (phase[s] = "serving") ~> (phase[s] \in {"ended", "reset"})
====
