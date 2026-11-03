---- MODULE RetryMarkerGeneration ----
\* The story. A sender owes a replicator peer some document scopes. For each
\* scope it keeps one durable retry marker whose generation changes on every
\* registration (peerstore.rs RegisterScopeMarkerAttempt). Three kinds of
\* worker touch the markers:
\*   - a live push records the generation it saw, sends, and on the peer's
\*     acknowledgement compare-deletes that generation (complete_retry_scope);
\*   - the sweep (p2p-adapter retry.rs run_retry_pass) wakes every tick, asks
\*     whether the peer's shared retry clock is due, and if so replays one
\*     page of markers that starts after a durable cursor (retry_doc);
\*   - peer management forgets and re-adds the peer, which changes its
\*     incarnation, and clears its retry state.
\* A document can also leave this node (purge, truncate): its marker then
\* has nothing left to deliver.
\* What must never happen:
\*   - an old acknowledgement or an old clear erases a newer registration;
\*   - one tick reads more marker keys than one page, whatever the backlog;
\*   - the marker of a document that left this node stays forever.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
  N,                 \* how many document scopes the peer can be owed; their keys are 1..N
  PageSize,          \* markers one due pass may replay (MAX_MARKERS_PER_PEER_PASS)
  CompareGeneration, \* guard: completion deletes only the generation the worker saw
  ValidatePeer,      \* guard: a clear finishes only for the incarnation it began on
  DischargeVanished, \* guard: a replay discharges the marker of a document that is gone
  BoundedSweep,      \* guard: a tick asks "due?" first and then reads one page, not every marker
  RotateCursor       \* guard: a pass that failed moves the cursor past the last scope it tried

\* N and PageSize are small positive counts; the guards are switches.
ASSUME N \in 1..3 /\ PageSize \in 1..3
\* Every guard is on or off.
ASSUME {CompareGeneration, ValidatePeer, DischargeVanished, BoundedSweep, RotateCursor} \subseteq BOOLEAN

\* Scope keys, in the order the peerstore iterates them.
Scopes == 1..N
\* Registrations per scope the model explores.
MaxGen == 2
\* The cursor before any pass has moved it.
NoScope == 0

VARIABLES
  present,     \* the replicator row exists
  incarnation, \* changes on forget and on re-add
  marker,      \* scope -> generation of its latest registration
  observed,    \* scope -> generation the latest worker saw before its network I/O
  pending,     \* scope -> the durable marker exists
  clearEpoch,  \* the incarnation a running clear began on
  clearing,    \* a clear is running
  staleClear,  \* witness: a clear finished across a forget/re-add
  live,        \* scope -> the document still exists on this node
  due,         \* the peer's shared retry clock has elapsed
  cursor,      \* the scope the last failed pass tried last (NoScope at first)
  page,        \* scopes the running pass still has to replay, in order
  failed,      \* the running pass hit a marker it could not discharge
  overRead     \* witness: a tick read more marker keys than its budget

\* Every variable, for stuttering steps.
vars == <<present, incarnation, marker, observed, pending, clearEpoch, clearing,
          staleClear, live, due, cursor, page, failed, overRead>>

\* The variables only the sweep's own steps change.
sweepVars == <<due, cursor, page, failed, overRead>>

\* The variables only peer management changes.
peerVars == <<present, incarnation, clearEpoch, clearing, staleClear>>

\* The scopes whose durable marker exists.
Pending == {s \in Scopes : pending[s]}

\* True for a scope whose marker exists; used to filter a sequence.
IsPending(s) == pending[s]

\* Scope keys in dispatch order: the keys after the cursor, then wrapping round.
Rotation == [i \in 1..N |-> ((cursor + i - 1) % N) + 1]

\* The scopes with a marker, in dispatch order.
DueOrder == SelectSeq(Rotation, IsPending)

\* The page a due pass loads: at most PageSize markers in dispatch order.
NewPage == SubSeq(DueOrder, 1, IF Len(DueOrder) < PageSize THEN Len(DueOrder) ELSE PageSize)

\* The scopes a sequence names.
Range(seq) == {seq[i] : i \in DOMAIN seq}

\* Marker keys a tick may read: one existence probe, plus, when the peer is due,
\* one page and one more key that tells whether the page was cut short.
Budget == 1 + (IF due THEN PageSize + 1 ELSE 0)

