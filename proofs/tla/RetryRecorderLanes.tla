---- MODULE RetryRecorderLanes ----
\* The story. Local writes and push workers hand the failure recorder
\* records, each about one replicator peer: a marker to register before a
\* push, an acknowledgement that clears one, a failure to keep on the retry
\* ladder. The recorder makes each durable and only then answers its sender
\* (crates/surfaces/p2p-adapter/src/retry.rs spawn_failure_recorder).
\*
\* It used to apply them one at a time, one transaction and one device sync
\* each, so a write waited for a sync per replicator, behind every record
\* ahead of it (#3471). Now it has lanes:
\*   - a record goes to the lane its peer id hashes to (lane_of), so all of a
\*     peer's records go to one lane, in the order they were sent;
\*   - a lane takes every record waiting in it (a round), splits them by peer
\*     keeping each peer's order, and runs the peers at once (run_lane); one
\*     task per peer applies that peer's records in order, a run of
\*     registrations and acknowledgements as one transaction
\*     (crates/engine/storage/src/stores/peerstore/scope_batch.rs
\*     Peerstore::record_scopes) and any other record as its own call
\*     (record_peer);
\*   - a lane starts its next round only when the last one is done; lanes do
\*     not wait for each other.
\* Transactions of different peers commit at once and share their device
\* syncs. Every key a record validates belongs to its peer (its marker, its
\* schedule row, its replicator row), so running different peers at once adds
\* no conflict, while one peer's records never run beside each other.
\*
\* The model treats a peer's share of a round as one transaction: in the code
\* its pieces run one after another in that peer's one task, so nothing else
\* of that peer runs meanwhile, which is all the model needs.
\*
\* What must never happen:
\*   - two transactions of one peer are in flight at once (INV_OnePeerAtATime);
\*   - a peer's records are applied out of the order they were sent
\*     (INV_PeerOrder);
\*   - a sender is answered before its record is durable
\*     (INV_AnsweredOnlyWhenDurable).
\* What must happen: every record sent is answered (LIVE_EveryRecordAnswered).

\* Naturals gives numbers; Sequences gives queues; FiniteSets gives set sizes.
EXTENDS Naturals, Sequences, FiniteSets

\* The guards each configuration switches.
CONSTANTS
  LaneByPeer,       \* guard: a record's lane is its peer's lane
  OneTxnPerPeer,    \* guard: a round applies a peer's records as one transaction
  AnswerAfterCommit \* guard: a sender is answered once its transaction commits

\* The guards are on or off.
ASSUME LaneByPeer \in BOOLEAN /\ OneTxnPerPeer \in BOOLEAN /\ AnswerAfterCommit \in BOOLEAN

\* The bound: three peers sharing two lanes, and four records, two of them
\* about the same peer, so a lane holds several peers and a peer several records.
Peers == {"p1", "p2", "p3"}
\* Two lanes, fewer than the peers, so two peers hash to one lane.
Lanes == {1, 2}
\* Four records, numbered 1..4 in the order they are sent.
N == 4
\* The records, in the order senders hand them over.
Records == 1..N
\* Records 1 and 4 are about p1, record 2 about p2, record 3 about p3.
PeerOf == [r \in Records |-> CASE r = 2 -> "p2" [] r = 3 -> "p3" [] OTHER -> "p1"]
\* p1 and p3 hash to lane 1, p2 to lane 2.
LaneOfPeer == [p \in Peers |-> IF p = "p2" THEN 2 ELSE 1]

VARIABLES
  sent,     \* how many records have been handed to the recorder
  queue,    \* queue[l]: the records waiting in lane l, oldest first
  txns,     \* the transactions in flight: [lane, peer, batch]
  applied,  \* applied[p]: peer p's records made durable, in commit order
  answered  \* the records whose sender has been answered

\* Every variable, for steps that change nothing and for fairness.
vars == <<sent, queue, txns, applied, answered>>

\* Peer p's records in the order they were sent.
OrderOf(p) == SelectSeq([i \in Records |-> i], LAMBDA r : PeerOf[r] = p)

\* The set of the elements of sequence s.
Elems(s) == {s[i] : i \in 1..Len(s)}

\* Every record made durable so far.
Durable == UNION {Elems(applied[p]) : p \in Peers}

\* At the start nothing is sent, waiting, running, durable or answered.
Init == /\ sent = 0                            \* no record handed over
        /\ queue = [l \in Lanes |-> <<>>]      \* every lane empty
        /\ txns = {}                           \* no transaction in flight
        /\ applied = [p \in Peers |-> <<>>]    \* nothing durable
        /\ answered = {}                       \* nobody answered

\* A sender hands over the next record. With the guard on it goes to its
\* peer's lane; with it off (the RED) to any lane, as hashing by the whole
\* scope would.
Send == /\ sent < N                                              \* records remain to send
        /\ LET r == sent + 1 IN                                  \* the next record
             \E l \in Lanes :                                    \* some lane
               /\ LaneByPeer => l = LaneOfPeer[PeerOf[r]]        \* its peer's, with the guard
               /\ queue' = [queue EXCEPT ![l] = Append(@, r)]    \* it waits there
        /\ sent' = sent + 1                                      \* one more handed over
        /\ UNCHANGED <<txns, applied, answered>>                 \* nothing runs yet

\* Lane l, idle, takes every record waiting in it and starts its round: per
\* peer, the peer's records in order as one transaction. With that guard off
\* (the RED) every record is a transaction of its own, all at once. Without
\* the answer guard (the RED) the senders are answered now.
Take(l) ==
  /\ queue[l] # <<>>                                             \* something waits
  /\ \A t \in txns : t.lane # l                                  \* its last round is done
  /\ LET waiting == queue[l]                                     \* what it takes
         peers == {PeerOf[waiting[i]] : i \in 1..Len(waiting)}    \* whose records they are
         mine(p) == SelectSeq(waiting, LAMBDA r : PeerOf[r] = p) \* p's records, in order
         round == IF OneTxnPerPeer
                    THEN {[lane |-> l, peer |-> p, batch |-> mine(p)] : p \in peers}  \* one per peer
                    ELSE {[lane |-> l, peer |-> PeerOf[r], batch |-> <<r>>] : r \in Elems(waiting)} \* one per record
     IN /\ txns' = txns \cup round                               \* the round is in flight
        /\ answered' = IF AnswerAfterCommit THEN answered        \* answers wait for the commits
                       ELSE answered \cup Elems(waiting)          \* RED: answered at once
  /\ queue' = [queue EXCEPT ![l] = <<>>]                          \* the lane is empty again
  /\ UNCHANGED <<sent, applied>>                                 \* nothing durable yet

\* Transaction t commits: its records become durable in its order, after
\* what its peer already has, and their senders are answered.
Commit(t) == /\ t \in txns                                                \* it is in flight
             /\ applied' = [applied EXCEPT ![t.peer] = @ \o t.batch]       \* durable, in order
             /\ answered' = answered \cup Elems(t.batch)                   \* senders answered
             /\ txns' = txns \ {t}                                         \* no longer in flight
             /\ UNCHANGED <<sent, queue>>                                  \* nothing else moves

\* Every step: a send, a lane starting a round, or a commit.
Next == Send \/ (\E l \in Lanes : Take(l)) \/ (\E t \in txns : Commit(t))

\* Senders keep sending, idle lanes keep taking, transactions keep committing.
Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(Send)                                   \* every record is sent
        /\ \A l \in Lanes : WF_vars(Take(l))               \* a lane takes what waits
        /\ WF_vars(\E t \in txns : Commit(t))              \* in-flight work commits

\* Every variable holds a value of the kind it should.
TypeOK == /\ sent \in 0..N                                 \* a count of sent records
          /\ queue \in [Lanes -> Seq(Records)]             \* lanes hold records
          /\ \A t \in txns : t.lane \in Lanes /\ t.peer \in Peers \* a transaction's lane and peer
          /\ applied \in [Peers -> Seq(Records)]           \* durable records per peer
          /\ answered \subseteq Records                    \* answered are records

\* One peer never has two transactions in flight, so batching adds no conflict.
INV_OnePeerAtATime == \A t1, t2 \in txns : t1 # t2 => t1.peer # t2.peer

\* Each peer's durable records are the first ones it was sent, in that order.
INV_PeerOrder == \A p \in Peers :
                   applied[p] = SubSeq(OrderOf(p), 1, Len(applied[p]))

\* A sender is answered only once its record is durable.
INV_AnsweredOnlyWhenDurable == answered \subseteq Durable

\* Every record sent is eventually answered.
LIVE_EveryRecordAnswered == <>(answered = Records)
====
