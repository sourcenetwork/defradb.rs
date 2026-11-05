----------------------------- MODULE P2PDelivery -----------------------------
\* The story. A document head is written on one node and has to reach every
\* node that replicates from that node, sometimes through a hub that merges
\* it and forwards it on. The head is owed to the next node for the whole
\* journey, and three parties hand that debt along:
\*   - the sender keeps a durable retry marker for each replicator edge until
\*     the receiver acknowledges the head;
\*   - the receiver acknowledges only once it has merged the head, or written a
\*     durable pending record for it or for a descendant whose DAG brings it.
\*     A bounded in-memory map holds the records it is working on, a small
\*     pool of fetch slots and transport permits fetches their blocks, and a
\*     resync sweep brings durable records back into the map;
\*   - a hub that merges a head forwards it, which starts the same chain on its
\*     own outgoing edges. Until the forward's markers exist, the hub's own
\*     durable record for the head is what still owes it downstream.
\*
\* The world can crash a node between two steps, drop and restore
\* connections, keep a provider away for good ("dead"), let a provider send
\* blocks for ever without finishing a DAG ("trickle"), run advisory pull
\* fetches that hold transport permits, flood the merge loop's channel, and
\* start local merge continuations that hold places in the in-memory map.
\*
\* What must never happen:
\*   - a head merged on a sender is owed by nobody: no marker, registration in
\*     flight or durable record on the sender covers it, and the receiver has
\*     neither merged it nor holds a durable record whose DAG brings it
\*     (INV_ObligationOwned; "covers" is judged by ancestry, never by priority);
\*   - a node sends a request on a connection whose other side does not serve
\*     it (INV_ReverseBorrowServed);
\*   - a head that can be delivered is never delivered (LIVE_HeadDelivered).
\*     Fairness goes to each task (the dispatch clock, the backoff timer, the
\*     resync sweep, the recorder, the retry sweep, a fetch task that already
\*     holds its slot, a request's timeout), never to a root waiting for a
\*     slot or a place, so such a root is delivered only if the dispatch
\*     order, the quantum and the rotation get it there.
\* INV_Bounded checks the model's own bookkeeping (slots, permits, the map,
\* the quota, the durable cap): every action enforces those bounds by
\* construction and no knob breaks them.
\*
\* Each mode constant is one design decision. Its GREEN value is the target
\* contract, not main: every RED value is what main (d23a8cf47) does, and every
\* GREEN value is a proposed fix, except ConnMode's, which the gents rc1 pin
\* (b50c667e9) already has. Each RED configuration flips exactly one of them
\* and fails the property named in its comment. So main breaks
\* INV_ObligationOwned (F1, F2, F3, F4, F5), LIVE_HeadDelivered (F8, F9) and
\* INV_ReverseBorrowServed (F27).
\*
\* Every variable is state the code holds, except the fault budgets
\* (crashes, regFails, discs, contMade), which bound the world:
\*   merged   the heads the database has applied (merge_handler; the h/p/ head publication)
\*   owed     peerstore retry markers (peerstore.rs register_scope_marker, complete_retry_scope)
\*   outbox   registrations still running in memory: the commit's broadcast
\*            task (txn_broadcaster.rs) and, on main, the forward that runs
\*            after mark_as_merged (handlers.rs push_to_replicators_with_creator)
\*   pendDur  durable pending records (pending_store, PendingDagStore::register)
\*   queue, back  in-memory pending entries whose next_retry_at has passed, or not yet
\*   slot     fetch slots (reserve_pending_dag_fetch) and limiter permits (DagFetchLimiter)
\*   cursor   the resync cursor (records_after_resync_cursor)
\*   known    whether startup restored the replicator list (restore_iroh_p2p_state)
\*   conn     the transport's connection per pair (connect_with_cache, handle_incoming)
\*   pull     limiter permits held by an advisory pull fetch (doc_sync.rs)
\*   flood    the merge loop's channel is full (try_reserve fails)
\*   contLive merge continuation entries in the map (continue_merge)
\*
\* The receiver rules #2548 put on main are fixed here, not knobs, and their
\* REDs live in MC_SyncOwnership_Dispatch and MC_SyncOwnership_Memory: a clock
\* dispatch makes one attempt and gives its slot back (Red_HeldSlot), its root
\* is due a backoff after the attempt ends (Red_ClaimTimeRearm), a productive
\* attempt ends at its quantum (Red_UnboundedProductive), an acknowledged root
\* never expires (Red_AckedTtl), the resync sweep resumes after the root it
\* admitted last (Red_KeyOrderResync), and a local continuation ends at its
\* first idle turn (Red_ImmortalContinuation).
\*
\* Code it models, on origin/main d23a8cf47 (P2PDelivery_DESIGN.md has the line map):
\*   crates/db/src/merge/txn_broadcaster.rs         SyncTxnBroadcaster::submit_update (Commit)
\*   crates/defra-node/src/lib.rs                   Node::shutdown (Crash: stop_transport before the drain)
\*   crates/p2p/src/sync/coordinator/broadcast.rs   admit_replicator_push (Register, RegFail)
\*   crates/p2p-adapter/src/retry.rs                run_retry_pass; crates/db/src/merge/push_docs.rs retry_doc (Push)
\*   crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner (Push: the admission)
\*   crates/p2p/src/sync/head_scope.rs              ScopeDecision::of (Covered, Superseded)
\*   crates/p2p/src/sync/coordinator/mod.rs         dispatch_due_pending_dag_fetches, DagFetchLimiter (Schedule, Acquire)
\*   crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, fetch_dag_attempt (Progress, Stall, Preempt)
\*   crates/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt (EndAttempt)
\*   crates/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt (EndAttempt),
\*       resync_persisted_pending_dags, records_after_resync_cursor (Resync),
\*       continue_merge, consume_pending_merge_continuation (StartCont, EndCont)
\*   crates/p2p/src/sync/replication/handlers.rs    handle_block_received (Progress: merge, discharge, forward)
\*   crates/p2p/src/iroh/endpoint_rpc.rs            connect_with_cache, retire_timed_out_connection (Reconnect, RetireTimedOut)
\*   crates/p2p/src/iroh/endpoint_streams.rs        handle_incoming (Reconnect: the accepting side)
\*   crates/p2p-adapter/src/iroh_peer/peer.rs       IrohPeer::assemble (Crash, RestoreReplicators)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  Nodes,            \* the nodes, as strings
  Repl,             \* replicator edges <<sender, receiver>>
  Heads,            \* the document heads (one document), as strings
  Author,           \* [Heads -> Nodes]: the node that writes each head
  Parent,           \* [Heads -> SUBSET Heads]: each head's parents in the DAG
  Prio,             \* [Heads -> Nat]: a head's priority, one more than its highest parent's
  KeyOrder,         \* a sequence of every head in CID byte order
  Base,             \* heads every node has merged at the start (a shared ancestor)
  Fresh,            \* heads written while the model runs
  Backlogs,         \* the possible durable records each node holds at start, as [Nodes -> SUBSET Heads]
  Trickle,          \* heads whose provider sends blocks for ever but never finishes the DAG
  Dead,             \* nodes that never connect to anyone
  PullPeer,         \* the provider advisory pull fetches read from
  PullAt,           \* receivers that run advisory pull fetches and local merge continuations
  Storm,            \* receivers whose merge-loop channel the catch-up traffic can fill
  Crashable,        \* nodes the world may crash
  ConnectedAtStart, \* "full" or "down": the connection state of every live pair at start
  K,                \* fetch slots per receiver
  Permits,          \* global fetch permits per receiver
  PeerPermits,      \* fetch permits per provider per receiver
  MemCap,           \* places in a receiver's in-memory pending map
  PeerQuota,        \* places one sender may hold in that map
  DurCap,           \* durable pending records per receiver
  MaxCrash,         \* crashes the world may cause
  MaxRegFail,       \* marker registrations the world may fail
  MaxDisc,          \* disconnects the world may cause
  MaxCont,          \* local merge continuations the world may start per receiver
  CoverRule,        \* "Ancestry" | "Priority": what makes a pushed head covered         (F1)
  ShutdownOrder,    \* "DrainFirst" | "StopFirst": writes drain before P2P stops         (F2)
  ForwardMode,      \* "BeforeMark" | "AfterDischarge": the hub forwards, then marks    (F3)
  StartupOrder,     \* "RestoreFirst" | "LoopFirst": replicators before the loops       (F4)
  FailedIntent,     \* "Kept" | "Dropped": a failed registration's own intent           (F5)
  FanoutMode,       \* "PerPeer" | "AbortOnFirstFail": the other peers' intents         (F5)
  SlotBeforePermit, \* BOOLEAN: a dispatch takes a slot before its permits              (F8)
  DispatchPath,     \* "Direct" | "MergeFifo": how the clock hands a fetch out          (F9)
  ConnMode          \* "Served" | "HalfRegistered": a connection dialed for a request   (F27)

