---- MODULE GovernanceContract ----
\* The merge-governance contract: what a merge validator plugged into
\* crates/db/src/merge/governance/ may assume of the database, as an abstract
\* specification.
\*
\* This is the SPECIFICATION side of a two-module argument:
\*
\*   GovernanceContract.tla   what a plugin may assume (this module)
\*   GovernanceMerge.tla      what the merge path does, anchored to the Rust
\*
\* GovernanceMerge is checked to IMPLEMENT this module under a refinement
\* mapping (GovernanceMerge_DESIGN.md), so the plugin interface's promises are
\* a machine-checked relationship between two models rather than doc comments.
\*
\* A plugin author models their own validator against THIS module: the
\* constants below are the interface. Nothing here is specific to any one
\* validator. The Fefra capability layer (sourcenetwork/fefra) is where the
\* corpus was first written and is one instance of a plugin.
\*
\* The validator itself is NOT modelled. It is abstracted to the facts
\* progress depends on:
\*
\*   Needs[w]     the inputs a verdict over composite w must read to accept.
\*                Accept iff Needs[w] \subseteq held; otherwise Defer.
\*   Bad          the composites that reject on present bytes. By the purity
\*                contract (MergeValidator docs) a reject never rests on
\*                absence, so membership is a property of the composite
\*                alone, decided on first sight.
\*   Deliverable  the inputs replication will actually deliver. An input
\*                outside it is withheld by policy or does not exist yet, so
\*                a composite needing it defers forever for a CONTENT reason
\*                rather than a host-machinery reason. The liveness
\*                properties tell the two apart; without that distinction
\*                "some composite never settles" would name both the bug and
\*                the correct behaviour.
\*
\* Self and Refs together abstract what a Defer can name in
\* MergeVerdict::Defer { awaiting }: an unheld input is nameable iff the
\* composite pins it (Self) or some held input references it (Refs). A
\* composite that pins nothing and holds nothing referencing its missing
\* inputs names nothing, which is the case the sweep exists for.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Replicas,
  Entries,       \* inputs a verdict may read, by CID
  Writes,        \* candidate composites
  Logs,          \* immutable-field await keys: (collection, field, value)
  Needs,         \* [Writes -> SUBSET Entries]   reads an accept requires
  Self,          \* [Writes -> SUBSET Entries]   entries the composite itself pins
  Refs,          \* [Entries -> SUBSET Entries]  e names Refs[e] when held
  Log,           \* [Entries -> Logs]            the field key an entry answers to
  AwaitLogs,     \* [Writes -> SUBSET Logs]      field keys a defer may await
  Bad,           \* SUBSET Writes                rejects on present bytes
  Deliverable,   \* SUBSET Entries               what replication will deliver
  MaxIndexed,    \* defer-index capacity (MAX_DEFERRED_COMPOSITES, 16 384)
  MaxRestarts,   \* bound on restarts, so behaviours are not trivially unfair
  Redrive,       \* re-drive waiters when an awaited composite merges
  RetryClock,    \* a sweep re-judges deferred composites with no arrival
  FieldAwait,    \* WaitKey::ImmutableField: await a field key, not only a CID
  Recovery,      \* restart re-drives the unmerged composites
  Repush,        \* a remote re-push of a quarantined composite is re-judged
  AbsenceReject, \* purity VIOLATED: reject a composite for what it lacks
  GC,            \* node-local disposition may forget a held input
  GCFloor,       \* the floor a disposition policy may not go below
  MaxForgets     \* bound on forgets, so behaviours are not trivially unfair

ASSUME Replicas # {}
ASSUME Needs \in [Writes -> SUBSET Entries]
ASSUME Self \in [Writes -> SUBSET Entries]
ASSUME Refs \in [Entries -> SUBSET Entries]
ASSUME Log \in [Entries -> Logs]
ASSUME AwaitLogs \in [Writes -> SUBSET Logs]
ASSUME Bad \subseteq Writes
ASSUME Deliverable \subseteq Entries
ASSUME MaxIndexed \in Nat
ASSUME MaxRestarts \in Nat
ASSUME Redrive \in BOOLEAN
ASSUME RetryClock \in BOOLEAN
ASSUME FieldAwait \in BOOLEAN
ASSUME Recovery \in BOOLEAN
ASSUME Repush \in BOOLEAN
ASSUME AbsenceReject \in BOOLEAN
ASSUME GC \in BOOLEAN
ASSUME GCFloor \in BOOLEAN
ASSUME MaxForgets \in Nat

