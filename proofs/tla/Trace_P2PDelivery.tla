-------------------------- MODULE Trace_P2PDelivery --------------------------
\* The story. A test starts real DefraDB nodes built with the `model-trace`
\* feature and runs a scenario on them: connect, set replicators, write.
\* Each node logs one event for each P2PDelivery action it takes: a commit,
\* a marker registration, an admission, a fetch dispatch, a merge, a crash,
\* a connection. The test collects the logs, orders the events by the time
\* they happened, names the nodes and heads, and hands them to this module
\* as the constant Trace.
\*
\* This module replays that trace through P2PDelivery. The event at index l
\* must be a step the model allows from the state the earlier events left,
\* with the node, peer and head the event names. Each event fixes its step,
\* so TLC walks one path. What can go wrong:
\*   - an event is not a step the model allows. TLC stops there with a
\*     deadlock and prints the state and l: the code took a step the model
\*     forbids (an acknowledgement for a sibling that is not covered, a merge
\*     with no fetch slot, a TTL expiry the design rules out);
\*   - a replayed state breaks one of the model's invariants;
\*   - the trace ends with a commit or forward intent the code never
\*     registered, or with a head that never reached a node it must reach.
\*     TLC reports a deadlock with l past the end.
\* A trace that replays to its end with none of these is a behaviour of the
\* model, and TLC reports no error.
\*
\* Some events are observations rather than steps, and replay as no change:
\*   - Ack: the model clears the marker when the receiver admits the head,
\*     so the sender's acknowledgement only confirms the marker is gone;
\*   - Admit with outcome "nacked": a refused push changes nothing;
\*   - a repeated Admit of a head already acknowledged, Acquire on a fetch
\*     that already runs, partial Progress of a root whose DAG is
\*     finite, and a connection event for a pair already in that state.
\* The model's dispatch takes slot and permits together (SlotBeforePermit
\* FALSE), so the code's Acquire after a Schedule that found its permits
\* free is no change either. The code logs no FloodOff: a Schedule through
\* the merge channel first replays the storm passing.
EXTENDS P2PDelivery

CONSTANT
  Trace  \* the recorded events in time order, each [ev, node, peer, head, outcome, sup, mode]

VARIABLE
  l      \* the index of the next event to replay

\* Every variable: the model's and the replay's position.
traceVars == <<vars, l>>

\* The event being replayed.
E == Trace[l]
\* The unordered pair of the acting node and the event's peer.
EvPair == {E.node, E.peer}
\* The replicator edge from the acting node to its peer (it sends).
OutEdge == <<E.node, E.peer>>
\* The replicator edge from the peer to the acting node (it receives).
InEdge == <<E.peer, E.node>>
\* The model takes no step for this event.
Stutter == UNCHANGED vars

\* --------------------------------------------------------------------------
\* One model step per kind of event.

\* The node started: everything before was a run that ended.
DoCrash == Crash(E.node)

\* The node restored its replicator list; already known if it restores first.
DoRestore ==
  IF known[E.node] THEN Stutter                                              \* restored before the loops: no change
  ELSE RestoreReplicators(E.node)                                            \* else the restore ends here

\* A connection to the peer came up, adopted on both sides or only dialed.
DoReconnect ==
  IF EvPair \notin Pairs THEN Stutter                                        \* no edge between them: not modeled
  ELSE IF conn[EvPair] = "down"                                              \* the pair was down
       THEN /\ Reconnect(EvPair)                                             \* it comes up
            /\ conn'[EvPair] = IF E.mode = "full" THEN "full" ELSE E.node    \* the way the event says
       ELSE /\ (conn[EvPair] = "full" \/ E.mode = "dialer")                  \* already up at least as well
            /\ Stutter                                                       \* so no change

\* The last connection to the peer closed.
DoDisconnect ==
  IF EvPair \notin Pairs \/ conn[EvPair] = "down" THEN Stutter               \* nothing modeled was up
  ELSE Disconnect(EvPair)                                                    \* the pair goes down

\* A local write committed the head.
DoCommit == Commit(E.node, E.head)

\* The sender's marker for the head toward the peer is durable. A hub forwards
\* a merged head before it marks it (BeforeMark): the model lays those markers
\* in the merge step, which the Merge event after them replays, so the hub's
\* Register for a head it did not write is an observation.
DoRegister ==
  /\ OutEdge \in Repl                                                       \* the node replicates to the peer
  /\ IF E.head \in outbox[OutEdge]                                          \* a commit's intent waits
     THEN Register(OutEdge, E.head)                                         \* it becomes a marker
     ELSE /\ Author[E.head] # E.node                                        \* a forward of a head it merges
          /\ ForwardMode = "BeforeMark"                                     \* laid by the merge step
          /\ Stutter                                                        \* nothing changes yet