\* The mode constants take only their named values.
ASSUME CoverRule \in {"Ancestry", "Priority"}
ASSUME ShutdownOrder \in {"DrainFirst", "StopFirst"}
ASSUME ForwardMode \in {"BeforeMark", "AfterDischarge"}
ASSUME StartupOrder \in {"RestoreFirst", "LoopFirst"}
ASSUME FailedIntent \in {"Kept", "Dropped"}
ASSUME FanoutMode \in {"PerPeer", "AbortOnFirstFail"}
ASSUME SlotBeforePermit \in BOOLEAN
ASSUME DispatchPath \in {"Direct", "MergeFifo"}
ASSUME ConnMode \in {"Served", "HalfRegistered"}
\* Every head is in the key order exactly once.
ASSUME Len(KeyOrder) = Cardinality(Heads) /\ {KeyOrder[i] : i \in 1..Len(KeyOrder)} = Heads
\* Every bound lets at least one thing through.
ASSUME {K, Permits, PeerPermits, MemCap, PeerQuota, DurCap} \subseteq Nat \ {0}

VARIABLES
  merged,    \* [Nodes -> SUBSET Heads]: heads merged on each node (durable, closed under parents)
  owed,      \* [Repl -> SUBSET Heads]: the sender's retry markers per edge (durable)
  outbox,    \* [Repl -> SUBSET Heads]: registrations running in memory, not yet markers (volatile)
  pendDur,   \* [Nodes -> SUBSET Heads]: the receiver's durable pending records
  queue,     \* [Nodes -> Seq(Heads)]: resident roots that are due, in the order they became due
  back,      \* [Nodes -> Seq(Heads)]: resident roots waiting out a backoff, in the order they will be due
  slot,      \* [Nodes -> [Heads -> {"none","run","prod","wait"}]]: each root's fetch slot
  cursor,    \* [Nodes -> Heads \cup {None}]: the root the resync sweep admitted last
  known,     \* [Nodes -> BOOLEAN]: the node has restored its replicator list since it started
  conn,      \* [Pairs -> {"down","full"} \cup Nodes]: each pair's connection; a node = dialed by it, half-registered
  pull,      \* [Nodes -> BOOLEAN]: an advisory pull fetch holds permits on this receiver
  flood,     \* [Nodes -> BOOLEAN]: catch-up traffic keeps this receiver's merge channel full
  contLive,  \* [Nodes -> Nat]: local merge continuations holding places in the map
  contMade,  \* [Nodes -> Nat]: local merge continuations started so far
  crashes,   \* crashes so far
  regFails,  \* failed marker registrations so far
  discs      \* disconnects so far

\* Variables grouped by who owns them, for UNCHANGED.
senderVars == <<owed, outbox>>
\* The receiver's in-memory scheduling state.
poolVars == <<queue, back, slot, cursor>>
\* What the world controls, and the fault budgets.
envVars == <<conn, pull, flood, contLive, contMade, crashes, regFails, discs>>
\* Every variable.
vars == <<merged, pendDur, known, senderVars, poolVars, envVars>>

\* --------------------------------------------------------------------------
\* Fixed facts about the shape, computed once.

