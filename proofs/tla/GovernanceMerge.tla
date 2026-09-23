---- MODULE GovernanceMerge ----
\* What the governed-merge path does, at the granularity the plugin contract
\* cares about, and a refinement check that it IMPLEMENTS
\* GovernanceContract.tla.
\*
\*   GovernanceContract.tla   what a plugin may assume     (the specification)
\*   GovernanceMerge.tla      what the merge path does     (this module)
\*   MC_GovernanceMerge_*     the knob settings: today's mechanism, the ones
\*                            that came before it, and the refactors to avoid
\*
\* Anchors (crates/db/src/merge/governance/ unless said otherwise; line
\* numbers in GovernanceMerge_DESIGN.md):
\*   validator.rs   MergeValidator::validate, the purity contract
\*   verdict.rs     MergeVerdict::{Accept, Reject, Defer { awaiting }}
\*   awaited.rs     WaitKey::{Composite, ImmutableField}
\*   deferred.rs    DeferredMerges: defer / release / enqueue_ready / take_ready,
\*                  MAX_DEFERRED_COMPOSITES, REDRIVE_BUDGET; in memory
\*   judge.rs       judge_governed maps the verdict; index_deferred files a Defer
\*   sweep.rs       sweep_unmerged_governed walks the blockstore's unmerged set
\*   merge_handler/dispatch.rs   redrive_deferred drains the ready queue
\*
\* Sibling models of the same machinery from other angles:
\*   PendingDagRestart.tla    -- pending-DAG registrations are process-local
\*                               and die with the process unless persisted
\*   PendingDagQuarantine.tla -- a registered root is re-driven by a sweep;
\*                               a content-determined rejection quarantines it
\*   MergeQueue.tla           -- the merge path itself, fail-closed
\*
\* WHAT IS STRUCTURALLY DIFFERENT FROM GovernanceContract.tla, and therefore
\* worth checking rather than assuming:
\*
\*  1. The defer index is keyed by what the verdict named. AwaitKeys selects
\*     composite CIDs only (the index before WaitKey::ImmutableField) or
\*     CIDs and immutable-field keys (today).
\*  2. Being indexed is what makes a composite re-drivable on arrival. A
\*     Defer that named nothing is NOT indexed (DeferredMerges::defer drops an
\*     empty awaiting), nor is one arriving at capacity.
\*  3. The sweep is the retry clock. SweepScope asks the question this model
\*     exists to answer: does it iterate the INDEX or the UNMERGED composites?
\*     The two differ exactly on composites that could not be indexed.
\*  4. The index is bounded (PendingCap) and in memory: a restart loses it.
\*     IndexDurable = FALSE is today; TRUE is what a persisted index would
\*     buy, kept so the table of pairings in the design note is complete.
\*
\* The verdict function is the plugin and is the same abstraction as in
\* GovernanceContract.tla: Needs/Bad/Self/Refs. That is not duplication: the
\* database calls the plugin, so both layers must see the same function.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Replicas, Entries, Writes, Logs,
  Needs, Self, Refs, Log, AwaitLogs, Bad, Deliverable,
  PendingCap,       \* capacity of the in-memory defer index
  MaxCrashes,       \* bound on crashes
  AwaitKeys,        \* "Cid" (composite CIDs only) | "CidAndField" (today)
  SweepScope,       \* "Indexed" (the defer index) | "Unmerged" (every
                    \* composite pushed and not yet merged or quarantined)
  IndexDurable      \* the defer index survives a crash (not today)

ASSUME AwaitKeys \in {"Cid", "CidAndField"}
ASSUME SweepScope \in {"Indexed", "Unmerged"}
ASSUME IndexDurable \in BOOLEAN
ASSUME PendingCap \in Nat /\ MaxCrashes \in Nat

VARIABLES
  store,        \* [Replicas -> SUBSET Entries]  the blockstore
  pushed,       \* [Replicas -> SUBSET Writes]   composites received over replication
  merged,       \* [Replicas -> SUBSET Writes]   the merged set (durable)
  quarantined,  \* [Replicas -> SUBSET Writes]   quarantine records (durable)
  pending,      \* [Replicas -> [Writes -> SUBSET Entries]]  the defer index (in memory)
  sweepQ,       \* [Replicas -> SUBSET Writes]   released waiters awaiting re-merge
  crashes

