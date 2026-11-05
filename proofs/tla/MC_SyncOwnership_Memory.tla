---- MODULE MC_SyncOwnership_Memory ----
\* The story. The receiver keeps every acknowledged head durably (pending,
\* SyncOwnership's durable want queue) but fetches only the roots it holds in
\* a bounded in-memory map (MemCap entries, max_pending_dags). The durable
\* store can hold more roots than the map: after an upgrade or a lower cap,
\* the restart restores only what fits. Some providers never serve, and the
\* receiver also pulls documents itself: a pull-sync merge that yields its
\* work budget leaves a local continuation in the same map.
\*
\* Admission has two tiers. A new head takes an entry and a durable record;
\* when the map is full it takes the durable record alone and is
\* acknowledged, and waits for the sweep like any durable root past the map.
\* Each sending peer holds at most DurableQuota durable roots: a head that
\* would take its peer past it is nacked, and the sender keeps its marker.
\*
\* Who acts: the senders (writes, hints and the retry of a nacked marker),
\* the receiver (its answer to a hint, the clock's claims, fetches, merges,
\* the resync sweep that admits durable roots into memory, a root giving its
\* entry up), a pull-sync merge and its turns, the wall clock (a root's retry
\* deadline passes), and a one-shot restart.
\*
\* What must never happen:
\*   - the map holds more than it may (INV_MemoryBounded), or an acknowledged
\*     entry without its durable record (INV_MemoryHoldsDurable);
\*   - a peer holds more durable roots than its quota (INV_DurablePeerQuota);
\*   - a local continuation is an acknowledged obligation
\*     (INV_ContinuationsNeverAcked): ending one must drop nothing a sender
\*     relies on;
\* and what must eventually happen:
\*   - every acknowledged durable head whose provider serves is merged, even
\*     when roots whose providers never serve were restored first and fill
\*     the map (LIVE_DurableRootMerged);
\*   - every head a sender writes, whose provider serves and whose peer is
\*     not held at its quota by never-serving roots, is merged: a full map
\*     does not leave it nacked for ever, with the sender holding it
\*     (LIVE_WrittenHeadMerged);
\*   - every local continuation leaves the map (LIVE_ContinuationEnds): a turn
\*     that does not yield again ends it.
\* A local continuation is not promised a merge: no sender waits on it, and a
\* non-terminal skip waits on an input the receiver does not own, so the
\* LIVE_AckedRootMerged promise of MC_SyncOwnership_Dispatch does not extend
\* to it. Its blocks stay local and the next pull or push starts it again.
\*
\* Every variable this module adds is state the code holds, and the
\* properties are stated over it; nothing is recorded only to state them.
\* Variables and the code state each one is (crates/p2p/src/sync):
\*   mem        the keys of the in-memory pending map (SyncManager::pending_dags,
\*              manager/process/mod.rs:90) whose PendingDag::origin (manager/pending.rs:93)
\*              is Push or Restored.
\*   conts      the keys of the same map whose origin is LocalContinuation.
\*   dispatches PendingDag::dispatches (manager/pending.rs:83): claims since the entry
\*              was made, counted by try_claim_pending_dag_dispatch
\*              (manager/process/pending_dag.rs:563), up to RotateAfter, the
\*              PENDING_ROTATE_AFTER_DISPATCHES (manager/pending.rs:353) that
\*              PendingDagLease::finish_attempt (pending_dag.rs:81) compares it with.
\*   dueOrder   the entries in mem whose PendingDag::next_retry_at (manager/pending.rs:81)
\*              has passed, in next_retry_at order (due_pending_dag_retries,
\*              pending_dag.rs:723); next_retry_at is kept by its order, the only
\*              thing the code reads from it.
\*   backoff    the entries in mem whose next_retry_at is still ahead. Which passes
\*              next is left open (BecomeDue), so every order of deadlines is explored.
\*   cursor     SyncManager::pending_resync_cursor (manager/process/mod.rs): the root
\*              the last sweep admitted, as its place in key order (0: none).
\* SyncOwnership's pending is the durable record set (PersistedView,
\* pending_store/view.rs), and the count of a peer's roots in it is the count the
\* store's registration reads and writes in its own transaction (pending_store/shares.rs),
\* by the record's source_peer (PeerScopes; a record written before shares holds no charge,
\* and the model has none), so a registration's quota check and its record are one step
\* here as in the code (PushLogAdmission's Red_ViewShare checks the quota against the
\* lagging view instead); flights the fetch reservation
\* (SyncShutdownHandle's pending_dag_fetch_tasks, coordinator/mod.rs), and crashed
\* the restart.
\*
\* Code it models (crates/p2p/src/sync):
\*   manager/process/pushlog.rs  process_block_inner (the two tiers, the share check and the nack)
\*   pending_store/store.rs  register and take_new (the quota, checked and charged with the record)
\*   manager/process/pending_dag.rs  try_insert_pending_dag (the map's cap),
\*     durable_share_admits, admits_durable_only, max_persisted_pending_per_peer (the quota),
\*     restored_root_fits (the sweep admits only what the map has a place for),
\*     resync_persisted_pending_dags_inner and records_after_resync_cursor (admit durable
\*     roots, resuming after the root admitted last), PendingDagLease::finish_attempt
\*     (a root that spent its dispatches gives its entry up while a durable root waits),
\*     continue_merge (a local continuation), consume_pending_merge_continuation and
\*     MergeTurn (the continuation ends when a turn does not yield again)
\*   manager/pending.rs  PendingOrigin, PENDING_ROTATE_AFTER_DISPATCHES
\*   pending_store/shares.rs  held, charge, hand_over, give_back (each peer's durable roots)
\*   replication/handlers.rs  handle_block_received, process_merge_batch (hold the turn)
\* Rust tests: tests/unit/durable_overflow_rotation.rs (both tests), tests/unit/durable_share.rs
\*   (both tests), tests/unit/resync_waiting_root.rs, pending_store/tests.rs
\*   a_registration_beside_another_of_its_peer_is_refused_at_the_share,
\*   replication/continuation_tests.rs a_local_continuation_ends_on_a_non_terminal_skip.
EXTENDS SyncOwnership, Sequences

CONSTANTS
  MemCap,          \* in-memory entries after the restart; before it the map held the whole durable set
  RotateAfter,     \* dispatches after which a root may give its entry up (PENDING_ROTATE_AFTER_DISPATCHES)
  Dead,            \* scopes whose provider never serves a fetch: busy, stalled or gone, for good
  PullScopes,      \* scopes the receiver also pulls, whose merges can leave a local continuation
  OverflowMode,    \* "Rotate" | "KeyOrder": the bug, the sweep restarts at the first key | "Retained": the bug, a root never gives its entry up
  ContinuationMode, \* "EndOnStop": a turn that does not yield ends the continuation | "Retained": the bug, it keeps its entry
  AdmissionMode,   \* "TwoTier": a full map admits on the durable record alone | "MemoryOnly": the bug, a full map nacks
  PeerScopes,      \* the sending peers, each the set of scopes it pushes (PersistedPendingDag::source_peer)
  DurableQuota     \* durable roots one peer may hold (max_persisted_pending_per_peer); Cap removes the quota

\* The map holds at least one entry and no more than the durable cap.
ASSUME MemCap \in Nat /\ MemCap >= 1 /\ MemCap <= Cap
\* A root gives its entry up only after at least one dispatch.
ASSUME RotateAfter \in Nat /\ RotateAfter >= 1
\* Never-serving providers belong to some of the scopes.
ASSUME Dead \subseteq Scopes
\* The receiver pulls some of the scopes.
ASSUME PullScopes \subseteq Scopes
\* The overflow knob takes one of its named values.
ASSUME OverflowMode \in {"Rotate", "KeyOrder", "Retained"}
\* The continuation knob takes one of its named values.
ASSUME ContinuationMode \in {"EndOnStop", "Retained"}
\* Acknowledged roots are durable: rotation keeps the record, never the entry alone.
ASSUME RegisterMode = "Durable"
\* The admission knob takes one of its named values.
ASSUME AdmissionMode \in {"TwoTier", "MemoryOnly"}
\* Every scope is pushed by exactly one peer.
ASSUME /\ UNION PeerScopes = Scopes
       /\ \A P, Q \in PeerScopes : P # Q => P \cap Q = {}
\* A peer may hold at least one durable root and at most the whole store.
ASSUME DurableQuota \in 1..Cap

VARIABLES
  mem,        \* entries of acknowledged roots (origin Push or Restored)
  conts,      \* entries of local pull-sync continuations (origin LocalContinuation)
  dispatches, \* [Roots -> 0..RotateAfter], claims since the entry was made
  dueOrder,   \* entries in mem whose next_retry_at has passed, in next_retry_at order
  backoff,    \* entries in mem whose next_retry_at is still ahead
  cursor      \* the root the last resync admitted, as its place in key order (0: none)

\* The variables this module adds.
mvars == <<mem, conts, dispatches, dueOrder, backoff, cursor>>
\* Every variable, SyncOwnership's and this module's.
allVars == <<vars, mvars>>

\* The roots a sequence holds.
Members(q) == {q[i] : i \in 1..Len(q)}
\* A sequence with the roots of S left out, the rest in order.
Without(q, S) == SelectSeq(q, LAMBDA r : r \notin S)
\* The roots ahead of r in q.
Before(q, r) == {q[i] : i \in 1..((CHOOSE k \in 1..Len(q) : q[k] = r) - 1)}
\* The roots of a finite set as a sequence, in one fixed order.
AsSeq(S) == CHOOSE f \in [1..Cardinality(S) -> S] :
              \A i, j \in 1..Cardinality(S) : i # j => f[i] # f[j]
\* The smaller of two numbers.
Min(a, b) == IF a < b THEN a ELSE b
\* Roots whose provider never serves.
DeadRoots == {r \in Roots : r[1] \in Dead}
\* The store's key order, worst case first: the never-serving roots sort first.
KeyOrder == AsSeq(DeadRoots) \o AsSeq(Roots \ DeadRoots)
\* Where a root sits in key order.
Position(r) == CHOOSE i \in 1..Len(KeyOrder) : KeyOrder[i] = r
\* The scopes of the peer that pushes scope s.
PeerOf(s) == CHOOSE P \in PeerScopes : s \in P
\* Before the restart the map held the whole durable set; after it, MemCap.
MemBound == IF crashed THEN MemCap ELSE Cap
\* Every in-memory entry: acknowledged roots and local continuations.
Held == mem \cup conts
\* Acknowledged durable roots with no in-memory entry.
Waiting == pending \ mem
\* The map has no place for one more entry.
MemoryFull == Cardinality(Held) >= MemBound
\* The scope's current head, which the head links, holds an entry the head
\* would take over.
ReplacesInMemory(s, v) == Supersedes(s, v) \cap mem # {}
\* The durable roots of scope s that head v retires (ScopeCurrent): the
\* scope's current head, only when v links it, as ProcessRegister retires.
Retires(s, v) == IF PendingMode = "ScopeCurrent" THEN Supersedes(s, v) ELSE {}
\* The head needs a durable record of its own: it is not registered yet.
NewRecord(s, v) == <<s, v>> \notin pending
\* The head's peer stays within its quota once the head is registered: what
\* it retires gives its record up (take_new, in the registration's transaction).
DurableRoom(s, v) ==
  Cardinality({r \in pending \ Retires(s, v) : r[1] \in PeerOf(s)}) < DurableQuota
\* Where the sweep starts: after the cursor when it rotates, at the first key otherwise.
Start == IF OverflowMode = "Rotate" THEN cursor ELSE 0
\* How far after the start a root comes, wrapping around.
Rank(r) == IF Position(r) > Start THEN Position(r) - Start
           ELSE Position(r) + Len(KeyOrder) - Start
\* A root the clock could hand the slot to: servable, with a provider, and no
\* fetch running.
CanClaim(r) ==
  /\ r \in pending
  /\ (r \in serveAuth \/ r[1] \in serveScopes)
  /\ HasQualifiedProvider(r)
  /\ flights[r] = 0
\* The entry fields of the roots in S, as a new entry has them: no dispatches.
Reset(S) == [r \in Roots |-> IF r \in S THEN 0 ELSE dispatches[r]]

\* A SyncOwnership step that does not admit anything: memory forgets what the
\* step retired.
M(A) ==
  /\ A
  /\ mem' = mem \cap pending'
  /\ dueOrder' = SelectSeq(dueOrder, LAMBDA r : r \in pending')
  /\ backoff' = backoff \cap pending'
  /\ dispatches' = Reset(mem \ pending')
  /\ UNCHANGED <<conts, cursor>>

\* The first tier: the head takes an in-memory entry and its durable record
\* (try_insert_pending_dag). A new root takes an entry, due at once, and a
\* push for a root with a local continuation takes over that continuation's
\* entry. A head that needs a record of its own needs room in its peer's
\* quota first.
MRegister(s, v) ==
  /\ NewRecord(s, v) => DurableRoom(s, v)
  /\ ProcessRegister(s, v)
  /\ LET added == pending' \ pending
         retired == pending \ pending'
     IN /\ mem' = (mem \ retired) \cup added
        /\ conts' = conts \ added
        /\ Cardinality(mem' \cup conts') <= MemBound
        /\ dueOrder' = Without(dueOrder, retired) \o AsSeq(added)
        /\ backoff' = backoff \ retired
        /\ dispatches' = Reset(retired \cup added)
  /\ UNCHANGED cursor

\* The second tier: the map is full, so the head takes its durable record
\* alone and is acknowledged; it waits for the sweep like any durable root
\* without an entry (resident false in process_block_inner).
MRegisterDurable(s, v) ==
  \* MemoryOnly, the rule before, has no second tier.
  /\ AdmissionMode = "TwoTier"
  \* A head that needs a record of its own, with no entry it could take over.
  /\ <<s, v>> \notin pending \cup conts
  /\ NewRecord(s, v)
  /\ ~ReplacesInMemory(s, v)
  \* The map is full, and the peer has room in its quota.
  /\ MemoryFull
  /\ DurableRoom(s, v)
  \* The durable registration and its acknowledgement (ProcessRegister keeps Cap),
  \* one step: the ack rests on the record the store committed
  \* (Processed::DurablyRegistered), not on the view, which may learn of it later.
  /\ ProcessRegister(s, v)
  \* No entry is taken; what the head retires leaves memory too.
  /\ LET retired == pending \ pending'
     IN /\ mem' = mem \ retired
        /\ dueOrder' = Without(dueOrder, retired)
        /\ backoff' = backoff \ retired
        /\ dispatches' = Reset(retired)
  /\ UNCHANGED <<conts, cursor>>

\* The nack (PendingDagCapacity): the sender keeps its marker and offers the
\* head again. TwoTier nacks only a head its peer's quota has no room for;
\* MemoryOnly also nacks every head the full map has no entry for.
MNack(s, v) ==
  \* An unmerged hint for a root the receiver does not hold yet.
  /\ <<s, v>> \in inflight
  /\ mergedV[s] < v
  /\ <<s, v>> \notin pending \cup conts
  /\ NewRecord(s, v)
  \* Its peer is at its quota, or (MemoryOnly) the map is full and the scope
  \* head it links holds no entry it would take over.
  /\ \/ ~DurableRoom(s, v)
     \/ /\ AdmissionMode = "MemoryOnly"
        /\ ~ReplacesInMemory(s, v)
        /\ MemoryFull
  \* The hint is answered with a nack; the sender's marker stays.
  /\ inflight' = inflight \ {<<s, v>>}
  /\ serveAuth' = serveAuth \ {<<s, v>>}
  /\ UNCHANGED <<localV, dirty, payloadLedger, pending, originBound, qualifiedOffers,
                 qualifiedProvider, serveScopes, flights, drained, ready, mergeFlights,
                 terminalFlights, fetchExhausted, lostCompletions, mergedV, crashed,
                 fwdVars>>
  /\ UNCHANGED senderVars
  /\ UNCHANGED mvars

\* The clock claims the oldest due entry that can take the slot
\* (dispatch_due_pending_dag_fetches): its next deadline is armed and its
\* dispatches counted (try_claim_pending_dag_dispatch).
MClaim(s, v) ==
  /\ <<s, v>> \in Members(dueOrder)
  /\ \A q \in Before(dueOrder, <<s, v>>) : ~CanClaim(q)
  /\ ClaimFetch(s, v)
  /\ dueOrder' = Without(dueOrder, {<<s, v>>})
  /\ backoff' = backoff \cup {<<s, v>>}
  /\ dispatches' = [dispatches EXCEPT ![<<s, v>>] = Min(@ + 1, RotateAfter)]
  /\ UNCHANGED <<mem, conts, cursor>>

\* An attempt ends without the DAG: it failed, was shed as busy, or deferred
\* behind a local storage owner. The root waits out its backoff.
MFail(s, v) ==
  /\ FailFetchBeforeWaiter(s, v) \/ ProviderShed(s, v) \/ ContendedIngest(s, v)
  /\ UNCHANGED mvars

\* A completed fetch, which a provider that never serves never gives.
MComplete(s, v) ==
  /\ s \notin Dead
  /\ CompleteFetch(s, v)
  /\ UNCHANGED mvars

\* The root's next_retry_at passes; its attempt has ended (the deadline runs
\* from the end of the attempt, PendingDagLease::finish_attempt).
BecomeDue(s, v) ==
  /\ <<s, v>> \in backoff
  /\ flights[<<s, v>>] = 0
  /\ backoff' = backoff \ {<<s, v>>}
  /\ dueOrder' = Append(dueOrder, <<s, v>>)
  /\ UNCHANGED <<vars, mem, conts, dispatches, cursor>>

\* At the end of an attempt a root that had RotateAfter dispatches gives its
\* entry up, keeping its durable record, when the map is full and a durable
\* root has no entry (PendingDagLease::finish_attempt).
Yield(s, v) ==
  \* Retained, the rule before, never gives an acknowledged entry up.
  /\ OverflowMode # "Retained"
  \* The root holds an entry, had its dispatches, and its attempt has ended.
  /\ <<s, v>> \in mem
  /\ dispatches[<<s, v>>] >= RotateAfter
  /\ flights[<<s, v>>] = 0
  \* Another durable root waits for an entry, and the map is full.
  /\ Waiting # {}
  /\ Cardinality(Held) >= MemBound
  \* The entry goes; the durable record (pending) stays.
  /\ mem' = mem \ {<<s, v>>}
  /\ dueOrder' = Without(dueOrder, {<<s, v>>})
  /\ backoff' = backoff \ {<<s, v>>}
  /\ dispatches' = Reset({<<s, v>>})
  /\ UNCHANGED <<vars, conts, cursor>>

\* The resync sweep admits the next waiting durable root in key order, due at
\* once: after the root it admitted last (Rotate), or from the first key
\* (KeyOrder and Retained), which re-admits the roots that just gave their
\* entries up.
Resync ==
  /\ Waiting # {}
  /\ Cardinality(Held) < MemBound
  /\ LET r == CHOOSE q \in Waiting : \A p \in Waiting : Rank(q) <= Rank(p)
     IN /\ mem' = mem \cup {r}
        /\ dueOrder' = Append(dueOrder, r)
        /\ dispatches' = Reset({r})
        /\ cursor' = IF OverflowMode = "Rotate" THEN Position(r) ELSE cursor
  /\ UNCHANGED <<vars, conts, backoff>>

\* A pull-sync merge of a head nobody pushed yields its work budget and takes
\* an in-memory entry for its next turn (continue_merge).
PullYield(s, v) ==
  /\ s \in PullScopes
  /\ <<s, v>> \notin pending \cup conts
  /\ mergedV[s] < v
  /\ Cardinality(Held) < MemBound
  /\ conts' = conts \cup {<<s, v>>}
  /\ UNCHANGED <<vars, mem, dispatches, dueOrder, backoff, cursor>>

\* A continuation's next turn ends without yielding again: it merged, or it
\* skipped or failed with no terminal outcome. EndOnStop removes the entry
\* when the turn ends (MergeTurn); Retained, the rule before, leaves it, and
\* the clock re-drives it, turn after turn.
ContinuationStop(s, v) ==
  /\ <<s, v>> \in conts
  /\ conts' = IF ContinuationMode = "EndOnStop" THEN conts \ {<<s, v>>} ELSE conts
  /\ UNCHANGED <<vars, mem, dispatches, dueOrder, backoff, cursor>>

\* The restart: memory, the cursor and every continuation are gone; the
\* durable roots stay, and from now on the map holds MemCap entries.
MCrash ==
  /\ Crash
  /\ mem' = {}
  /\ conts' = {}
  /\ dispatches' = [r \in Roots |-> 0]
  /\ dueOrder' = <<>>
  /\ backoff' = {}
  /\ cursor' = 0

\* Nothing in memory, no continuation, no sweep yet.
MInit ==
  /\ Init
  /\ mem = {}
  /\ conts = {}
  /\ dispatches = [r \in Roots |-> 0]
  /\ dueOrder = <<>>
  /\ backoff = {}
  /\ cursor = 0

\* The receiver's answer to a hint from scope s: acknowledged as merged,
\* registered in either tier, or nacked.
Answer(s, v) ==
  \/ M(ProcessMerged(s, v)) \/ MRegister(s, v) \/ MRegisterDurable(s, v)
  \/ MNack(s, v) \/ M(ProcessNack(s, v))

\* Every SyncOwnership action, with registration, nack, claim, failure,
\* completion and restart in their memory-aware forms, plus the second tier,
\* the deadline, the sweep, the yield and the continuations.
MNext ==
  \/ \E s \in Scopes : M(Update(s)) \/ M(ReHintMarker(s))
  \/ \E s \in Scopes, v \in 1..MaxV :
       \/ M(ReHintPayload(s, v)) \/ M(DropHint(s, v)) \/ M(ProcessMerged(s, v))
       \/ MRegister(s, v) \/ MRegisterDurable(s, v) \/ M(ProcessNack(s, v)) \/ MNack(s, v)
       \/ M(SameRootReannounce(s, v))
       \/ M(OfferQualifiedAlternate(s, v)) \/ M(AddQualifiedAlternate(s, v))
       \/ MClaim(s, v) \/ MFail(s, v) \/ BecomeDue(s, v)
       \/ M(StarveTransportCompletion(s, v)) \/ M(StarveRecoveryServe(s, v))
       \/ M(StarveRecoveryAuthorization(s, v))
       \/ MComplete(s, v) \/ M(ClaimMerge(s, v))
       \/ M(CompleteMerge(s, v)) \/ M(ClaimTerminal(s, v))
       \/ M(CompleteTerminal(s, v))
       \/ Yield(s, v) \/ PullYield(s, v) \/ ContinuationStop(s, v)
  \/ Resync
  \/ MCrash
  \/ M(Terminating)

\* Safety runs.
MSpec == MInit /\ [][MNext]_allVars

\* Liveness runs: fairness is the schedulers' only, as in
\* MC_SyncOwnership_Dispatch: the clock claims whenever it can (in due order),
\* the sweep runs, a root that may give its entry up eventually does when it
\* keeps being able to, merges run, and a continuation's next turn comes.
\* Each sending peer's retry ledger offers one of its dirty markers again,
\* and the receiver answers one of that peer's hints: one scheduler per peer,
\* never per scope, so a peer may keep offering a head that is nacked for
\* ever instead of another. Per root there are only timers: a deadline
\* passes, an attempt's windows end. Pull-sync merges, failures, the cleanup
\* after a merge and the restart are never forced. The environment
\* assumption is the dispatch model's: a fetch that can complete eventually
\* does (strong fairness on some MComplete).
MFairSpec ==
  /\ MSpec
  /\ \A P \in PeerScopes : WF_allVars(\E s \in P : M(ReHintMarker(s)))
  /\ \A P \in PeerScopes : SF_allVars(\E s \in P, v \in 1..MaxV : Answer(s, v))
  /\ SF_allVars(Resync)
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : MClaim(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : MComplete(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : Yield(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : M(ClaimMerge(s, v)))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : M(CompleteMerge(s, v)))
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : ContinuationStop(s, v))
  /\ \A s \in Scopes :
       /\ WF_allVars(\E v \in 1..MaxV : MFail(s, v))
       /\ WF_allVars(\E v \in 1..MaxV : BecomeDue(s, v))

\* Promise: the added variables keep their shapes, on top of SyncOwnership's.
INV_MemoryTypeOK ==
  /\ TypeOK
  /\ mem \subseteq Roots
  /\ conts \subseteq Roots
  /\ dispatches \in [Roots -> 0..RotateAfter]
  /\ dueOrder \in Seq(mem)
  /\ backoff \subseteq mem
  /\ cursor \in 0..Len(KeyOrder)

\* Promise: an entry with no fetch running is either due or waiting out its
\* backoff, never both and never neither, so no entry is lost to the clock.
INV_DueOrBackoff ==
  \A r \in mem : flights[r] = 0 => ((r \in backoff) # (r \in Members(dueOrder)))

\* Promise: the map never holds more entries than it may.
INV_MemoryBounded == Cardinality(Held) <= MemBound

\* Promise: an acknowledged entry in memory always has its durable record.
INV_MemoryHoldsDurable == mem \subseteq pending

\* Promise: no peer holds more durable roots than its quota.
INV_DurablePeerQuota ==
  \A P \in PeerScopes : Cardinality({r \in pending : r[1] \in P}) <= DurableQuota

\* Promise: a local continuation is never an acknowledged obligation, so
\* ending one drops nothing a sender relies on.
INV_ContinuationsNeverAcked == conts \cap pending = {}

\* Promise: every acknowledged durable head whose provider serves is merged,
\* however many never-serving roots were restored ahead of it.
LIVE_DurableRootMerged ==
  \A r \in Roots : r[1] \notin Dead => ((r \in pending) ~> (mergedV[r[1]] >= r[2]))

\* Promise: every local continuation leaves the map.
LIVE_ContinuationEnds == \A r \in Roots : (r \in conts) ~> (r \notin conts)

\* Promise: every head a sender writes on a scope whose provider serves is
\* merged, when no never-serving root of the same peer could hold its quota.
\* A head nacked for ever stays the sender's: this fails when a full map
\* nacks it while nothing gives an entry up.
LIVE_WrittenHeadMerged ==
  \A s \in Scopes :
    (PeerOf(s) \cap Dead = {}) =>
      \A v \in 1..MaxV : (localV[s] >= v) ~> (mergedV[s] >= v)
====