\* Marker keys a tick does read. Bounded: the probe, then a page only when due.
\* Unbounded (the code before this change): every marker, before asking "due?".
Reads == IF BoundedSweep
           THEN IF Pending = {} THEN 0
                ELSE 1 + (IF due THEN IF Len(DueOrder) > PageSize THEN PageSize + 1 ELSE Len(DueOrder)
                          ELSE 0)
           ELSE Cardinality(Pending)

\* At the start every scope is dirty at generation 0, the peer is present, its
\* clock is due, and no pass is running.
Init == /\ present = TRUE
        /\ incarnation = 0
        /\ marker = [s \in Scopes |-> 0]
        /\ observed = [s \in Scopes |-> 0]
        /\ pending = [s \in Scopes |-> TRUE]
        /\ clearEpoch = 0
        /\ clearing = FALSE
        /\ staleClear = FALSE
        /\ live = [s \in Scopes |-> TRUE]
        /\ due = TRUE
        /\ cursor = NoScope
        /\ page = <<>>
        /\ failed = FALSE
        /\ overRead = FALSE

\* A local commit on a document that exists registers a fresh generation,
\* even when the scope was already dirty.
Register(s) == /\ present
               /\ live[s]
               /\ marker[s] < MaxGen
               /\ marker' = [marker EXCEPT ![s] = @ + 1]
               /\ pending' = [pending EXCEPT ![s] = TRUE]
               /\ UNCHANGED <<peerVars, observed, live, sweepVars>>

\* A live push records the generation it saw before it sends.
Observe(s) == /\ pending[s]
              /\ observed' = [observed EXCEPT ![s] = marker[s]]
              /\ UNCHANGED <<peerVars, marker, pending, live, sweepVars>>

\* The peer acknowledges the live push. With the guard, a newer registration
\* than the one observed keeps the marker; without it, the ack deletes it.
Ack(s) == /\ pending[s]
          /\ pending' = [pending EXCEPT ![s] = CompareGeneration /\ observed[s] # marker[s]]
          /\ UNCHANGED <<peerVars, marker, observed, live, sweepVars>>

\* Forget deletes the replicator row and changes the incarnation.
Forget == /\ present
          /\ incarnation < 2
          /\ present' = FALSE
          /\ incarnation' = incarnation + 1
          /\ UNCHANGED <<marker, observed, pending, clearEpoch, clearing, staleClear, live, sweepVars>>

\* Re-add installs a new incarnation and registers every document that still exists.
Readd == /\ ~present
         /\ incarnation < 2
         /\ \A s \in Scopes : live[s] => marker[s] < MaxGen
         /\ present' = TRUE
         /\ incarnation' = incarnation + 1
         /\ marker' = [s \in Scopes |-> IF live[s] THEN marker[s] + 1 ELSE marker[s]]
         /\ pending' = [s \in Scopes |-> pending[s] \/ live[s]]
         /\ UNCHANGED <<observed, clearEpoch, clearing, staleClear, live, sweepVars>>

\* A clear records the incarnation it began on before it sweeps retry state.
BeginClear == /\ present
              /\ ~clearing
              /\ clearing' = TRUE
              /\ clearEpoch' = incarnation
              /\ UNCHANGED <<present, incarnation, marker, observed, pending, staleClear, live, sweepVars>>

\* A clear erases the peer's retry state. With the guard it finishes only for
\* the incarnation it began on; without it, staleClear records the crossing.
FinishClear == /\ clearing
               /\ clearing' = FALSE
               /\ IF ValidatePeer
                    THEN /\ clearEpoch = incarnation
                         /\ staleClear' = staleClear
                    ELSE staleClear' = (staleClear \/ (clearEpoch # incarnation))
               /\ pending' = [s \in Scopes |-> FALSE]
               /\ observed' = marker
               /\ UNCHANGED <<present, incarnation, marker, clearEpoch, live, sweepVars>>

\* A purge or truncate removes the document from this node; no commit, no registration.
Vanish(s) == /\ live[s]
             /\ live' = [live EXCEPT ![s] = FALSE]
             /\ UNCHANGED <<peerVars, marker, observed, pending, sweepVars>>

\* The same content-addressed document is created again; its registration follows as Register.
Recreate(s) == /\ ~live[s]
               /\ live' = [live EXCEPT ![s] = TRUE]
               /\ UNCHANGED <<peerVars, marker, observed, pending, sweepVars>>