\* The marker for "no root" and "no provider".
None == "none"
\* A node's outgoing replicator edges.
Out(n) == {e \in Repl : e[1] = n}
\* Nodes some edge points at: the receivers.
Receivers == {e[2] : e \in Repl}
\* Unordered pairs that share an edge: each has one connection.
Pairs == {{e[1], e[2]} : e \in Repl}
\* Grow a set of heads by n generations of parents.
RECURSIVE Closure(_, _)
Closure(S, n) == IF n = 0 THEN S ELSE Closure(S \cup UNION {Parent[y] : y \in S}, n - 1)
\* Every ancestor of each head.
AncOf == [hd \in Heads |-> Closure(Parent[hd], Cardinality(Heads))]
\* A set of heads and everything their DAGs bring along.
Reach(S) == S \cup UNION {AncOf[y] : y \in S}
\* Each head's position in CID byte order.
KeyOf == [hd \in Heads |-> CHOOSE i \in 1..Len(KeyOrder) : KeyOrder[i] = hd]
\* y sorts strictly before z by (priority, CID): the order head_scope.rs uses.
Older(y, z) == Prio[y] < Prio[z] \/ (Prio[y] = Prio[z] /\ KeyOf[y] < KeyOf[z])
\* The sender a receiver gets each head from: its author when the author replicates
\* to it directly, else the one node that replicates to it (the hub).
SrcOf == [r \in Nodes, hd \in Heads |->
            IF <<Author[hd], r>> \in Repl THEN Author[hd]                     \* the author, when it replicates to r
            ELSE IF \E s \in Nodes : <<s, r>> \in Repl                        \* else, if anyone replicates to r,
                 THEN CHOOSE s \in Nodes : <<s, r>> \in Repl                  \* the node that does
                 ELSE None]                                                   \* r receives nothing
\* Heads that can be delivered: written during the run or in a backlog, their
\* provider not dead, their DAG finite.
Deliverable == {hd \in Heads \ Base : hd \notin Trickle /\ Author[hd] \notin Dead}
\* Edges followed transitively: who a head written on a node must reach.
RECURSIVE Hops(_, _)
Hops(S, n) ==
  IF n = 0 THEN S                                                             \* done growing
  ELSE Hops(S \cup {pr \in Nodes \X Nodes : \E e \in S, f \in Repl : e[2] = f[1] /\ pr = <<e[1], f[2]>>}, n - 1) \* add one more hop
\* Every <<writer, node>> a head must travel.
ReplPlus == Hops(Repl, Cardinality(Nodes))

\* --------------------------------------------------------------------------
\* What the receiver's memory holds, derived from the variables.

\* The elements of a sequence.
Range(s) == {s[i] : i \in DOMAIN s}
\* A sequence without the heads in S.
Without(s, S) == SelectSeq(s, LAMBDA y : y \notin S)
\* Roots holding a fetch slot (running, or waiting for a permit).
Slotted(r) == {hd \in Heads : slot[r][hd] # "none"}
\* Roots whose fetch runs and holds its permits.
Running(r) == {hd \in Heads : slot[r][hd] \in {"run", "prod"}}
\* Roots in the in-memory pending map.
Resident(r) == Range(queue[r]) \cup Range(back[r]) \cup Slotted(r)
\* Places taken in the map, local continuations included.
MemUsed(r) == Cardinality(Resident(r)) + contLive[r]
\* Places sender s holds in the map.
Quota(r, s) == Cardinality({hd \in Resident(r) : SrcOf[r, hd] = s})
\* Durable records with no place in the map.
DurOnly(r) == pendDur[r] \ Resident(r)
\* Permits held: one global and one for the provider, per running fetch and per pull fetch.
GlobalUsed(r) == Cardinality(Running(r)) + (IF pull[r] THEN 1 ELSE 0)
\* Permits held for provider p.
PeerUsed(r, p) == Cardinality({hd \in Running(r) : SrcOf[r, hd] = p}) + (IF pull[r] /\ p = PullPeer THEN 1 ELSE 0)
\* A fetch from p could take its permits now.
PermitFree(r, p) == GlobalUsed(r) < Permits /\ PeerUsed(r, p) < PeerPermits
\* Root hd holds a contended place: a durable record is waiting for a place,
\* and the map is full or hd's sender is at its quota (finish_attempt).
Contended(r, hd) ==
  /\ DurOnly(r) # {}                                                          \* a durable record waits
  /\ MemUsed(r) >= MemCap \/ Quota(r, SrcOf[r, hd]) >= PeerQuota              \* the map or the quota is full
\* Sets each root in S back to no slot.
FreeSlots(r, S) == [hd \in Heads |-> IF hd \in S THEN "none" ELSE slot[r][hd]]

\* Takes the roots in S out of r's memory: off the queues, out of their slots.
Unload(r, S) ==
  /\ queue' = [queue EXCEPT ![r] = Without(@, S)]       \* no longer due
  /\ back' = [back EXCEPT ![r] = Without(@, S)]         \* no longer in backoff
  /\ slot' = [slot EXCEPT ![r] = FreeSlots(r, S)]       \* slots and permits released

\* --------------------------------------------------------------------------
\* Connections.

\* The connection between x and y.
Link(x, y) == conn[{x, y}]
\* r can fetch blocks from p: p is not dead, and the connection is fully
\* registered or r dialed it itself (a half-registered connection serves only
\* the requests of the side that dialed it).
FetchOk(r, p) == p \notin Dead /\ (Link(r, p) = "full" \/ Link(r, p) = r)
\* s can push to r, under the same rule.
PushOk(s, r) == Link(s, r) = "full" \/ Link(s, r) = s
\* r can fetch root hd now: the provider is reachable and holds its whole DAG.
Fetchable(r, hd) == FetchOk(r, SrcOf[r, hd]) /\ Reach({hd}) \subseteq merged[SrcOf[r, hd]]
\* The node of pair p that accepted its half-registered connection.
Acceptor(p) == CHOOSE o \in p : o # conn[p]
\* The acceptor of a half-registered connection has a request for its dialer
\* on it: it caches the accepted connection for its own requests, so a
\* marker it owes the dialer or a fetch from the dialer goes out on it.
Borrowed(p) ==
  /\ conn[p] \in p                                                            \* half-registered
  /\ LET o == Acceptor(p)                                                     \* the side that did not dial
         d == conn[p]                                                         \* the dialer
     IN \/ <<o, d>> \in Repl /\ owed[<<o, d>>] # {}                           \* a push to the dialer
        \/ \E hd \in Running(o) : SrcOf[o, hd] = d                            \* or a fetch from it

\* --------------------------------------------------------------------------
\* Admission: what the receiver does with a pushed head (head_scope.rs).

\* The durable records r holds from sender s: one sender scope (one document).
Scope(r, s) == {y \in pendDur[r] : SrcOf[r, y] = s}
\* hd is covered by a record of its scope. By ancestry: the record's DAG brings
\* hd. By priority (main): some record sorts at or after hd, sibling or not.
Covered(r, s, hd) ==
  \E cur \in Scope(r, s) :                                                    \* some record of the scope
    IF CoverRule = "Ancestry" THEN hd \in Reach({cur}) ELSE ~Older(cur, hd)   \* brings hd, or sorts at or after it
\* The records hd retires. By ancestry: its own ancestors, whose blocks its DAG
\* brings. By priority (main): every record that sorts before it.
Superseded(r, s, hd) ==
  {cur \in Scope(r, s) : IF CoverRule = "Ancestry" THEN cur \in AncOf[hd] ELSE Older(cur, hd)} \* ancestors, or everything older

\* --------------------------------------------------------------------------
\* Sender actions.

\* Node n writes head hd on top of exactly its parents. The commit leaves one
\* registration per outgoing edge running in memory (the broadcast task the
\* commit waits on); nothing durable backs it until the recorder writes the
\* marker. The node serves no write until startup has restored its
\* replicators: assemble awaits setup_iroh, which restores them, before it
\* returns the node.
Commit(n, hd) ==
  /\ hd \in Fresh                                                  \* written during the run
  /\ Author[hd] = n                                                \* by its author
  /\ known[n]                                                      \* once the node is up
  /\ merged[n] = Reach(Parent[hd])                                 \* on exactly its parents
  /\ merged' = [merged EXCEPT ![n] = @ \cup {hd}]                  \* the write commits
  /\ outbox' = [e \in Repl |-> IF e \in Out(n)                     \* one intent per edge
                                THEN outbox[e] \cup {hd} ELSE outbox[e]]      \* other edges unchanged
  /\ UNCHANGED <<pendDur, known, owed, poolVars, envVars>>          \* nothing else moves