Verdicts == {"None", "Accept", "Reject"}

VARIABLES
  held,       \* [Replicas -> SUBSET Entries]  the blockstore
  arrived,    \* [Replicas -> SUBSET Writes]   composites delivered here
  final,      \* [Replicas -> [Writes -> Verdicts]]  merged set + quarantine
  awaiting,   \* [Replicas -> [Writes -> SUBSET Entries]]  the defer index
  queue,      \* [Replicas -> SUBSET Writes]   waiters a drained budget left
  restarts,   \* how many restarts have happened, bounded by MaxRestarts
  forgotten,  \* [Replicas -> SUBSET Entries]  dropped by disposition, not re-delivered
  forgets,    \* how many inputs have been forgotten, bounded by MaxForgets
  wasJust     \* ghost: [Replicas -> [Writes -> BOOLEAN]] was an Accept
              \* justified by what the replica held when it was recorded

vars == << held, arrived, final, awaiting, queue, restarts, forgotten, forgets, wasJust >>

\* A composite is deferred at r when it has arrived and holds no final
\* verdict: MergeOutcome::Skipped { terminal: false }, unmerged in the
\* blockstore. The deferred set is derived, not stored.
Deferred(r) == { w \in arrived[r] : final[r][w] = "None" }

\* The defer index (DeferredMerges) is in memory and bounded.
Indexed(r) == { w \in Writes : awaiting[r][w] # {} }

\* ---- The verdict, abstracted (MergeValidator::validate) ----

Judgement(r, w) ==
  IF w \in Bad THEN "Reject"
  ELSE IF Needs[w] \subseteq held[r] THEN "Accept"
  ELSE IF AbsenceReject THEN "Reject"   \* the purity violation; MC_GovernanceContract_Red_Absence
  ELSE "None"

\* What a defer can name (MergeVerdict::Defer { awaiting }).
Nameable(r, w) ==
  { e \in Needs[w] \ held[r] :
      \/ e \in Self[w]
      \/ \E f \in held[r] : e \in Refs[f] }

\* Judging w at r. A Defer is indexed by what it can name, subject to the
\* index capacity: at capacity a new defer is NOT indexed (DeferredMerges::defer)
\* and only the sweep ever reaches it. A composite already holding a slot may
\* always re-index into it.
Judge(r, w) ==
  LET v    == Judgement(r, w)
      room == Cardinality(Indexed(r)) < MaxIndexed \/ awaiting[r][w] # {}
  IN
  /\ final' = [final EXCEPT ![r][w] = v]
  /\ awaiting' = [awaiting EXCEPT ![r][w] =
                    IF v # "None"  THEN {}
                    ELSE IF room   THEN Nameable(r, w)
                                   ELSE {}]
  /\ wasJust' = [wasJust EXCEPT ![r][w] =
                   IF v = "Accept" THEN Needs[w] \subseteq held[r] ELSE @]

\* ---- Events the host raises ----

\* Which deferred composites the arrival of e re-drives. A composite CID
\* fires only for a composite that named it (WaitKey::Composite). Under
\* FieldAwait an entry of an awaited field key fires too (WaitKey::ImmutableField),
\* whether or not the composite could ever have named a CID.
Waiters(r, e) ==
  { w \in Deferred(r) :
      \/ (Redrive /\ e \in awaiting[r][w])
      \/ (FieldAwait /\ Log[e] \in AwaitLogs[w]) }

\* An input arrives, in any order, and its waiters are queued for re-drive
\* (DeferredMerges::release). Only Deliverable inputs ever arrive; the rest
\* are withheld or do not exist. The per-drain budget (REDRIVE_BUDGET) is
\* abstracted by queueing the waiters rather than re-judging them inline;
\* DrainOne spends the budget, so a configuration that never drains models a
\* budget that ran out with nothing coming back.
DeliverEntry(r, e) ==
  /\ e \in Deliverable
  /\ e \notin forgotten[r]
  /\ e \notin held[r]
  /\ held' = [held EXCEPT ![r] = @ \cup {e}]
  /\ queue' = [queue EXCEPT ![r] = @ \cup Waiters(r, e)]
  /\ UNCHANGED << arrived, final, awaiting, restarts, forgotten, forgets, wasJust >>