\* The sender's recorder failed to register the head toward the peer. For a
\* hub's forward the merge has committed and the head stays unmarked: the
\* model's ForwardFail, once per attempt.
DoRegFail ==
  /\ OutEdge \in Repl                                                       \* the node replicates to the peer
  /\ IF E.head \in outbox[OutEdge]                                          \* a commit's intent waits
     THEN RegFail(OutEdge, E.head)                                          \* its registration failed
     ELSE /\ Author[E.head] # E.node                                        \* a forward of a head it merges
          /\ IF slot[E.node][E.head] = "run"                                \* the attempt that merged it
             THEN ForwardFail(E.node, E.head)                               \* ends with the head unmarked
             ELSE Stutter                                                   \* a later edge of the same forward

\* The acting node, the receiver, decided a head the peer pushed. A head
\* already acknowledged over the edge is pushed again only as a duplicate,
\* which changes nothing.
DoAdmit ==
  /\ InEdge \in Repl                                                         \* the peer replicates to this node
  /\ CASE E.outcome = "registered" ->                                        \* a new durable obligation
            /\ E.head \notin pendDur[E.node]                                 \* not recorded before
            /\ Push(InEdge, E.head)                                          \* the push is admitted
            /\ E.head \in pendDur'[E.node]                                   \* and recorded, not just acked
            /\ E.sup \subseteq Superseded(E.node, E.peer, E.head)            \* retiring only what it may retire
       [] E.outcome = "merged" ->                                            \* already merged here
            /\ E.head \in merged[E.node]                                     \* the model agrees
            /\ IF E.head \in owed[InEdge] THEN Push(InEdge, E.head) ELSE Stutter \* the ack clears the marker
       [] E.outcome = "covered" ->                                           \* a durable record brings it
            /\ Covered(E.node, E.peer, E.head)                               \* by ancestry, in the fixed design
            /\ IF E.head \in owed[InEdge] THEN Push(InEdge, E.head) ELSE Stutter \* the ack clears the marker
       [] E.outcome = "nacked" -> Stutter                                    \* refused: nothing changes
       [] OTHER -> FALSE                                                     \* no other outcome exists

\* The sender acted on an acknowledgement: its marker must already be gone.
DoAck == OutEdge \in Repl /\ E.head \notin owed[OutEdge] /\ Stutter

\* The dispatch clock gave the head a fetch slot, running or waiting for permits.
DoSchedule ==
  /\ slot[E.node][E.head] = "none"                                           \* it had no slot
  /\ Schedule(E.node)                                                        \* the clock dispatches
  /\ slot'[E.node][E.head] = IF E.mode = "run" THEN "run" ELSE "wait"        \* this head, as the event says

\* The fetch took its permits; with slot and permits together it already had them.
DoAcquire ==
  IF slot[E.node][E.head] = "wait" THEN Acquire(E.node, E.head)              \* parked on its slot: now it runs
  ELSE slot[E.node][E.head] = "run" /\ Stutter                               \* already running: no change

\* A window brought blocks without finishing the DAG.
DoProgress ==
  IF E.head \in Trickle THEN Progress(E.node, E.head)                        \* a DAG that never ends: productive
  ELSE slot[E.node][E.head] \in {"run", "prod"} /\ Stutter                   \* a finite DAG: one block, no change

\* The fetch completed and the head merged: discharge and forward intents.
DoMerge == E.head \notin Trickle /\ Progress(E.node, E.head)