\* The recorder turns the intent for edge e into a durable retry marker.
Register(e, hd) ==
  /\ hd \in outbox[e]                                     \* an intent is waiting
  /\ outbox' = [outbox EXCEPT ![e] = @ \ {hd}]            \* it is consumed
  /\ owed' = [owed EXCEPT ![e] = @ \cup {hd}]             \* the marker exists
  /\ UNCHANGED <<merged, pendDur, known, poolVars, envVars>>  \* nothing else moves

\* The recorder fails to register the marker for edge e (a storage error or a
\* replicator row changing under it). Its own intent is kept to be tried
\* again, or dropped (main: the error is logged and nobody retries). Per peer,
\* the other edges' intents are untouched; abort on first fail (main): the
\* fan-out returns its error and the intents of the other edges not registered
\* yet are dropped (F5).
RegFail(e, hd) ==
  /\ hd \in outbox[e]                                           \* an intent is waiting
  /\ regFails < MaxRegFail                                      \* the world still may fail one
  /\ regFails' = regFails + 1                                   \* counted
  /\ outbox' = [f \in Repl |->                                  \* for each edge
                  IF \/ f = e /\ FailedIntent = "Dropped"        \* the failed one, if dropped
                     \/ /\ f \in Out(e[1]) /\ f # e              \* or a sibling edge of the fan-out,
                        /\ FanoutMode = "AbortOnFirstFail"       \* if the fan-out aborts
                  THEN outbox[f] \ {hd}                          \* loses its intent
                  ELSE outbox[f]]                                \* every other edge keeps its own
  /\ UNCHANGED <<merged, pendDur, known, owed, poolVars, conn, pull, flood, contLive, contMade, crashes, discs>> \* nothing else moves

\* The sender pushes head hd over edge e (a live push or the retry sweep) and
\* the receiver admits it, in the order of process_block_inner. Merged or
\* covered: acknowledged, nothing written. Otherwise, if the map, the sender's
\* quota and the durable cap all have room once the records it retires are
\* gone: those records are dropped, a durable record for hd is written and hd
\* is due in memory, then acknowledged. A push the caps refuse is nacked; it
\* changes nothing, so it is not a step here. An acknowledgement clears the
\* marker.
Push(e, hd) ==
  LET s == e[1]                                                   \* the sender
      r == e[2]                                                   \* the receiver
      sup == Superseded(r, s, hd)                                 \* records hd retires
      keep == Resident(r) \ sup                                   \* memory left after that
  IN
  /\ hd \in owed[e]                                               \* a marker drives the push
  /\ PushOk(s, r)                                                 \* the connection carries it
  /\ \/ /\ hd \in merged[r] \/ Covered(r, s, hd)                  \* merged or covered: ack only
        /\ UNCHANGED <<pendDur, poolVars>>                        \* nothing written
     \/ /\ hd \notin merged[r]                                    \* a new obligation
        /\ ~Covered(r, s, hd)                                     \* that nothing covers
        /\ Cardinality(keep) + contLive[r] < MemCap               \* the map has room
        /\ Cardinality({y \in keep : SrcOf[r, y] = s}) < PeerQuota \* the sender's quota has room
        /\ Cardinality(pendDur[r] \ sup) < DurCap                 \* the durable cap has room
        /\ pendDur' = [pendDur EXCEPT ![r] = (@ \ sup) \cup {hd}] \* retire, then record durably
        /\ queue' = [queue EXCEPT ![r] = Append(Without(@, sup), hd)] \* due now, behind the rest
        /\ back' = [back EXCEPT ![r] = Without(@, sup)]           \* retired roots leave backoff
        /\ slot' = [slot EXCEPT ![r] = FreeSlots(r, sup)]         \* and their slots
        /\ UNCHANGED cursor                                       \* the sweep is untouched
  /\ owed' = [owed EXCEPT ![e] = @ \ {hd}]                        \* the ack clears the marker
  /\ UNCHANGED <<merged, outbox, known, envVars>>                 \* nothing else moves

\* --------------------------------------------------------------------------
\* Receiver actions: the dispatch clock, the fetch pool, the merge.