\* Time passes until the peer's retry clock is due again.
Elapse == /\ ~due
          /\ due' = TRUE
          /\ UNCHANGED <<peerVars, marker, observed, pending, live, cursor, page, failed, overRead>>

\* The two-second tick for a present peer with no pass running: it is charged
\* the keys it reads, and when the peer is due it loads one page after the
\* cursor and records the generation of every marker on it.
Tick == /\ present
        /\ page = <<>>
        /\ overRead' = (overRead \/ Reads > Budget)
        /\ page' = IF due THEN NewPage ELSE <<>>
        /\ observed' = [s \in Scopes |-> IF s \in Range(page') THEN marker[s] ELSE observed[s]]
        /\ failed' = FALSE
        /\ UNCHANGED <<peerVars, marker, pending, live, due, cursor>>

\* The pass replays the next marker on its page. The peer may or may not take a
\* live document. A document that is gone has nothing to send: with the guard
\* the replay discharges its marker, without it the replay errors. Either way a
\* discharge is a compare-delete of the generation the page saw, so a newer
\* registration survives. When the page is done, a pass that failed charges
\* the clock one rung and, with the guard, moves the cursor past its last scope.
Step == /\ page # <<>>
        /\ \E delivered \in BOOLEAN :
             LET s == Head(page)
                 rest == Tail(page)
                 current == ~CompareGeneration \/ observed[s] = marker[s]
                 gone == ~live[s] /\ DischargeVanished
                 took == live[s] /\ delivered
                 ok == pending[s] /\ present /\ (gone \/ took) /\ current
                 nowFailed == failed \/ ~ok
             IN /\ pending' = [pending EXCEPT ![s] = IF ok THEN FALSE ELSE @]
                /\ page' = rest
                /\ failed' = nowFailed
                /\ due' = IF rest = <<>> /\ nowFailed THEN FALSE ELSE due
                /\ cursor' = IF rest = <<>> /\ nowFailed /\ RotateCursor THEN s ELSE cursor
        /\ UNCHANGED <<peerVars, marker, observed, live, overRead>>

\* Any one thing can happen next.
Next == \/ \E s \in Scopes : Register(s) \/ Observe(s) \/ Ack(s) \/ Vanish(s) \/ Recreate(s)
        \/ Forget \/ Readd \/ BeginClear \/ FinishClear
        \/ Elapse \/ Tick \/ Step

\* Safety runs: any interleaving at all.
Spec == Init /\ [][Next]_vars

\* Liveness runs: only the scheduler is fair. The tick keeps firing, the
\* running pass keeps stepping, and the clock keeps elapsing. No scope, no
\* acknowledgement and no peer outcome is promised.
FairSpec == Spec /\ WF_vars(Tick) /\ WF_vars(Step) /\ WF_vars(Elapse)

\* Every variable stays in its intended range.
TypeOK == /\ present \in BOOLEAN
          /\ incarnation \in 0..2
          /\ marker \in [Scopes -> 0..MaxGen]
          /\ observed \in [Scopes -> 0..MaxGen]
          /\ pending \in [Scopes -> BOOLEAN]
          /\ clearEpoch \in 0..2
          /\ clearing \in BOOLEAN
          /\ staleClear \in BOOLEAN
          /\ live \in [Scopes -> BOOLEAN]
          /\ due \in BOOLEAN
          /\ cursor \in 0..N
          /\ Len(page) <= PageSize /\ Range(page) \subseteq Scopes
          /\ failed \in BOOLEAN
          /\ overRead \in BOOLEAN

\* An old acknowledgement never removed newer work: a scope without a marker
\* was last seen at its current generation.
INV_NewRegistrationSurvivesOldAck == \A s \in Scopes : pending[s] \/ marker[s] = observed[s]

\* A clear that crossed a forget/re-add never finished.
INV_NewRegistrationSurvivesOldClear == ~staleClear

\* No tick ever read more marker keys than one probe plus, when due, one page.
INV_SweepReadsBounded == ~overRead

\* A marker whose document left this node, at a present peer, is eventually
\* discharged, unless the document comes back or the peer is forgotten.
LIVE_VanishedMarkerDischarged ==
  \A s \in Scopes : (pending[s] /\ ~live[s] /\ present) ~> (~pending[s] \/ live[s] \/ ~present)
====