\* The attempt got nothing, and kept its slot or let it go. The model's one-
\* block DAG abstracts a window that fails while its provider serves, so the
\* replay does not ask whether the provider was reachable.
DoStall ==
  /\ StallStep(E.node, E.head)                                               \* the attempt ends empty
  /\ (slot'[E.node][E.head] = "hold") <=> (E.mode = "hold")                  \* keeping its slot only if the code did

\* The quantum cut a productive attempt.
DoPreempt == Preempt(E.node, E.head)

\* The head's backoff ran out; a head given up at the front is due already.
DoBecomeDue ==
  IF E.head \in Range(back[E.node])                                          \* waiting out a backoff
  THEN Head(back[E.node]) = E.head /\ BecomeDue(E.node)                      \* it is the next one due
  ELSE E.head \in Range(queue[E.node]) /\ Stutter                            \* due already: no change

\* The resync sweep gave the head's durable record a place in memory.
DoResync == Resync(E.node) /\ Pick(E.node) = E.head

\* The pending TTL dropped an entry from memory. An acknowledged root never
\* expires (it keeps its durable record and its entry until it merges), so
\* only an entry the model does not hold may, and nothing modeled changes.
DoExpire ==
  /\ E.head \notin pendDur[E.node]                                           \* no durable record backs it
  /\ E.mode # "removed"                                                     \* and no record goes
  /\ Stutter                                                                \* nothing modeled changes

\* An advisory pull fetch from the peer took permits.
DoPullStart == PullPeer = E.peer /\ PullStart(E.node)

\* The advisory pull fetch released its permits.
DoPullEnd == PullEnd(E.node)

\* The clock found the merge channel full.
DoFloodOn == IF flood[E.node] THEN Stutter ELSE FloodOn(E.node)

\* The clock found room in the merge channel.
DoFloodOff == IF flood[E.node] THEN FloodOff(E.node) ELSE Stutter

\* A local continuation took a place in the map.
DoStartCont == StartCont(E.node)

\* A local continuation left the map.
DoEndCont == EndCont(E.node)

\* The step the current event names. An event of any other kind is not a step.
Replay ==
  CASE E.ev = "Crash" -> DoCrash
    [] E.ev = "RestoreReplicators" -> DoRestore
    [] E.ev = "Reconnect" -> DoReconnect
    [] E.ev = "Disconnect" -> DoDisconnect
    [] E.ev = "Commit" -> DoCommit
    [] E.ev = "Register" -> DoRegister
    [] E.ev = "RegFail" -> DoRegFail
    [] E.ev = "Admit" -> DoAdmit
    [] E.ev = "Ack" -> DoAck
    [] E.ev = "Schedule" -> DoSchedule
    [] E.ev = "Acquire" -> DoAcquire
    [] E.ev = "Progress" -> DoProgress
    [] E.ev = "Merge" -> DoMerge
    [] E.ev = "Stall" -> DoStall
    [] E.ev = "Preempt" -> DoPreempt
    [] E.ev = "BecomeDue" -> DoBecomeDue
    [] E.ev = "Resync" -> DoResync
    [] E.ev = "Expire" -> DoExpire
    [] E.ev = "PullStart" -> DoPullStart
    [] E.ev = "PullEnd" -> DoPullEnd
    [] E.ev = "FloodOn" -> DoFloodOn
    [] E.ev = "FloodOff" -> DoFloodOff
    [] E.ev = "StartCont" -> DoStartCont
    [] E.ev = "EndCont" -> DoEndCont
    [] OTHER -> FALSE

\* The code dispatched through a full merge channel only once the storm
\* passed; the code logs no FloodOff, so the replay takes it here.
StormPasses ==
  /\ l <= Len(Trace)                                                         \* an event is left
  /\ E.ev = "Schedule"                                                       \* it is a dispatch
  /\ DispatchPath = "MergeFifo"                                              \* through the merge channel
  /\ flood[E.node]                                                           \* which the model holds full
  /\ FloodOff(E.node)                                                        \* so the storm passed first
  /\ UNCHANGED l                                                             \* and the event is still next

\* The run settled: every intent became a marker, and every head reached
\* every node the replicator edges lead it to.
Settled ==
  /\ \A e \in Repl : outbox[e] = {}                                          \* nothing left unregistered
  /\ \A hd \in Heads \ Base : \A n \in Nodes :                               \* every written head, every node
       (hd \in merged[Author[hd]] /\ <<Author[hd], n>> \in ReplPlus) => hd \in merged[n] \* reached where it must

\* Replay the next event, let a storm pass before a dispatch, or rest once
\* the whole trace replayed and the run settled.
TraceNext ==
  \/ /\ l <= Len(Trace)                                                      \* an event is left
     /\ Replay                                                               \* the model takes its step
     /\ l' = l + 1                                                           \* and moves to the next event
  \/ StormPasses                                                             \* a storm the code saw end
  \/ /\ l > Len(Trace)                                                       \* every event replayed
     /\ Settled                                                              \* and the run settled
     /\ UNCHANGED traceVars                                                  \* so the replay rests

\* The replay starts from the model's start, at the first event.
TraceInit == Init /\ l = 1

\* The replay: start, then follow the trace.
TraceSpec == TraceInit /\ [][TraceNext]_traceVars

\* Promise: the replay never leaves the trace's index range.
INV_TraceIndex == l \in 1..(Len(Trace) + 1)
=============================================================================