\* The clock hands out one fetch when a slot is free (and, through the merge
\* FIFO, only when the channel has room). Target: the first due root whose
\* provider has a free permit takes slot and permits together. Slot before
\* permit (main): the first due root takes the slot and waits there for a
\* permit (F8).
Schedule(r) ==
  LET ready == SelectSeq(queue[r], LAMBDA y : PermitFree(r, SrcOf[r, y]))  \* due roots that could run now
  IN
  /\ Cardinality(Slotted(r)) < K                                   \* a slot is free
  /\ DispatchPath = "Direct" \/ ~flood[r]                          \* the FIFO has room, if used (F9)
  /\ IF SlotBeforePermit                                                      \* which order: slot first (main) or permits first (target)
       THEN /\ queue[r] # <<>>                                     \* some root is due
            /\ slot' = [slot EXCEPT ![r][Head(queue[r])] =          \* the first takes the slot
                          IF PermitFree(r, SrcOf[r, Head(queue[r])]) THEN "run" ELSE "wait"] \* and runs, or parks for a permit
            /\ queue' = [queue EXCEPT ![r] = Tail(@)]               \* and leaves the queue
       ELSE /\ ready # <<>>                                        \* some due root can run
            /\ slot' = [slot EXCEPT ![r][Head(ready)] = "run"]      \* it takes slot and permits
            /\ queue' = [queue EXCEPT ![r] = Without(@, {Head(ready)})] \* and leaves the queue
  /\ UNCHANGED <<merged, pendDur, known, senderVars, back, cursor, envVars>> \* nothing else moves

\* A dispatch parked on its slot gets its permits (only with slot before permit).
Acquire(r, hd) ==
  /\ slot[r][hd] = "wait"                                  \* parked on a slot
  /\ PermitFree(r, SrcOf[r, hd])                           \* the permits are free
  /\ slot' = [slot EXCEPT ![r][hd] = "run"]                \* it runs
  /\ UNCHANGED <<merged, pendDur, known, senderVars, queue, back, cursor, envVars>> \* nothing else moves

\* The one attempt of a clock dispatch for hd ends without merging and gives
\* its slot back (finish_clock_attempt). If hd holds a contended place it
\* gives that place up and keeps its durable record (rotation). Otherwise it
\* is due again: at once, behind every root already due, when the quantum cut
\* it ("due"), or after a backoff from the attempt's end ("back").
EndAttempt(r, hd, where) ==
  /\ slot' = [slot EXCEPT ![r][hd] = "none"]                      \* slot and permits released
  /\ IF Contended(r, hd) /\ hd \in pendDur[r]                                 \* a durable record waits for its place
       THEN UNCHANGED <<queue, back>>                             \* gives its place up
       ELSE IF where = "due"                                                  \* else, when is it due
              THEN /\ queue' = [queue EXCEPT ![r] = Append(@, hd)] \* now, behind the due roots
                   /\ UNCHANGED back                                          \* backoff untouched
              ELSE /\ back' = [back EXCEPT ![r] = Append(@, hd)]   \* after a backoff
                   /\ UNCHANGED queue                                         \* due list untouched

