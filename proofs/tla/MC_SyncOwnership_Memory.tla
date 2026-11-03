---- MODULE MC_SyncOwnership_Memory ----
\* The story. The receiver keeps every acknowledged head durably (pending,
\* SyncOwnership's durable want queue) but fetches only the roots it holds in
\* a bounded in-memory map (MemCap entries, max_pending_dags). The durable
\* store can hold more roots than the map: after an upgrade or a lower cap,
\* the restart restores only what fits. Some providers never serve, and the
\* receiver also pulls documents itself: a pull-sync merge that yields its
\* work budget leaves a local continuation in the same map.
\*
\* Who acts: the sender (writes and hints), the receiver (registration, the
\* clock's claims, fetches, merges, the resync sweep that admits durable roots
\* into memory, a root giving its entry up), a pull-sync merge and its turns,
\* and a one-shot restart.
\*
\* What must never happen:
\*   - the map holds more than it may (INV_MemoryBounded), or an entry for a
\*     root nobody acknowledged as durable (INV_MemoryHoldsDurable);
\*   - a local continuation is an acknowledged obligation
\*     (INV_ContinuationsNeverAcked): ending one must drop nothing a sender
\*     relies on;
\*   - a continuation whose turn ended without yielding again keeps its entry
\*     (INV_StoppedContinuationReleased);
\* and what must eventually happen:
\*   - every acknowledged durable head whose provider serves is merged, even
\*     when roots whose providers never serve were restored first and fill
\*     the map (LIVE_DurableRootMerged);
\*   - every local continuation leaves the map (LIVE_ContinuationEnds).
\* A local continuation is not promised a merge: no sender waits on it, and a
\* non-terminal skip waits on an input the receiver does not own, so the
\* LIVE_AckedRootMerged promise of MC_SyncOwnership_Dispatch does not extend
\* to it. Its blocks stay local and the next pull or push starts it again.
\*
\* Code it models (crates/p2p/src/sync):
\*   manager/process/pending_dag.rs  try_insert_pending_dag (the map's cap; overflow nacks),
\*     resync_persisted_pending_dags_inner and records_after_resync_cursor (admit durable
\*     roots, resuming after the root admitted last), PendingDagLease::finish_attempt
\*     (a root that spent its dispatches gives its entry up while a durable root waits),
\*     continue_merge (a local continuation), consume_pending_merge_continuation and
\*     MergeTurn (the continuation ends when a turn does not yield again)
\*   manager/pending.rs  PendingOrigin, PENDING_ROTATE_AFTER_DISPATCHES
\*   replication/handlers.rs  handle_block_received, process_merge_batch (hold the turn)
\* Rust tests: tests/unit/durable_overflow_rotation.rs,
\*   replication/continuation_tests.rs a_local_continuation_ends_on_a_non_terminal_skip.
EXTENDS SyncOwnership, Sequences

CONSTANTS
  MemCap,          \* in-memory entries after the restart; before it the map held the whole durable set
  Dead,            \* scopes whose provider never serves a fetch: busy, stalled or gone, for good
  PullScopes,      \* scopes the receiver also pulls, whose merges can leave a local continuation
  OverflowMode,    \* "Rotate" | "KeyOrder": the bug, the sweep restarts at the first key | "Retained": the bug, a root never gives its entry up
  ContinuationMode \* "EndOnStop": a turn that does not yield ends the continuation | "Retained": the bug, it keeps its entry

\* The map holds at least one entry and no more than the durable cap.
ASSUME MemCap \in Nat /\ MemCap >= 1 /\ MemCap <= Cap
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

VARIABLES
  mem,     \* acknowledged roots with an in-memory entry (pushed or restored)
  turns,   \* roots in memory with no fetch running, in the order they became claimable
  spent,   \* roots in memory with an attempt that failed since they were admitted
  cursor,  \* position in KeyOrder of the root the last resync admitted (0: none yet)
  conts,   \* local pull-sync continuations, each holding an in-memory entry
  stopped  \* roots whose last continuation turn ended without yielding again

\* The variables this module adds.
mvars == <<mem, turns, spent, cursor, conts, stopped>>
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
\* Roots whose provider never serves.
DeadRoots == {r \in Roots : r[1] \in Dead}
\* The store's key order, worst case first: the never-serving roots sort first.
KeyOrder == AsSeq(DeadRoots) \o AsSeq(Roots \ DeadRoots)
\* Where a root sits in key order.
Position(r) == CHOOSE i \in 1..Len(KeyOrder) : KeyOrder[i] = r
\* Before the restart the map held the whole durable set; after it, MemCap.
MemBound == IF crashed THEN MemCap ELSE Cap
\* Every in-memory entry: acknowledged roots and local continuations.
Held == mem \cup conts
\* Acknowledged durable roots with no in-memory entry.
Waiting == pending \ mem
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

\* A SyncOwnership step that does not admit anything: memory forgets what the
\* step retired.
M(A) ==
  /\ A
  /\ mem' = mem \cap pending'
  /\ turns' = SelectSeq(turns, LAMBDA r : r \in pending')
  /\ spent' = spent \cap pending'
  /\ UNCHANGED <<cursor, conts, stopped>>

\* Registration needs an in-memory entry first (try_insert_pending_dag): a new
\* root takes one, and a push for a root with a local continuation takes over
\* that continuation's entry.
MRegister(s, v) ==
  /\ ProcessRegister(s, v)
  /\ LET added == pending' \ pending
         retired == pending \ pending'
     IN /\ mem' = (mem \ retired) \cup added
        /\ turns' = Without(turns, retired) \o AsSeq(added)
        /\ conts' = conts \ added
        /\ Cardinality(mem' \cup conts') <= MemBound
        /\ spent' = spent \ retired
        /\ stopped' = stopped \ added
  /\ UNCHANGED cursor

\* With the map full the push is nacked before anything is durable
\* (PendingDagCapacity); the sender keeps its marker and offers it again.
MNack(s, v) ==
  \* An unmerged hint for a root the receiver does not hold yet.
  /\ <<s, v>> \in inflight
  /\ mergedV[s] < v
  /\ <<s, v>> \notin pending \cup conts
  \* No older head of its scope in memory that it would replace.
  /\ ~\E r \in mem : r[1] = s /\ r[2] < v
  \* The map is full.
  /\ Cardinality(Held) >= MemBound
  \* The hint is answered with a nack; the sender's marker stays.
  /\ inflight' = inflight \ {<<s, v>>}
  /\ serveAuth' = serveAuth \ {<<s, v>>}
  /\ UNCHANGED <<localV, dirty, payloadLedger, pending, originBound, qualifiedOffers,
                 qualifiedProvider, serveScopes, flights, drained, ready, mergeFlights,
                 terminalFlights, fetchExhausted, lostCompletions, mergedV, crashed>>
  /\ UNCHANGED mvars

\* The clock claims only a root it holds in memory, and the first of them in
\* turn that it can: the due order of MC_SyncOwnership_Dispatch, without its
\* deadlines.
MClaim(s, v) ==
  /\ <<s, v>> \in Members(turns)
  /\ \A q \in Before(turns, <<s, v>>) : ~CanClaim(q)
  /\ ClaimFetch(s, v)
  /\ turns' = Without(turns, {<<s, v>>})
  /\ UNCHANGED <<mem, spent, cursor, conts, stopped>>

\* An attempt ends without the DAG: it failed, was shed as busy, or deferred
\* behind a local storage owner. Every dispatch counts toward the budget, as
\* `dispatches` does in Rust whatever the attempt's outcome.
MFail(s, v) ==
  /\ FailFetchBeforeWaiter(s, v) \/ ProviderShed(s, v) \/ ContendedIngest(s, v)
  /\ spent' = spent \cup ({<<s, v>>} \cap mem)
  /\ turns' = IF <<s, v>> \in mem THEN Append(turns, <<s, v>>) ELSE turns
  /\ UNCHANGED <<mem, cursor, conts, stopped>>

\* A completed fetch, which a provider that never serves never gives.
MComplete(s, v) ==
  /\ s \notin Dead
  /\ CompleteFetch(s, v)
  /\ turns' = IF <<s, v>> \in mem THEN Append(turns, <<s, v>>) ELSE turns
  /\ UNCHANGED <<mem, spent, cursor, conts, stopped>>

\* At the end of an attempt a root that spent its dispatches gives its entry
\* up, keeping its durable record, when the map is full and a durable root
\* has no entry (PendingDagLease::finish_attempt). The Rust budget of
\* PENDING_ROTATE_AFTER_DISPATCHES dispatches is one failed attempt here.
Yield(s, v) ==
  \* Retained, the rule before, never gives an acknowledged entry up.
  /\ OverflowMode # "Retained"
  \* The root holds an entry, spent its dispatches and has no attempt running.
  /\ <<s, v>> \in mem \cap spent
  /\ flights[<<s, v>>] = 0
  \* Another durable root waits for an entry, and the map is full.
  /\ Waiting # {}
  /\ Cardinality(Held) >= MemBound
  \* The entry goes; the durable record (pending) stays.
  /\ mem' = mem \ {<<s, v>>}
  /\ turns' = Without(turns, {<<s, v>>})
  /\ spent' = spent \ {<<s, v>>}
  /\ UNCHANGED <<vars, cursor, conts, stopped>>

\* The resync sweep admits the next waiting durable root in key order: after
\* the root it admitted last (Rotate), or from the first key (KeyOrder and
\* Retained), which re-admits the roots that just gave their entries up.
Resync ==
  /\ Waiting # {}
  /\ Cardinality(Held) < MemBound
  /\ LET r == CHOOSE q \in Waiting : \A p \in Waiting : Rank(q) <= Rank(p)
     IN /\ mem' = mem \cup {r}
        /\ turns' = Append(turns, r)
        /\ spent' = spent \ {r}
        /\ cursor' = IF OverflowMode = "Rotate" THEN Position(r) ELSE cursor
  /\ UNCHANGED <<vars, conts, stopped>>

\* A pull-sync merge of a head nobody pushed yields its work budget and takes
\* an in-memory entry for its next turn (continue_merge).
PullYield(s, v) ==
  /\ s \in PullScopes
  /\ <<s, v>> \notin pending \cup conts
  /\ mergedV[s] < v
  /\ Cardinality(Held) < MemBound
  /\ conts' = conts \cup {<<s, v>>}
  /\ stopped' = stopped \ {<<s, v>>}
  /\ UNCHANGED <<vars, mem, turns, spent, cursor>>

\* A continuation's next turn ends without yielding again: it merged, or it
\* skipped or failed with no terminal outcome. EndOnStop removes the entry
\* when the turn ends (MergeTurn); Retained keeps it, and the clock re-drives
\* it, turn after turn.
ContinuationStop(s, v) ==
  /\ <<s, v>> \in conts
  /\ conts' = IF ContinuationMode = "EndOnStop" THEN conts \ {<<s, v>>} ELSE conts
  /\ stopped' = stopped \cup {<<s, v>>}
  /\ UNCHANGED <<vars, mem, turns, spent, cursor>>

\* The restart: memory, the cursor and every continuation are gone; the
\* durable roots stay, and from now on the map holds MemCap entries.
MCrash ==
  /\ Crash
  /\ mem' = {}
  /\ turns' = <<>>
  /\ spent' = {}
  /\ cursor' = 0
  /\ conts' = {}
  /\ stopped' = {}

\* Nothing in memory, no continuation, no sweep yet.
MInit ==
  /\ Init
  /\ mem = {}
  /\ turns = <<>>
  /\ spent = {}
  /\ cursor = 0
  /\ conts = {}
  /\ stopped = {}

\* Every SyncOwnership action, with registration, nack, claim, failure,
\* completion and restart in their memory-aware forms, plus the sweep, the
\* yield and the continuations.
MNext ==
  \/ \E s \in Scopes : M(Update(s)) \/ M(ReHintMarker(s))
  \/ \E s \in Scopes, v \in 1..MaxV :
       \/ M(ReHintPayload(s, v)) \/ M(DropHint(s, v)) \/ M(ProcessMerged(s, v))
       \/ MRegister(s, v) \/ M(ProcessNack(s, v)) \/ MNack(s, v)
       \/ M(SameRootReannounce(s, v))
       \/ M(OfferQualifiedAlternate(s, v)) \/ M(AddQualifiedAlternate(s, v))
       \/ MClaim(s, v) \/ MFail(s, v)
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

\* Liveness runs: only the receiver is fair, and only at the scheduler, as in
\* MC_SyncOwnership_Dispatch: the clock claims whenever it can (in turn), the
\* sweep runs, a spent root gives its entry up when one keeps being able to,
\* merges run, and a continuation's next turn comes. Per root there is only a
\* timer: an attempt's windows end. Pull-sync merges, failures, nacks, the
\* cleanup after a merge and the restart are never forced: neither promise
\* needs them. The environment assumption is the dispatch model's: a fetch
\* that can complete eventually does (strong fairness on some MComplete).
MFairSpec ==
  /\ MSpec
  /\ SF_allVars(Resync)
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : MClaim(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : MComplete(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : Yield(s, v))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : M(ClaimMerge(s, v)))
  /\ SF_allVars(\E s \in Scopes, v \in 1..MaxV : M(CompleteMerge(s, v)))
  /\ WF_allVars(\E s \in Scopes, v \in 1..MaxV : ContinuationStop(s, v))
  /\ \A s \in Scopes : WF_allVars(\E v \in 1..MaxV : MFail(s, v))

\* Promise: the added variables keep their shapes, on top of SyncOwnership's.
INV_MemoryTypeOK ==
  /\ TypeOK
  /\ mem \subseteq Roots
  /\ turns \in Seq(mem)
  /\ spent \subseteq mem
  /\ cursor \in 0..Len(KeyOrder)
  /\ conts \subseteq Roots
  /\ stopped \subseteq Roots

\* Promise: the map never holds more entries than it may.
INV_MemoryBounded == Cardinality(Held) <= MemBound

\* Promise: an acknowledged entry in memory always has its durable record.
INV_MemoryHoldsDurable == mem \subseteq pending

\* Promise: a local continuation is never an acknowledged obligation, so
\* ending one drops nothing a sender relies on.
INV_ContinuationsNeverAcked == conts \cap pending = {}

\* Promise: a continuation whose turn ended without yielding holds no entry.
INV_StoppedContinuationReleased == stopped \cap conts = {}

\* Promise: every acknowledged durable head whose provider serves is merged,
\* however many never-serving roots were restored ahead of it.
LIVE_DurableRootMerged ==
  \A r \in Roots : r[1] \notin Dead => ((r \in pending) ~> (mergedV[r[1]] >= r[2]))

\* Promise: every local continuation leaves the map.
LIVE_ContinuationEnds == \A r \in Roots : (r \in conts) ~> (r \notin conts)
====