\* The merge attempt: the host calls the validator once on arrival and maps
\* the verdict (judge_governed).
DeliverWrite(r, w) ==
  /\ w \notin arrived[r]
  /\ arrived' = [arrived EXCEPT ![r] = @ \cup {w}]
  /\ Judge(r, w)
  /\ UNCHANGED << held, queue, restarts, forgotten, forgets >>

\* Re-drive a queued waiter through the same verdict path (redrive_deferred).
DrainOne(r, w) ==
  /\ w \in queue[r]
  /\ queue' = [queue EXCEPT ![r] = @ \ {w}]
  /\ IF final[r][w] = "None"
       THEN Judge(r, w)
       ELSE UNCHANGED << final, awaiting, wasJust >>    \* already final
  /\ UNCHANGED << held, arrived, restarts, forgotten, forgets >>

\* The sweep (sweep_unmerged_governed): re-judge a deferred composite with no
\* arrival to prompt it. The backstop that a defer naming nothing, the index
\* bound and a restart all rest on.
Retry(r, w) ==
  /\ RetryClock
  /\ w \in Deferred(r)
  /\ Judge(r, w)
  /\ UNCHANGED << held, arrived, queue, restarts, forgotten, forgets >>

\* Quarantine is local and a remote re-push is judged afresh, against what
\* the replica holds NOW.
\*
\* This is safe only because a reject never rests on absence. If it could, a
\* re-push after more inputs arrived could return a different verdict and two
\* replicas would disagree. MC_GovernanceContract_Red_Absence turns that
\* clause off and TLC produces exactly that split.
RePushQuarantined(r, w) ==
  /\ Repush
  /\ final[r][w] = "Reject"
  /\ Judge(r, w)
  /\ UNCHANGED << held, arrived, queue, restarts, forgotten, forgets >>

\* Restart. The blockstore, the merged set and the quarantine are durable;
\* the defer index and the re-drive queue are in memory. With Recovery the
\* unmerged composites are re-driven after the restart (the sweep's startup
\* pass); without it the index dies with the process and nothing looks at
\* those composites again.
Restart(r) ==
  /\ restarts < MaxRestarts
  /\ restarts' = restarts + 1
  /\ awaiting' = [awaiting EXCEPT ![r] =
                    IF Recovery THEN @ ELSE [w \in Writes |-> {}]]
  /\ queue' = [queue EXCEPT ![r] = IF Recovery THEN Deferred(r) ELSE {}]
  /\ UNCHANGED << held, arrived, final, forgotten, forgets, wasJust >>

\* The floor a disposition policy may not go below. An input may be
\* forgotten only once no composite that needs it is still unsettled here,
\* including composites that have not arrived yet, which is the case a
\* policy keyed on arrival would miss. Expiry is one way to approximate this.
GCFloorOK(r, e) == \A w \in Writes : e \in Needs[w] => final[r][w] # "None"

\* Disposition: drop a held input by node-local policy. It is never a
\* verdict input; it only removes bytes this replica holds. The database
\* does not yet expose such a primitive; the lever is here so that the
\* obligation on one is stated before it exists.
\*
\* A forgotten input is NOT re-delivered. That is what makes the floor
\* load-bearing: were re-delivery guaranteed, forgetting would cost nothing
\* and any policy would be safe.
Forget(r, e) ==
  /\ GC
  /\ forgets < MaxForgets
  /\ e \in held[r]
  /\ (GCFloor => GCFloorOK(r, e))
  /\ held' = [held EXCEPT ![r] = @ \ {e}]
  /\ forgotten' = [forgotten EXCEPT ![r] = @ \cup {e}]
  /\ forgets' = forgets + 1
  /\ UNCHANGED << arrived, final, awaiting, queue, restarts, wasJust >>

Next ==
  \/ \E r \in Replicas, e \in Entries : Forget(r, e)
  \/ \E r \in Replicas, e \in Entries : DeliverEntry(r, e)
  \/ \E r \in Replicas, w \in Writes  : DeliverWrite(r, w)
  \/ \E r \in Replicas, w \in Writes  : DrainOne(r, w)
  \/ \E r \in Replicas, w \in Writes  : Retry(r, w)
  \/ \E r \in Replicas, w \in Writes  : RePushQuarantined(r, w)
  \/ \E r \in Replicas                : Restart(r)