\* The provider sends blocks of hd. A trickle root's attempt becomes
\* productive and never ends by itself. Any other root's DAG completes and
\* the merge handler runs: the merge commits the DAG (or finds it applied
\* already, on a re-drive), and the forward goes to every outgoing edge the
\* node knows. Forward before the mark (target, #2556): the forward's
\* markers are registered, then mark_as_merged discharges the durable
\* records the DAG covers. After the discharge (main): the records are
\* discharged first and the forward is left running in memory (F3). A node
\* that has not restored its replicators forwards to nobody (F4).
Progress(r, hd) ==
  LET done == Reach({hd})                                          \* the DAG the fetch brings
      fwd == {e \in Out(r) : known[r]}                             \* the edges the forward reaches
  IN
  /\ slot[r][hd] = "run"                                           \* the attempt runs
  /\ Fetchable(r, hd)                                              \* the provider serves it
  /\ IF hd \in Trickle                                                        \* a DAG that never ends
       THEN /\ slot' = [slot EXCEPT ![r][hd] = "prod"]             \* productive, not done
            /\ UNCHANGED <<merged, pendDur, owed, outbox, queue, back>>       \* nothing merged
       ELSE /\ merged' = [merged EXCEPT ![r] = @ \cup done]        \* merged
            /\ pendDur' = [pendDur EXCEPT ![r] = @ \ done]         \* records discharged by the mark
            /\ Unload(r, done)                                     \* merged roots leave memory
            /\ IF ForwardMode = "BeforeMark"                                  \* forward, then mark
                 THEN /\ owed' = [e \in Repl |-> IF e \in fwd THEN owed[e] \cup {hd} ELSE owed[e]] \* markers first
                      /\ UNCHANGED outbox                                     \* nothing left running
                 ELSE /\ outbox' = [e \in Repl |-> IF e \in fwd THEN outbox[e] \cup {hd} ELSE outbox[e]] \* forward still running
                      /\ UNCHANGED owed                                       \* no marker yet
  /\ UNCHANGED <<known, cursor, envVars>>                                     \* nothing else moves

\* Forward before the mark only: the merge commits but the forward fails (a
\* recorder or storage error, a registration P2P's shutdown refuses, or a
\* crash between the two). The handler returns MergedButBroadcastFailed and
\* leaves the root unmarked, so its durable records stay and the clock
\* re-drives it after a backoff; the re-drive finds the DAG applied and
\* forwards (forward_owed). It counts as a failed registration.
ForwardFail(r, hd) ==
  /\ ForwardMode = "BeforeMark"                                    \* the mark waits for the forward
  /\ slot[r][hd] = "run"                                           \* the attempt runs
  /\ Fetchable(r, hd)                                              \* the provider serves it
  /\ hd \notin Trickle                                             \* the DAG completes
  /\ known[r] /\ Out(r) # {}                                       \* there is a forward to fail
  /\ regFails < MaxRegFail                                         \* the world still may fail one
  /\ regFails' = regFails + 1                                      \* counted
  /\ merged' = [merged EXCEPT ![r] = @ \cup Reach({hd})]           \* the merge commits
  /\ EndAttempt(r, hd, "back")                                     \* slot back, re-driven later
  /\ UNCHANGED <<pendDur, known, senderVars, cursor, conn, pull, flood, contLive, contMade, crashes, discs>> \* records stay

\* The provider cannot serve hd (gone, dead, or a connection it does not
\* answer on): the attempt ends and hd is due after a backoff. A dispatch
\* parked for its permits whose provider is not connected ends the same way,
\* without the permits: poll_fetch_dag defers before it awaits the limiter.
Stall(r, hd) ==
  /\ \/ slot[r][hd] \in {"run", "prod"} /\ ~Fetchable(r, hd)       \* the attempt gets nothing
     \/ slot[r][hd] = "wait" /\ ~FetchOk(r, SrcOf[r, hd])          \* or no provider is connected
  /\ EndAttempt(r, hd, "back")                                     \* slot back, due later
  /\ UNCHANGED <<merged, pendDur, known, senderVars, cursor, envVars>>        \* nothing else moves

\* The quantum cuts a productive attempt; the blocks it brought stay and hd is
\* due again at once.
Preempt(r, hd) ==
  /\ slot[r][hd] = "prod"                                  \* this attempt has been productive
  /\ EndAttempt(r, hd, "due")                              \* slot back, due behind the due roots
  /\ UNCHANGED <<merged, pendDur, known, senderVars, cursor, envVars>>        \* nothing else moves

\* The oldest backoff runs out: that root is due, behind the roots due already.
BecomeDue(r) ==
  /\ back[r] # <<>>                                                 \* someone waits out a backoff
  /\ queue' = [queue EXCEPT ![r] = Append(@, Head(back[r]))]        \* it is due last
  /\ back' = [back EXCEPT ![r] = Tail(@)]                           \* and leaves backoff
  /\ UNCHANGED <<merged, pendDur, known, senderVars, slot, cursor, envVars>> \* nothing else moves

\* Durable records with no place in memory that would fit there now.
Candidates(r) ==
  {hd \in DurOnly(r) : MemUsed(r) < MemCap /\ Quota(r, SrcOf[r, hd]) < PeerQuota} \* room in the map and in the quota
\* The record the sweep admits next: the first after the one admitted last,
\* in key order, wrapping round (records_after_resync_cursor).
Pick(r) ==
  LET later == {hd \in Candidates(r) : cursor[r] = None \/ KeyOf[hd] > KeyOf[cursor[r]]} \* after the cursor
      pool == IF later # {} THEN later ELSE Candidates(r)                               \* or from the start
  IN CHOOSE hd \in pool : \A y \in pool : KeyOf[hd] <= KeyOf[y]                         \* lowest key

\* The resync sweep gives a durable record a place in memory, due now, and
\* remembers it as where the next sweep resumes.
Resync(r) ==
  /\ Candidates(r) # {}                                              \* a record fits
  /\ queue' = [queue EXCEPT ![r] = Append(@, Pick(r))]               \* it is due, last
  /\ cursor' = [cursor EXCEPT ![r] = Pick(r)]                        \* the sweep remembers it
  /\ UNCHANGED <<merged, pendDur, known, senderVars, back, slot, envVars>> \* nothing else moves

\* --------------------------------------------------------------------------
\* The world.

\* Node n stops and starts again. Memory, running registrations, connections
\* and the node's own pull fetches and continuations are gone; merged heads,
\* markers and durable records stay. Drain first (target, #2559): the stop
\* lets every write in flight finish, its marker registration included,
\* before P2P stops, so it never lands between a commit and its markers.
\* Stop first (main): P2P stops before the drain and a write can commit with
\* nobody left to register its markers (F2). A kill inside that window is
\* outside the contract either way. Replicators restored before the loops
\* start (target) are known at once; loops first (main) run with none until
\* the restore (F4).
Crash(n) ==
  /\ n \in Crashable                                                     \* may crash
  /\ crashes < MaxCrash                                                  \* within the budget
  /\ ShutdownOrder = "DrainFirst" =>                                     \* draining first,
       \A e \in Out(n) : \A hd \in outbox[e] : Author[hd] # n                 \* no own write is still registering
  /\ crashes' = crashes + 1                                              \* counted
  /\ queue' = [queue EXCEPT ![n] = <<>>]                                 \* memory gone
  /\ back' = [back EXCEPT ![n] = <<>>]                                        \* no backoff
  /\ slot' = [slot EXCEPT ![n] = [hd \in Heads |-> "none"]]                   \* no fetch slot held
  /\ cursor' = [cursor EXCEPT ![n] = None]                                    \* the sweep starts at the top
  /\ outbox' = [e \in Repl |-> IF e \in Out(n) THEN {} ELSE outbox[e]]   \* running registrations gone
  /\ known' = [known EXCEPT ![n] = StartupOrder = "RestoreFirst"]        \* replicators known?
  /\ conn' = [p \in Pairs |-> IF n \in p THEN "down" ELSE conn[p]]       \* connections dropped
  /\ pull' = [pull EXCEPT ![n] = FALSE]                                  \* pull fetches gone
  /\ flood' = [flood EXCEPT ![n] = FALSE]                                \* channel empty
  /\ contLive' = [contLive EXCEPT ![n] = 0]                              \* continuations gone
  /\ UNCHANGED <<merged, pendDur, owed, contMade, regFails, discs>>           \* durable state stays

\* A started node finishes restoring its replicator list.
RestoreReplicators(n) ==
  /\ ~known[n]                                                        \* not restored yet
  /\ known' = [known EXCEPT ![n] = TRUE]                              \* restored
  /\ UNCHANGED <<merged, pendDur, senderVars, poolVars, envVars>>             \* nothing else moves

\* A connection drops.
Disconnect(p) ==
  /\ conn[p] # "down"                                                 \* it was up
  /\ discs < MaxDisc                                                  \* within the budget
  /\ conn' = [conn EXCEPT ![p] = "down"]                              \* it is down
  /\ discs' = discs + 1                                               \* counted
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, pull, flood, contLive, contMade, crashes, regFails>> \* nothing else moves

\* A pair without a dead node connects again. Served (the pin): every new
\* connection is adopted and fully registered on both sides. Half-registered
\* (main): a connection one side dialed for its own request is served only to
\* that side, and the side that accepted it caches it for its own requests.
Reconnect(p) ==
  /\ conn[p] = "down"                                                 \* it is down
  /\ p \cap Dead = {}                                                 \* both ends are alive
  /\ conn' \in {[conn EXCEPT ![p] = c] :                              \* it comes back as
                  c \in IF ConnMode = "Served" THEN {"full"} ELSE {"full"} \cup p} \* full, or dialed by one end
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, pull, flood, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* The accepting side's request on a half-registered connection gets no
\* answer, and after REQUEST_RESPONSE_TIMEOUT (30 s) it retires the
\* connection: evicted and closed, on both sides. The next request dials again.
RetireTimedOut(p) ==
  /\ Borrowed(p)                                                      \* a request nobody answers
  /\ conn' = [conn EXCEPT ![p] = "down"]                              \* the connection is closed
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, pull, flood, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* An advisory pull fetch from PullPeer starts and takes a global and a
\* per-provider permit, with no slot and no durable obligation. The limiter
\* is first come, first served, so it cannot pass a dispatch already parked
\* on a slot for the same provider.
PullStart(r) ==
  /\ r \in PullAt                                                     \* pulls run here
  /\ ~pull[r]                                                         \* none running
  /\ PermitFree(r, PullPeer)                                          \* its permits are free
  /\ \A hd \in Heads : slot[r][hd] = "wait" => SrcOf[r, hd] # PullPeer \* nobody queued ahead
  /\ pull' = [pull EXCEPT ![r] = TRUE]                                \* it runs
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, flood, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* The pull fetch ends. A productive one need never end.
PullEnd(r) ==
  /\ pull[r]                                                          \* one runs
  /\ pull' = [pull EXCEPT ![r] = FALSE]                               \* it ends
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, flood, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* Catch-up traffic fills the merge loop's channel, with blocked senders waiting.
FloodOn(r) ==
  /\ r \in Storm                                                      \* storms happen here
  /\ ~flood[r]                                                        \* not full yet
  /\ flood' = [flood EXCEPT ![r] = TRUE]                              \* full
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, pull, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* The storm passes. It need never pass.
FloodOff(r) ==
  /\ flood[r]                                                         \* full
  /\ flood' = [flood EXCEPT ![r] = FALSE]                             \* drained
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, pull, contLive, contMade, crashes, regFails, discs>> \* nothing else moves

\* A local pull-sync merge yields its work budget and leaves a continuation
\* entry in the map. It has no durable record and no sender waits on it.
StartCont(r) ==
  /\ r \in PullAt                                                     \* pulls run here
  /\ contMade[r] < MaxCont                                            \* within the budget
  /\ MemUsed(r) < MemCap                                              \* the map has room
  /\ contLive' = [contLive EXCEPT ![r] = @ + 1]                       \* it takes a place
  /\ contMade' = [contMade EXCEPT ![r] = @ + 1]                       \* counted
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, pull, flood, crashes, regFails, discs>> \* nothing else moves

\* The continuation's next merge turn does not yield, so its entry leaves the
\* map (MergeTurn).
EndCont(r) ==
  /\ contLive[r] > 0                                                  \* one is live
  /\ contLive' = [contLive EXCEPT ![r] = @ - 1]                       \* it leaves the map
  /\ UNCHANGED <<merged, pendDur, known, senderVars, poolVars, conn, pull, flood, contMade, crashes, regFails, discs>> \* nothing else moves

\* --------------------------------------------------------------------------
\* The specification.

\* The run starts after a restart: each node holds one of the possible durable
\* backlogs, its author has merged every backlog head, its senders hold no
\* marker for it any more (it was acknowledged), and memory is empty.
Init ==
  /\ \E bl \in Backlogs :                                                  \* the world picks a backlog
       /\ pendDur = bl                                                     \* durable records
       /\ merged = [n \in Nodes |->                                        \* authors merged them,
                      Base \cup                                            \* everyone the base
                      Reach({hd \in UNION {bl[m] : m \in Nodes} : Author[hd] = n})] \* and their own backlog heads
  /\ owed = [e \in Repl |-> {}]                                            \* no markers
  /\ outbox = [e \in Repl |-> {}]                                          \* no intents
  /\ queue = [n \in Nodes |-> <<>>]                                        \* nothing due yet
  /\ back = [n \in Nodes |-> <<>>]                                         \* nothing in backoff
  /\ slot = [n \in Nodes |-> [hd \in Heads |-> "none"]]                    \* every slot free
  /\ cursor = [n \in Nodes |-> None]                                       \* the sweep starts at the top
  /\ known = [n \in Nodes |-> TRUE]                                        \* replicators known
  /\ conn = [p \in Pairs |-> IF p \cap Dead # {} THEN "down" ELSE ConnectedAtStart] \* dead pairs never connect
  /\ pull = [n \in Nodes |-> FALSE]                                        \* no pull fetch
  /\ flood = [n \in Nodes |-> FALSE]                                       \* no storm
  /\ contLive = [n \in Nodes |-> 0]                                        \* no continuation
  /\ contMade = [n \in Nodes |-> 0]                                           \* none started yet
  /\ crashes = 0                                                           \* no fault yet
  /\ regFails = 0                                                             \* no failed registration
  /\ discs = 0                                                                \* no disconnect

\* Every step anyone can take.
Next ==
  \/ \E n \in Nodes, hd \in Heads : Commit(n, hd)                             \* a write
  \/ \E e \in Repl, hd \in Heads : Register(e, hd) \/ RegFail(e, hd) \/ Push(e, hd) \* the sender's recorder and pushes
  \/ \E r \in Receivers :                                                     \* a receiver's
       \/ Schedule(r) \/ BecomeDue(r) \/ Resync(r)                            \* clock and sweep
       \/ \E hd \in Heads :                                                   \* fetch tasks
            \/ Acquire(r, hd) \/ Progress(r, hd) \/ Stall(r, hd) \/ Preempt(r, hd) \* permit, blocks, watchdog, quantum
            \/ ForwardFail(r, hd)                                             \* a forward the world fails
       \/ PullStart(r) \/ PullEnd(r) \/ FloodOn(r) \/ FloodOff(r)             \* pull fetches and storms
       \/ StartCont(r) \/ EndCont(r)                                          \* local continuations
  \/ \E n \in Nodes : Crash(n) \/ RestoreReplicators(n)                       \* crashes and startup
  \/ \E p \in Pairs : Disconnect(p) \/ Reconnect(p) \/ RetireTimedOut(p)      \* connections

\* The fetch task a dispatch spawned for hd: a permit is granted, blocks
\* arrive, the watchdog fires, the quantum runs out. Only a root that already
\* holds a slot has a task.
FetchTask(r, hd) ==
  Acquire(r, hd) \/ Progress(r, hd) \/ Stall(r, hd) \/ Preempt(r, hd)        \* permit, blocks, watchdog, quantum
\* The merge loop gives a local continuation its next turn.
MergeLoopStep(r) == EndCont(r)

\* A sender's loop takes one step: the recorder registers an intent, or
\* startup restores the replicator list.
SenderStep(n) ==
  \/ \E e \in Out(n), hd \in Heads : Register(e, hd)                          \* an intent becomes a marker
  \/ RestoreReplicators(n)                                                    \* or startup ends

\* Fairness for each task only. No root, scope or job is promised a dispatch
\* of its own: the clock, the sweep, the recorder and the retry sweep each get
\* one condition, so a root waiting for a slot or a place gets one only if the
\* dispatch order, the quantum and the rotation give it one. The backoff timer
\* is its own condition: a backoff ends when its deadline passes, whatever the
\* clock dispatches meanwhile, and it ends the oldest backoff, not a chosen
\* root. A fetch task that already holds its slot runs like any spawned task,
\* and its provider, if it serves, sends blocks. A request's timeout fires.
\* Writes, crashes, disconnects, failed registrations and forwards, pull
\* fetches, storms and continuations are the world's and are not fair.
\* Reconnect is fair for every pair without a dead node: eventual
\* connectivity, assumed.
Fairness ==
  /\ \A n \in Nodes : WF_vars(SenderStep(n))                     \* the recorder drains, startup ends
  /\ \A e \in Repl : WF_vars(\E hd \in Heads : Push(e, hd))       \* the retry sweep runs
  /\ \A r \in Receivers :                                                     \* for each receiver,
       /\ WF_vars(Schedule(r))                                   \* the dispatch clock runs
       /\ WF_vars(BecomeDue(r))                                  \* backoffs run out
       /\ WF_vars(Resync(r))                                     \* the resync sweep runs
       /\ WF_vars(MergeLoopStep(r))                              \* the merge loop runs
       /\ \A hd \in Heads : WF_vars(FetchTask(r, hd))            \* a spawned fetch task runs
  /\ \A p \in Pairs : WF_vars(Reconnect(p))                      \* eventual connectivity
  /\ \A p \in Pairs : WF_vars(RetireTimedOut(p))                 \* an unanswered request times out

\* The system: start, take steps, with fairness.
Spec == Init /\ [][Next]_vars /\ Fairness

\* --------------------------------------------------------------------------
\* Properties.

\* Every variable has the shape its comment says.
INV_TypeOK ==
  /\ merged \in [Nodes -> SUBSET Heads]                                       \* merged heads
  /\ owed \in [Repl -> SUBSET Heads]                                          \* markers
  /\ outbox \in [Repl -> SUBSET Heads]                                        \* intents
  /\ pendDur \in [Nodes -> SUBSET Heads]                                      \* durable records
  /\ \A n \in Nodes : Range(queue[n]) \subseteq Heads /\ Range(back[n]) \subseteq Heads \* the two queues
  /\ slot \in [Nodes -> [Heads -> {"none", "run", "prod", "wait"}]]           \* slots
  /\ cursor \in [Nodes -> Heads \cup {None}]                                  \* the sweep's cursor
  /\ known \in [Nodes -> BOOLEAN]                                             \* replicators restored
  /\ conn \in [Pairs -> {"down", "full"} \cup Nodes]                          \* connections
  /\ crashes \in 0..MaxCrash /\ regFails \in 0..MaxRegFail /\ discs \in 0..MaxDisc \* fault budgets

\* Some party still owes head hd over edge e: a marker or a running
\* registration on the sender whose DAG brings hd, or a durable record on the
\* sender whose DAG brings it (a hub forwards a root when it merges it, and
\* with the forward before the mark, while the record stands), or the
\* receiver has merged it or holds a durable record whose DAG brings it.
\* Ancestry decides, both sides.
Owned(e, hd) ==
  \/ hd \in Reach(owed[e] \cup outbox[e] \cup pendDur[e[1]])                  \* the sender still owes it
  \/ hd \in Reach(merged[e[2]] \cup pendDur[e[2]])                            \* or the receiver owns it

\* Promise: every head merged on a sender is owed to each of its receivers
\* until that receiver has it. An acknowledgement may clear a marker only once
\* the receiver owns the head by ancestry.
INV_ObligationOwned ==
  \A e \in Repl : \A hd \in merged[e[1]] : Owned(e, hd)                       \* every edge, every head its sender merged

\* Promise: no node has a request for its peer on a connection that peer does
\* not serve.
INV_ReverseBorrowServed ==
  \A p \in Pairs : ~Borrowed(p)                                               \* no unanswerable request

\* Sanity check of the model's bookkeeping: every bound holds, and every root
\* in memory has a durable record.
INV_Bounded ==
  \A r \in Nodes :                                                            \* on every node
    /\ Cardinality(Slotted(r)) <= K                                  \* fetch slots
    /\ GlobalUsed(r) <= Permits                                      \* global permits
    /\ \A p \in Nodes : PeerUsed(r, p) <= PeerPermits                \* per-provider permits
    /\ MemUsed(r) <= MemCap                                          \* the in-memory map
    /\ \A s \in Nodes : Quota(r, s) <= PeerQuota                     \* each sender's quota
    /\ Cardinality(pendDur[r]) <= DurCap                             \* the durable cap
    /\ Resident(r) \subseteq pendDur[r]                              \* memory is backed

\* Promise: a deliverable head written on a node eventually reaches every node
\* the replicator edges lead it to, through any hub. Fairness assumes
\* connections come back; nothing else.
LIVE_HeadDelivered ==
  \A hd \in Deliverable : \A n \in Nodes \ {Author[hd]} :                     \* every deliverable head, every other node
    <<Author[hd], n>> \in ReplPlus =>                                         \* the edges lead it there
      ((hd \in merged[Author[hd]]) ~> (hd \in merged[n]))                     \* written there, merged here
=============================================================================