dvars == << store, pushed, merged, quarantined, pending, sweepQ, crashes >>

\* An index entry exists iff the composite is filed under at least one key.
\* A composite that named nothing cannot be filed: there is no key for it.
Registered(r) == { w \in Writes : pending[r][w] # {} }
Unmerged(r) == { w \in pushed[r] : w \notin merged[r] /\ w \notin quarantined[r] }

\* ---- The plugin (MergeValidator::validate) ----

Verdict(r, w) ==
  IF w \in Bad THEN "Reject"
  ELSE IF Needs[w] \subseteq store[r] THEN "Accept"
  ELSE "Defer"

\* What the defer can name (MergeVerdict::Defer { awaiting }).
Awaited(r, w) ==
  { e \in Needs[w] \ store[r] :
      \/ e \in Self[w]
      \/ \E f \in store[r] : e \in Refs[f] }

\* judge_governed: map the verdict. Accept merges; Reject quarantines
\* (MergeOutcome::rejected); Defer is Skipped { terminal: false } and is
\* indexed under the keys it named, if it named any and the index has room.
Apply(r, w) ==
  LET v    == Verdict(r, w)
      room == Cardinality(Registered(r)) < PendingCap \/ pending[r][w] # {}
  IN
  /\ merged'      = [merged      EXCEPT ![r] = IF v = "Accept" THEN @ \cup {w} ELSE @]
  /\ quarantined' = [quarantined EXCEPT ![r] = IF v = "Reject" THEN @ \cup {w} ELSE @]
  /\ pending'     = [pending     EXCEPT ![r][w] =
                       IF v # "Defer" THEN {}
                       ELSE IF room   THEN Awaited(r, w)
                                      ELSE {}]

\* ---- Events ----

\* A block arrives and releases its waiters (DeferredMerges::release). A
\* composite CID releases what named it; with field keys an entry of an
\* awaited field key releases too.
Released(r, e) ==
  { w \in Unmerged(r) :
      \/ e \in pending[r][w]
      \/ (AwaitKeys = "CidAndField" /\ Log[e] \in AwaitLogs[w]) }

ReceiveBlock(r, e) ==
  /\ e \in Deliverable
  /\ e \notin store[r]
  /\ store' = [store EXCEPT ![r] = @ \cup {e}]
  /\ sweepQ' = [sweepQ EXCEPT ![r] = @ \cup Released(r, e)]
  /\ UNCHANGED << pushed, merged, quarantined, pending, crashes >>

PushLog(r, w) ==
  /\ w \notin pushed[r]
  /\ pushed' = [pushed EXCEPT ![r] = @ \cup {w}]
  /\ Apply(r, w)
  /\ UNCHANGED << store, sweepQ, crashes >>

\* redrive_deferred: re-merge a released waiter through the verifying path.
DrainReleased(r, w) ==
  /\ w \in sweepQ[r]
  /\ sweepQ' = [sweepQ EXCEPT ![r] = @ \ {w}]
  /\ IF w \in Unmerged(r)
       THEN Apply(r, w)
       ELSE UNCHANGED << merged, quarantined, pending >>
  /\ UNCHANGED << store, pushed, crashes >>

\* The sweep. SweepScope is the question: what does it iterate?
\* "Unmerged" is sweep_unmerged_governed, which walks the blockstore's
\* unmerged set; "Indexed" is a sweep over the defer index, which is the
\* natural refactor and is exactly wrong.
Sweep(r, w) ==
  /\ w \in Unmerged(r)
  /\ (SweepScope = "Unmerged" \/ w \in Registered(r))
  /\ Apply(r, w)
  /\ UNCHANGED << store, pushed, sweepQ, crashes >>

\* A remote re-push of a quarantined composite is judged afresh.
RePush(r, w) ==
  /\ w \in quarantined[r]
  /\ Apply(r, w)
  /\ UNCHANGED << store, pushed, sweepQ, crashes >>

\* The defer index is process-local; a crash empties it. What survives is
\* the blockstore, the merged marks and the quarantine.
Crash(r) ==
  /\ crashes < MaxCrashes
  /\ crashes' = crashes + 1
  /\ pending' = [pending EXCEPT ![r] =
                   IF IndexDurable THEN @ ELSE [w \in Writes |-> {}]]
  /\ sweepQ' = [sweepQ EXCEPT ![r] = IF IndexDurable THEN Unmerged(r) ELSE {}]
  /\ UNCHANGED << store, pushed, merged, quarantined >>