Init ==
  /\ held = [r \in Replicas |-> {}]
  /\ arrived = [r \in Replicas |-> {}]
  /\ final = [r \in Replicas |-> [w \in Writes |-> "None"]]
  /\ awaiting = [r \in Replicas |-> [w \in Writes |-> {}]]
  /\ queue = [r \in Replicas |-> {}]
  /\ restarts = 0
  /\ forgotten = [r \in Replicas |-> {}]
  /\ forgets = 0
  /\ wasJust = [r \in Replicas |-> [w \in Writes |-> TRUE]]

\* Replication eventually delivers every deliverable input and every
\* composite to every replica; the host eventually drains its queue and,
\* when configured, runs its sweep. Fairness is per (replica, item), not per
\* action, so no single composite can be starved by a scheduler that keeps
\* choosing another.
\*
\* Restart and RePushQuarantined are deliberately NOT fair: they may happen
\* or not. Restart is bounded by MaxRestarts so that a behaviour cannot
\* restart forever and call the resulting lack of progress a liveness failure.
Fairness ==
  /\ \A r \in Replicas, e \in Entries : WF_vars(DeliverEntry(r, e))
  /\ \A r \in Replicas, w \in Writes  : WF_vars(DeliverWrite(r, w))
  /\ \A r \in Replicas, w \in Writes  : WF_vars(DrainOne(r, w))
  /\ \A r \in Replicas, w \in Writes  : WF_vars(Retry(r, w))

Spec == Init /\ [][Next]_vars /\ Fairness

\* ---- Safety ----

TypeOK ==
  /\ held \in [Replicas -> SUBSET Entries]
  /\ arrived \in [Replicas -> SUBSET Writes]
  /\ final \in [Replicas -> [Writes -> Verdicts]]
  /\ awaiting \in [Replicas -> [Writes -> SUBSET Entries]]
  /\ queue \in [Replicas -> SUBSET Writes]
  /\ restarts \in 0 .. MaxRestarts
  /\ forgotten \in [Replicas -> SUBSET Entries]
  /\ forgets \in 0 .. MaxForgets
  /\ wasJust \in [Replicas -> [Writes -> BOOLEAN]]

\* A reject rests on present bytes, never on absence. No composite outside
\* Bad is ever quarantined, however little the replica holds.
INV_RejectIntrinsic ==
  \A r \in Replicas, w \in Writes : final[r][w] = "Reject" => w \in Bad

\* An Accept is justified by what the replica holds when it is recorded.
\*
\* Without GC, held only grows, so "was justified" and "is justified"
\* coincide. GC separates them: a settled composite's inputs may be
\* forgotten, after which the accept is still correct but no longer
\* re-derivable here. The ghost wasJust records the fact at the moment of
\* the verdict; INV_AcceptHeldNow is the strong form, true only when nothing
\* is ever forgotten.
INV_AcceptJustified ==
  \A r \in Replicas, w \in Writes :
    final[r][w] = "Accept" => wasJust[r][w]

INV_AcceptHeldNow ==
  \A r \in Replicas, w \in Writes :
    final[r][w] = "Accept" => Needs[w] \subseteq held[r]

\* A defer names only inputs the verdict could actually read, and a settled
\* composite awaits nothing.
\*
\* Not stated as `\subseteq Needs[w] \ held[r]`: the index is written when
\* the composite is judged and is not revised until it is judged again, so
\* between an arrival and its re-drive the index legitimately names an input
\* the replica now holds.
INV_AwaitSound ==
  \A r \in Replicas, w \in Writes :
    /\ awaiting[r][w] \subseteq Needs[w]
    /\ final[r][w] # "None" => awaiting[r][w] = {}

\* Two replicas never hold opposite final verdicts for one composite.
INV_NoSplit ==
  \A r1, r2 \in Replicas, w \in Writes :
    ~(final[r1][w] = "Accept" /\ final[r2][w] = "Reject")

