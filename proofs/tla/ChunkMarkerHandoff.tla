---- MODULE ChunkMarkerHandoff ----
\* The story: a request writes an article whose text is `@chunk`. One commit
\* stores the article and all its chunk documents. After the commit, every one
\* of those heads still needs a sender marker, one durable record per head
\* that says "this peer is still owed this head". A big article has hundreds
\* of chunks, so registering their markers takes a while.
\*
\* Who acts:
\*   - the request, which commits and then waits for the markers;
\*   - the registration, which writes the markers one head at a time;
\*   - the request's deadline, which can end the wait at any moment;
\*   - the sender's push and retry sweep, which deliver a marked head and
\*     clear its marker once the peer acknowledges it.
\*
\* What must never happen: a committed head that no marker, no pending
\* registration and no delivery covers (INV_CommittedHeadOwned). Nothing ever
\* sends such a head again, so the peer never gets it. And every committed
\* head must reach the peer in the end (LIVE_AllDelivered).
\*
\* Code it models:
\*   crates/db/src/merge/broadcast_mutator/heads.rs  register_committed_heads
\*     (the registration runs as a coordinator task the request only awaits)
\*   crates/db/src/merge/broadcast_mutator/mod.rs  BroadcastMutator::register_write_heads,
\*     BroadcastMutator::derived_heads, create_as, create_many_as, update, delete_as
\*   crates/db/src/merge/broadcast_mutator/batch.rs  BroadcastBatchMutator::register_pending
\*   crates/query/src/runner/executor.rs  await_with_timeout (the deadline drops the request)
\*   crates/p2p/src/sync/coordinator/broadcast.rs  admit_replicator_push (one marker)
\*
\* KNOBS. Owner says who runs the registration.
\*   Owner = "Coordinator"  a coordinator task runs it; the request only waits
\*                          for its result, so the deadline cannot stop it.  [GREEN]
\*   Owner = "Request"      the request's own future runs it; the deadline drops
\*                          that future with the heads not yet registered.
\*                          INV_CommittedHeadOwned fails.                   [RED]
EXTENDS FiniteSets

CONSTANTS
  Heads,  \* the heads one commit writes: the article's and its chunks'
  Owner   \* "Coordinator" | "Request": who runs the marker registration

\* Owner takes only its two named values.
ASSUME Owner \in {"Coordinator", "Request"}

VARIABLES
  committed,  \* TRUE once the write's transaction has committed
  pending,    \* heads whose marker registration has not run yet
  marked,     \* heads with a durable sender marker
  delivered,  \* heads the peer acknowledged, whose marker is cleared
  waiting     \* TRUE while the request still waits for the registration

vars == <<committed, pending, marked, delivered, waiting>>  \* every variable, for stuttering

\* Every variable holds a value of the right kind.
TypeOK ==
  /\ committed \in BOOLEAN       \* a flag
  /\ pending \subseteq Heads     \* a set of heads
  /\ marked \subseteq Heads      \* a set of heads
  /\ delivered \subseteq Heads   \* a set of heads
  /\ waiting \in BOOLEAN         \* a flag

\* Nothing is written yet, and nobody waits.
Init ==
  /\ committed = FALSE  \* no commit yet
  /\ pending = {}       \* nothing to register
  /\ marked = {}        \* no marker
  /\ delivered = {}     \* nothing delivered
  /\ waiting = FALSE    \* no request waiting

\* The request commits every head at once, then waits for their markers.
Commit ==
  /\ ~committed              \* only one write in this model
  /\ committed' = TRUE       \* the transaction commits
  /\ pending' = Heads        \* each head now needs its marker
  /\ waiting' = TRUE         \* the request waits for them
  /\ UNCHANGED <<marked, delivered>>  \* no marker and no delivery yet

\* One head's marker is written. The coordinator's task keeps going after
\* the request stopped waiting; the request's own future runs only while the
\* request still waits.
Register(h) ==
  /\ h \in pending                              \* this head still needs its marker
  /\ Owner = "Coordinator" \/ waiting           \* someone still runs the registration
  /\ pending' = pending \ {h}                   \* its registration is done
  /\ marked' = marked \cup {h}                  \* and the marker exists
  /\ UNCHANGED <<committed, delivered, waiting>>  \* nothing else moves

\* The request's deadline fires while heads are still unregistered. The
\* request returns an error. With the request as owner, its dropped future
\* takes the remaining registrations with it.
Deadline ==
  /\ waiting                                                \* the request still waits
  /\ pending # {}                                           \* some marker is not written yet
  /\ waiting' = FALSE                                       \* it stops waiting
  /\ pending' = IF Owner = "Request" THEN {} ELSE pending   \* the request's own work is gone
  /\ UNCHANGED <<committed, marked, delivered>>             \* written markers stay

\* Every marker is written, so the request returns.
Return ==
  /\ waiting               \* the request still waits
  /\ pending = {}          \* nothing left to register
  /\ waiting' = FALSE      \* it returns
  /\ UNCHANGED <<committed, pending, marked, delivered>>  \* nothing else moves

\* The push or the retry sweep sends a marked head; the peer acknowledges it
\* and the marker is cleared.
Deliver(h) ==
  /\ h \in marked                          \* this head has a marker
  /\ marked' = marked \ {h}                \* the acknowledgement clears it
  /\ delivered' = delivered \cup {h}       \* the peer has the head
  /\ UNCHANGED <<committed, pending, waiting>>  \* nothing else moves

\* One step of any actor.
Next ==
  \/ Commit                       \* the request commits
  \/ \E h \in Heads : Register(h)  \* a marker is written
  \/ Deadline                     \* the deadline fires
  \/ Return                       \* the request returns
  \/ \E h \in Heads : Deliver(h)   \* a head is delivered

\* The registration and the sender keep running, so each step that can
\* happen does happen. The deadline is not fair: it may or may not fire.
Fairness ==
  /\ WF_vars(Commit)                              \* the write happens
  /\ \A h \in Heads : WF_vars(Register(h))        \* a runnable registration runs
  /\ \A h \in Heads : WF_vars(Deliver(h))         \* a marked head gets sent
  /\ WF_vars(Return)                              \* a finished request returns

\* The whole behaviour.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Every committed head is owed by something: a pending registration, a
\* marker, or it was already delivered.
INV_CommittedHeadOwned ==
  committed => \A h \in Heads : h \in pending \cup marked \cup delivered  \* no head is orphaned

\* Every committed head reaches the peer in the end.
LIVE_AllDelivered ==
  <>(committed /\ delivered = Heads)  \* the peer ends up with all of them
====