DNext ==
  \/ \E r \in Replicas, e \in Entries : ReceiveBlock(r, e)
  \/ \E r \in Replicas, w \in Writes  : PushLog(r, w)
  \/ \E r \in Replicas, w \in Writes  : DrainReleased(r, w)
  \/ \E r \in Replicas, w \in Writes  : Sweep(r, w)
  \/ \E r \in Replicas, w \in Writes  : RePush(r, w)
  \/ \E r \in Replicas                : Crash(r)

DInit ==
  /\ store = [r \in Replicas |-> {}]
  /\ pushed = [r \in Replicas |-> {}]
  /\ merged = [r \in Replicas |-> {}]
  /\ quarantined = [r \in Replicas |-> {}]
  /\ pending = [r \in Replicas |-> [w \in Writes |-> {}]]
  /\ sweepQ = [r \in Replicas |-> {}]
  /\ crashes = 0

DFairness ==
  /\ \A r \in Replicas, e \in Entries : WF_dvars(ReceiveBlock(r, e))
  /\ \A r \in Replicas, w \in Writes  : WF_dvars(PushLog(r, w))
  /\ \A r \in Replicas, w \in Writes  : WF_dvars(DrainReleased(r, w))
  /\ \A r \in Replicas, w \in Writes  : WF_dvars(Sweep(r, w))

DSpec == DInit /\ [][DNext]_dvars /\ DFairness

\* ---- The refinement mapping ----
\*
\* The contract's abstract state, expressed in this module's concrete state.
\* The merged set and the quarantine collapse to one verdict function; the
\* defer index is the contract's awaiting; the release queue is its re-drive
\* queue.
hFinal == [r \in Replicas |->
             [w \in Writes |->
                IF w \in merged[r] THEN "Accept"
                ELSE IF w \in quarantined[r] THEN "Reject"
                ELSE "None"]]

\* The contract's levers, read off this module's mechanism. RetryClock maps
\* to TRUE unconditionally: there IS a sweep either way. What SweepScope
\* changes is WHICH composites it reaches, and that difference is meant to
\* show up as a liveness failure of the refinement rather than be hidden in
\* a constant. The database has no disposition primitive, so the GC levers
\* map off and nothing is ever forgotten; wasJust is constantly TRUE because
\* the store only grows.
hForgotten == [r \in Replicas |-> {}]
hWasJust == [r \in Replicas |-> [w \in Writes |-> TRUE]]

H == INSTANCE GovernanceContract WITH
       held <- store, arrived <- pushed, final <- hFinal,
       awaiting <- pending, queue <- sweepQ, restarts <- crashes,
       forgotten <- hForgotten, forgets <- 0, wasJust <- hWasJust,
       GC <- FALSE,
       GCFloor <- TRUE,
       MaxForgets <- 0,
       Redrive <- TRUE,
       RetryClock <- TRUE,
       FieldAwait <- (AwaitKeys = "CidAndField"),
       Recovery <- IndexDurable,
       Repush <- TRUE,
       AbsenceReject <- FALSE,
       MaxIndexed <- PendingCap,
       MaxRestarts <- MaxCrashes

\* The claim: every behaviour of the merge path is a behaviour the contract
\* allows. Safety AND liveness: H!Spec carries the contract's fairness, so
\* this fails if the machinery cannot keep the promises a plugin relies on.
REFINES_Contract == H!Spec

\* The contract's obligations, restated over this module's own state, so a
\* failure says which promise broke rather than only that the mapping
\* diverged.
DINV_NoSplit ==
  \A r1, r2 \in Replicas, w \in Writes :
    ~(w \in merged[r1] /\ w \in quarantined[r2])

DINV_RejectIntrinsic ==
  \A r \in Replicas : quarantined[r] \subseteq Bad

DINV_CapRespected ==
  \A r \in Replicas : Cardinality(Registered(r)) <= PendingCap

DSettleable(w) == w \in Bad \/ Needs[w] \subseteq Deliverable

DL1_SettleableSettles ==
  <>[](\A r \in Replicas, w \in Writes :
         DSettleable(w) => (w \in merged[r] \/ w \in quarantined[r]))

====