\* A deferred composite whose named inputs have ALL arrived must be queued
\* for re-drive: the obligation the host owes for every defer that named
\* something.
\*
\* Note what it cannot say. A defer that named nothing satisfies the
\* antecedent vacuously and is excluded, so a composite that could name
\* nothing is invisible to this invariant however long it sits. That
\* composite is caught by the liveness properties and by nothing else here.
INV_NoStuck ==
  \A r \in Replicas : \A w \in Deferred(r) :
    (awaiting[r][w] # {} /\ awaiting[r][w] \subseteq held[r]) => w \in queue[r]

\* The defer index respects its capacity.
INV_IndexBound ==
  \A r \in Replicas : Cardinality(Indexed(r)) <= MaxIndexed

\* ---- Action properties ----

\* A composite that arrived stays arrived, across restarts, since the merged
\* set and the quarantine are durable.
ACT_ArrivedDurable ==
  [][ \A r \in Replicas : arrived[r] \subseteq arrived'[r] ]_vars

\* The blockstore grows by delivery and shrinks ONLY by disposition.
ACT_HeldShrinksOnlyByGC ==
  [][ \A r \in Replicas :
        held[r] \subseteq held'[r] \/ (\E e \in Entries : Forget(r, e)) ]_vars

\* Kept for the non-GC configurations, where it is the stronger statement.
ACT_Durable ==
  [][ \A r \in Replicas :
        /\ held[r] \subseteq held'[r]
        /\ arrived[r] \subseteq arrived'[r] ]_vars

\* A merged composite is never unmerged: Accept is terminal.
ACT_AcceptTerminal ==
  [][ \A r \in Replicas, w \in Writes :
        final[r][w] = "Accept" => final'[r][w] = "Accept" ]_vars

\* No verdict ever flips between the two final answers. Weaker than
\* stickiness for Reject on purpose: a re-push is judged afresh, so a
\* quarantined composite MAY be re-judged; it must simply never come back
\* Accept.
ACT_NoFlip ==
  [][ \A r \in Replicas, w \in Writes :
        /\ (final[r][w] = "Accept" => final'[r][w] # "Reject")
        /\ (final[r][w] = "Reject" => final'[r][w] # "Accept") ]_vars

\* A reject is local. A step that quarantines a composite at one replica
\* leaves every other replica's state exactly as it was.
ACT_RejectLocal ==
  [][ \A r \in Replicas, w \in Writes :
        (final[r][w] # "Reject" /\ final'[r][w] = "Reject") =>
          \A q \in Replicas \ {r} :
            /\ held'[q] = held[q]
            /\ arrived'[q] = arrived[q]
            /\ final'[q] = final[q]
            /\ awaiting'[q] = awaiting[q]
            /\ queue'[q] = queue[q] ]_vars

\* ---- Liveness: the point of this model ----

Settled(r, w) == final[r][w] # "None"

\* A composite can reach a final verdict iff it rejects on its own bytes, or
\* everything it needs will actually be delivered. A composite outside this
\* set defers forever for a CONTENT reason (a withheld or non-existent
\* input), which is correct behaviour and not a liveness failure.
Settleable(w) == w \in Bad \/ Needs[w] \subseteq Deliverable

\* L1: every composite that can settle does settle, everywhere, and stays
\* settled. This is what the host machinery owes: re-drive, the defer index,
\* restart recovery and the sweep exist to make it true.
L1_SettleableSettles ==
  <>[](\A r \in Replicas, w \in Writes : Settleable(w) => Settled(r, w))

\* L2: no composite is left deferred forever once the inputs it needs are
\* all held. The pointwise form, which localises a counterexample.
L2_NoPermanentDefer ==
  \A r \in Replicas, w \in Writes :
    (Needs[w] \subseteq held[r] /\ w \in arrived[r]) ~> Settled(r, w)

\* L3: some composite reaches Accept: the model is not settling everything
\* by rejection. A claim over all behaviours, so in a configuration where an
\* acceptable composite can be stranded this fails too, which is intended.
L3_SomeAccept == <>(\E r \in Replicas, w \in Writes : final[r][w] = "Accept")

\* L4: a composite that cannot settle is never wrongly settled: it stays
\* deferred rather than being rejected for absence or accepted on partial
\* reads. The safety companion to L1's exclusion of those composites.
L4_UnsettleableDefers ==
  [](\A r \in Replicas, w \in Writes :
       (~Settleable(w) /\ w \in arrived[r]) => final[r][w] = "None")

====
