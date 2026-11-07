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
\*   - an old acknowledgement or an old clear erases a newer registration
\*     (INV_NewRegistrationSurvivesOldAck, ACT_NewRegistrationSurvivesOldClear);
\*   - one tick reads more marker keys than one page, whatever the backlog
\*     (INV_SweepReadsBounded);
\*   - the marker of a document that left this node stays forever at a peer
\*     the sweep reaches (LIVE_VanishedMarkerDischarged).
\* The peer is always reachable here: a pass for a peer the transport does not
\* report connected redials and replays nothing, so it keeps every marker.
\*
\* Every variable is state the code holds; the properties are stated over that
\* state, as invariants or as properties of each step, and nothing is recorded
\* only to state them.
\*
\* Variables and the code state each one is (crates/storage/src/stores, and
\* crates/p2p-adapter/src/retry.rs run_retry_pass):
\*   present      the peer's replicator row (Peerstore::get_replicator).
\*   incarnation  Peerstore::get_replicator_incarnation.
\*   marker[s]    the generation stored at scope s's marker key
\*                (RegisterScopeMarkerAttempt).
\*   pending[s]   that marker key exists.
\*   observed[s]  the generation the latest worker on s read before its network
\*                I/O: the live push's, or PushRetryMarker::generation on the
\*                sweep's page.
\*   clearEpoch   the expected_incarnation a running
\*                clear_retry_peer_for_incarnation was given.
\*   clearing     that clear is running.
\*   live[s]      s's DocID mapping exists (the lookup at the top of retry_doc).
\*   due          RetryInfo::is_due for the peer's shared clock.
\*   cursor       RetryInfo::dispatch_after (NoScope is None).
\*   page         the entries of run_retry_pass's `markers` its loop has yet to
\*                try, at most MAX_MARKERS_PER_PEER_PASS.
\*   failed       run_retry_pass's `failed`.
EXTENDS Naturals, Sequences

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

VARIABLES present, incarnation, marker, observed, pending, clearEpoch, clearing,
          live, due, cursor, page, failed

\* Every variable, for stuttering steps.
vars == <<present, incarnation, marker, observed, pending, clearEpoch, clearing,
          live, due, cursor, page, failed>>

\* The variables only the sweep's own steps change.
sweepVars == <<due, cursor, page, failed>>

\* The variables only peer management changes.
peerVars == <<present, incarnation, clearEpoch, clearing>>

\* True for a scope whose marker exists; used to filter a sequence.
IsPending(s) == pending[s]

\* Scope keys in dispatch order: the keys after the cursor, then wrapping round.
Rotation == [i \in 1..N |-> ((cursor + i - 1) % N) + 1]

\* The scopes with a marker, in dispatch order.
DueOrder == SelectSeq(Rotation, IsPending)

\* The first k entries of a sequence, or all of it when it is shorter.
Prefix(seq, k) == SubSeq(seq, 1, IF Len(seq) < k THEN Len(seq) ELSE k)

\* get_retry_page(peer, k): at most k markers in dispatch order. Its iterator
\* reads one marker key per marker it returns.
Load(k) == Prefix(DueOrder, k)

\* What a tick loads before it asks "due?", and the pass loads again at its end
\* to decide `complete`: get_retry_page(peer, 1). With BoundedSweep off it is
\* get_retry_documents, which returns every marker.
Probe == IF BoundedSweep THEN Load(1) ELSE DueOrder

\* The marker keys a due tick reads after its probe: one page and one more
\* marker that tells whether the page was cut short. With BoundedSweep off the
\* probe already holds every marker, and the pass replays from it.
PageRead == IF BoundedSweep THEN Load(PageSize + 1) ELSE <<>>

\* What a due tick holds for its pass (`markers`).
Held == IF BoundedSweep THEN PageRead ELSE Probe

\* Whether this tick's peer has markers and its clock is due.
Runs == due /\ Probe # <<>>

\* The scopes a sequence names.
Range(seq) == {seq[i] : i \in DOMAIN seq}

\* At the start every scope is dirty at generation 0, the peer is present, its
\* clock is due, and no pass is running.
Init == /\ present = TRUE
        /\ incarnation = 0
        /\ marker = [s \in Scopes |-> 0]
        /\ observed = [s \in Scopes |-> 0]
        /\ pending = [s \in Scopes |-> TRUE]
        /\ clearEpoch = 0
        /\ clearing = FALSE
        /\ live = [s \in Scopes |-> TRUE]
        /\ due = TRUE
        /\ cursor = NoScope
        /\ page = <<>>
        /\ failed = FALSE

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
          /\ UNCHANGED <<marker, observed, pending, clearEpoch, clearing, live, sweepVars>>

\* Re-add installs a new incarnation and registers every document that still exists.
Readd == /\ ~present
         /\ incarnation < 2
         /\ \A s \in Scopes : live[s] => marker[s] < MaxGen
         /\ present' = TRUE
         /\ incarnation' = incarnation + 1
         /\ marker' = [s \in Scopes |-> IF live[s] THEN marker[s] + 1 ELSE marker[s]]
         /\ pending' = [s \in Scopes |-> pending[s] \/ live[s]]
         /\ UNCHANGED <<observed, clearEpoch, clearing, live, sweepVars>>

\* A clear records the incarnation it began on before it sweeps retry state.
BeginClear == /\ present
              /\ ~clearing
              /\ clearing' = TRUE
              /\ clearEpoch' = incarnation
              /\ UNCHANGED <<present, incarnation, marker, observed, pending, live, sweepVars>>

\* A clear erases the peer's retry state. With the guard it finishes only for
\* the incarnation it began on; without it, also across a forget/re-add.
FinishClear == /\ clearing
               /\ clearing' = FALSE
               /\ ValidatePeer => clearEpoch = incarnation
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
          /\ UNCHANGED <<peerVars, marker, observed, pending, live, cursor, page, failed>>

\* The two-second tick for a present peer with no pass running: it probes, and
\* when the peer is due it replays the first PageSize markers it holds, which
\* start after the cursor, and records the generation of every marker on them.
Tick == /\ present
        /\ page = <<>>
        /\ page' = IF Runs THEN Prefix(Held, PageSize) ELSE <<>>
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
        /\ UNCHANGED <<peerVars, marker, observed, live>>

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
          /\ live \in [Scopes -> BOOLEAN]
          /\ due \in BOOLEAN
          /\ cursor \in 0..N
          /\ Len(page) <= PageSize /\ Range(page) \subseteq Scopes
          /\ failed \in BOOLEAN

\* An old acknowledgement never removed newer work: a scope without a marker
\* was last seen at its current generation.
INV_NewRegistrationSurvivesOldAck == \A s \in Scopes : pending[s] \/ marker[s] = observed[s]

\* A clear that began before a forget/re-add never finishes.
ACT_NewRegistrationSurvivesOldClear == [][FinishClear => clearEpoch = incarnation]_vars

\* What the sweep reads never grows with the backlog: the probe a tick reads
\* before "due?", which is also the probe that ends a pass, is one marker key,
\* and what a due tick reads after it is one page and one more key.
INV_SweepReadsBounded == Len(Probe) <= 1 /\ Len(PageRead) <= PageSize + 1

\* A marker whose document left this node, at a present peer, is eventually
\* discharged, unless the document comes back or the peer is forgotten.
LIVE_VanishedMarkerDischarged ==
  \A s \in Scopes : (pending[s] /\ ~live[s] /\ present) ~> (~pending[s] \/ live[s] \/ ~present)
====
