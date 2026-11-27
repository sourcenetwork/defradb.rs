---- MODULE IndexCachePublication ----
\* The story: a node keeps a copy of each collection's definition in memory,
\* the collection cache, and says there for every index whether it is still
\* being built, built, or failed. The query planner reads only that copy: it
\* uses an index the cache calls built and scans the collection otherwise.
\* What is on disk (the store) is the truth; the cache must follow it.
\*
\* Who acts:
\*   - two indexes, a and b, each created by someone: the create commits the
\*     index into the definition with a "building" record, publishes the new
\*     definition to the cache and returns; then its backfill settles, in one
\*     commit, as built (record cleared) or as failed (record errored), and
\*     publishes that to the cache too;
\*   - writers, whose commit hooks hold back the cache publications of every
\*     later commit until they finish (one hold, released at some point);
\*   - something else on the node that publishes an unrelated cache change
\*     (another collection's schema, say) once, at any time.
\*
\* Cache publications run in commit order, after the commit. A publication
\* carries its transaction's definition and applies only the index status
\* that transaction changed; every other index keeps the status already in
\* the cache at that point in commit order.
\* A create reserves its index ID before its transaction, so two creates name
\* two indexes even when their snapshots both predate either definition.
\*
\* What must never happen:
\*   - a create returns while the cache does not have its index yet, so the
\*     caller's next request (a drop, a second create, the backfill itself)
\*     works from a definition older than the one it just committed;
\*   - once everyone has finished, the cache says something about an index
\*     that the store does not: above all, the store has the index built
\*     and the cache still calls it building, so it is never used.
\*
\* Code it models (crates/database/db/src/index):
\*   create.rs    DB::define_index,
\*                DB::stage_index_definition (the ID, definition and action
\*                record commit together, and
\*                the create waits for their cache publication:
\*                OnCommit::OrderedAndWait)
\*                DB::publish_index_definition (only a committed index
\*                claims a backfill, so retries never compete for the claim)
\*   manager/mod.rs IndexManager::next_index_id (uses the same floor as
\*                a drop; the definition read is validated by the commit)
\*   drop.rs      DB::stage_index_drop (the same publication for a drop)
\*   settle.rs    FinishIndexAttempt (the build and the cleared record in one
\*                commit, which publishes the built index), FailBackfillAttempt
\*                (the errored record, published the same way), publish_on_commit
\*   backfill.rs  DB::backfill_index (no cache reload after settling)
\*   crates/database/db/src/collection/index_status.rs
\*                IndexStatuses::load_for (the snapshot that supplies indexes
\*                the cache has not seen)
\*   crates/database/db/src/collection/mod.rs
\*                Collection::load_index_actions, with_current_index_actions
\*   crates/database/db/src/collection/epoch.rs
\*                EpochedCollections::publish, publish_statuses, rcu_if_epoch
\*                (per-index changes; a settlement never restores a stale
\*                definition; a reload another publication overtakes is dropped)
\*   crates/engine/storage/src/corekv/callbacks.rs OnCommit::Ordered,
\*                OnCommit::OrderedAndWait
EXTENDS Naturals, Sequences

CONSTANTS
  PublishMode, \* "Wait": a create returns after its cache publication ran | "Queue": right after its commit
  SettleMode,  \* "Commit": a backfill's settling commit publishes the cache | "Reload": a reload after it does
  StatusRead   \* "Incremental": a publication applies its own index only | "Validated" or "Snapshot": it puts every status it read

\* Each switch takes one of its two values.
ASSUME PublishMode \in {"Wait", "Queue"}
ASSUME SettleMode \in {"Commit", "Reload"}
ASSUME StatusRead \in {"Incremental", "Validated", "Snapshot"}

\* The two indexes someone creates.
Indexes == {"a", "b"}

\* What the store or the cache can say about an index: not defined, being
\* built, built, or failed.
Status == {"None", "Building", "Built", "Errored"}

\* Where each index's lifecycle is:
\*   Idle       the create has not read yet
\*   Created    the create read the definition and statuses, about to commit
\*   AwaitPub   the create committed and waits for its publication ("Wait")
\*   Returned   the create returned; the backfill is about to read
\*   Settling   the backfill read the definition and statuses, about to commit
\*   SettlePub  the backfill committed and waits for its publication
\*   ReloadRead the backfill committed and is about to reload ("Reload")
\*   ReloadPub  the reload read the epoch and the store, about to publish
\*   Done       finished
Pcs == {"Idle", "Created", "AwaitPub", "Returned", "Settling", "SettlePub",
        "ReloadRead", "ReloadPub", "Done", "DropPub", "Dropped"}

\* A queued publication: the whole collection it puts, or, when other is
\* TRUE, the unrelated change, which puts nothing of this collection.
Pub == [other : BOOLEAN, target : Indexes, val : [Indexes -> Status]]

\* The states in which the create has returned to its caller.
CreatedPcs == {"Returned", "Settling", "SettlePub", "ReloadRead", "ReloadPub", "Done"}

VARIABLES
  store,     \* each index's status on disk
  defVer,    \* how many commits wrote the definition key: what a validated read of it compares
  statVer,   \* how many commits wrote an index status key: what a validated scan of them compares
  cache,     \* each index's status in the collection cache
  epoch,     \* how many publications the cache has taken: what a reload compares
  queue,     \* the publications committed and not run yet, in commit order
  appended,  \* how many publications have ever been queued
  applied,   \* how many of them have run
  held,      \* a writer's commit hook still holds back the publications
  otherDone, \* the unrelated publication has happened
  pc,        \* each index's lifecycle step
  snap,      \* the statuses the index's open transaction read
  readDef,   \* the definition version that transaction read
  readStat,  \* the status version that transaction read
  mySeq,     \* the position of the index's last publication in the queue
  reloadEp,  \* the epoch the index's reload read
  outcome    \* how the index's backfill ended: "Built" or "Errored"

vars == <<store, defVer, statVer, cache, epoch, queue, appended, applied,
          held, otherDone, pc, snap, readDef, readStat, mySeq, reloadEp, outcome>>

\* Every variable stays in its range.
TypeOK ==
  /\ store \in [Indexes -> Status]                       \* a status per index on disk
  /\ cache \in [Indexes -> Status]                       \* a status per index in memory
  /\ defVer \in Nat /\ statVer \in Nat                   \* change counters
  /\ epoch \in Nat                                       \* a publication counter
  /\ queue \in Seq(Pub)                                 \* queued publications
  /\ appended \in Nat /\ applied \in Nat                 \* queue positions
  /\ held \in BOOLEAN /\ otherDone \in BOOLEAN           \* the two flags
  /\ pc \in [Indexes -> Pcs]                             \* a known step per index
  /\ snap \in [Indexes -> [Indexes -> Status]]           \* what each open transaction read
  /\ readDef \in [Indexes -> Nat]                        \* the definition version it read
  /\ readStat \in [Indexes -> Nat]                       \* the status version it read
  /\ mySeq \in [Indexes -> Nat]                          \* where its publication sits
  /\ reloadEp \in [Indexes -> Nat]                       \* the epoch its reload read
  /\ outcome \in [Indexes -> {"Built", "Errored"}]       \* how its backfill ended

\* Nothing is defined, the cache agrees, and a writer holds the publications.
Init ==
  /\ store = [i \in Indexes |-> "None"]          \* no index on disk
  /\ cache = [i \in Indexes |-> "None"]          \* none in memory
  /\ defVer = 0 /\ statVer = 0                   \* nothing written yet
  /\ epoch = 0                                   \* no publication yet
  /\ queue = <<>> /\ appended = 0 /\ applied = 0 \* nothing queued
  /\ held = TRUE                                 \* a writer's hook is running
  /\ otherDone = FALSE                           \* the unrelated change is still to come
  /\ pc = [i \in Indexes |-> "Idle"]             \* no create has started
  /\ snap = [i \in Indexes |-> [j \in Indexes |-> "None"]] \* no read yet
  /\ readDef = [i \in Indexes |-> 0]             \* no read yet
  /\ readStat = [i \in Indexes |-> 0]            \* no read yet
  /\ mySeq = [i \in Indexes |-> 0]               \* no publication yet
  /\ reloadEp = [i \in Indexes |-> 0]            \* no reload yet
  /\ outcome = [i \in Indexes |-> "Built"]       \* decided when the backfill settles

\* A transaction opened now reads the definition and the statuses as they
\* are on disk, and remembers which versions it read.
Open(i) ==
  /\ snap' = [snap EXCEPT ![i] = store]          \* the statuses it will publish from
  /\ readDef' = [readDef EXCEPT ![i] = defVer]   \* the definition version it read
  /\ readStat' = [readStat EXCEPT ![i] = statVer] \* the status version it read

\* The commit of i's open transaction conflicts: the definition changed since
\* it read it (always validated), or, when that read is validated, a status.
Conflicts(i) ==
  \/ defVer # readDef[i]                                   \* a definition change landed first
  \/ (StatusRead = "Validated" /\ statVer # readStat[i]) \* an old whole-scan publication conflicts

\* Queue the publication of i's transaction: the whole collection as it
\* read it, with its own write applied.
Publish(i, status, statuses) ==
  /\ queue' = Append(queue, [other |-> FALSE, target |-> i, val |-> [statuses EXCEPT ![i] = status]]) \* what it computed from its reads
  /\ appended' = appended + 1                               \* one more queued
  /\ mySeq' = [mySeq EXCEPT ![i] = appended + 1]            \* remember where it sits

\* ---------------------------------------------------------------- create

\* The create opens its transaction and reads the definition and statuses.
CreateRead(i) ==
  /\ pc[i] = "Idle"                              \* not started
  /\ Open(i)                                     \* read what is on disk
  /\ pc' = [pc EXCEPT ![i] = "Created"]          \* about to commit
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, mySeq, reloadEp, outcome>>

\* The create's commit conflicts and its attempt runs again from the start.
CreateConflict(i) ==
  /\ pc[i] = "Created"                           \* holding its reads
  /\ Conflicts(i)                                \* something it read changed
  /\ pc' = [pc EXCEPT ![i] = "Idle"]             \* run again
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* The create commits the index with its "building" record and queues its
\* publication; with "Queue" it returns at once, with "Wait" it waits.
CreateCommit(i) ==
  /\ pc[i] = "Created"                                  \* holding its reads
  /\ ~Conflicts(i)                                      \* nothing it read changed
  /\ store' = [store EXCEPT ![i] = "Building"]          \* the definition and record land
  /\ defVer' = defVer + 1                               \* it wrote the definition key
  /\ statVer' = statVer + 1                             \* and a status key
  /\ Publish(i, "Building", snap[i])                    \* its cache publication waits its turn
  /\ pc' = [pc EXCEPT ![i] = IF PublishMode = "Wait" THEN "AwaitPub" ELSE "Returned"]
  /\ UNCHANGED <<cache, epoch, applied, held, otherDone, snap, readDef,
                 readStat, reloadEp, outcome>>

\* The create returns once its own publication has run.
CreateReturn(i) ==
  /\ pc[i] = "AwaitPub"                          \* waiting for its publication
  /\ applied >= mySeq[i]                         \* it has run
  /\ pc' = [pc EXCEPT ![i] = "Returned"]         \* back to the caller
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* ---------------------------------------------------------------- backfill

\* The backfill, its pages done, opens its settling transaction.
SettleRead(i) ==
  /\ pc[i] = "Returned"                          \* the create returned
  /\ Open(i)                                     \* read what is on disk
  /\ pc' = [pc EXCEPT ![i] = "Settling"]         \* about to commit
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, mySeq, reloadEp, outcome>>

\* The settling commit conflicts and runs again.
SettleConflict(i) ==
  /\ pc[i] = "Settling"                          \* holding its reads
  /\ Conflicts(i)                                \* something it read changed
  /\ pc' = [pc EXCEPT ![i] = "Returned"]         \* run again
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* The backfill settles as built or as failed, in one commit. With "Commit"
\* that commit queues the publication and the backfill waits for it; with
\* "Reload" nothing is published and a reload follows.
SettleCommit(i) ==
  /\ pc[i] = "Settling"                                 \* holding its reads
  /\ ~Conflicts(i)                                      \* nothing it read changed
  /\ \E end \in {"Built", "Errored"}:                   \* the backfill completed or failed
       /\ outcome' = [outcome EXCEPT ![i] = end]        \* remember which
       /\ store' = [store EXCEPT ![i] = end]            \* the record is cleared or errored
       /\ IF SettleMode = "Commit"
            THEN /\ Publish(i, end, snap[i])            \* the commit publishes it
                 /\ pc' = [pc EXCEPT ![i] = "SettlePub"] \* and waits for it
            ELSE /\ UNCHANGED <<queue, appended, mySeq>> \* nothing published
                 /\ pc' = [pc EXCEPT ![i] = "ReloadRead"] \* a reload follows
  /\ statVer' = statVer + 1                             \* it wrote a status key
  /\ UNCHANGED <<defVer, cache, epoch, applied, held, otherDone, snap,
                 readDef, readStat, reloadEp>>

\* The backfill ends once its publication has run.
SettleReturn(i) ==
  /\ pc[i] = "SettlePub"                         \* waiting for its publication
  /\ applied >= mySeq[i]                         \* it has run
  /\ pc' = [pc EXCEPT ![i] = "Done"]             \* finished
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* The index is dropped: its definition and status record leave in one commit,
\* and the publication names None for that index. With "Wait" the drop returns
\* only after the publication; with "Queue" it returns at once.
Drop(i) ==
  /\ pc[i] = "Done"                                      \* the index finished building
  /\ store' = [store EXCEPT ![i] = "None"]               \* definition and record are gone
  /\ defVer' = defVer + 1                                \* the definition key changed
  /\ statVer' = statVer + 1                              \* the backfill status changed
  /\ Publish(i, "None", store)                           \* the drop uses the store at its commit
  /\ pc' = [pc EXCEPT ![i] = IF PublishMode = "Wait" THEN "DropPub" ELSE "Dropped"]
  /\ UNCHANGED <<cache, epoch, applied, held, otherDone, snap, readDef,
                 readStat, reloadEp, outcome>>

\* The drop returns once its own publication has run.
DropReturn(i) ==
  /\ pc[i] = "DropPub"                                   \* waiting for its publication
  /\ applied >= mySeq[i]                                 \* it has run
  /\ pc' = [pc EXCEPT ![i] = "Dropped"]                  \* back to the caller
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* The reload reads the cache's epoch and everything on disk.
ReloadRead(i) ==
  /\ pc[i] = "ReloadRead"                        \* the settle committed
  /\ reloadEp' = [reloadEp EXCEPT ![i] = epoch]  \* the epoch it started at
  /\ snap' = [snap EXCEPT ![i] = store]          \* what it will publish
  /\ pc' = [pc EXCEPT ![i] = "ReloadPub"]        \* about to publish
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, held, otherDone, readDef, readStat, mySeq, outcome>>

\* The reload publishes only if no publication ran since it read the epoch;
\* otherwise it is dropped as stale.
ReloadPublish(i) ==
  /\ pc[i] = "ReloadPub"                         \* holding its read
  /\ IF epoch = reloadEp[i]
       THEN /\ cache' = snap[i]                  \* nothing overtook it: it lands
            /\ epoch' = epoch + 1                \* one more publication
       ELSE UNCHANGED <<cache, epoch>>           \* overtaken: dropped
  /\ pc' = [pc EXCEPT ![i] = "Done"]             \* finished
  /\ UNCHANGED <<store, defVer, statVer, queue, appended, applied, held,
                 otherDone, snap, readDef, readStat, mySeq, reloadEp, outcome>>

\* ---------------------------------------------------------------- the rest

\* The writer's hook finishes and lets publications run.
Release ==
  /\ held                                        \* still holding
  /\ held' = FALSE                               \* not any more
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, queue, appended,
                 applied, otherDone, pc, snap, readDef, readStat, mySeq,
                 reloadEp, outcome>>

\* Something else commits an unrelated change and queues its publication.
Other ==
  /\ ~otherDone                                  \* once
  /\ otherDone' = TRUE                           \* done
  /\ queue' = Append(queue, [other |-> TRUE, target |-> "a", val |-> cache]) \* its publication waits its turn
  /\ appended' = appended + 1                    \* one more queued
  /\ UNCHANGED <<store, defVer, statVer, cache, epoch, applied, held, pc,
                 snap, readDef, readStat, mySeq, reloadEp, outcome>>

\* The oldest queued publication runs, once no hook holds it back. The
\* unrelated one leaves these indexes alone but still counts as a publication.
Dispatch ==
  /\ ~held                                       \* nothing holds it back
  /\ queue # <<>>                                \* something to run
  /\ cache' = IF Head(queue).other
                THEN cache                    \* the unrelated change touches no index
                ELSE IF StatusRead = "Incremental"
                     THEN [cache EXCEPT ![Head(queue).target] = Head(queue).val[Head(queue).target]] \* apply only the publishing index
                     ELSE Head(queue).val         \* the old whole-collection publication
  /\ epoch' = epoch + 1                          \* one more publication
  /\ queue' = Tail(queue)                        \* it is done
  /\ applied' = applied + 1                      \* and counted
  /\ UNCHANGED <<store, defVer, statVer, appended, held, otherDone, pc, snap,
                 readDef, readStat, mySeq, reloadEp, outcome>>

\* Any one actor takes its next step.
Next ==
  \/ \E i \in Indexes:
       \/ CreateRead(i) \/ CreateConflict(i) \/ CreateCommit(i) \/ CreateReturn(i)
       \/ SettleRead(i) \/ SettleConflict(i) \/ SettleCommit(i) \/ SettleReturn(i)
       \/ ReloadRead(i) \/ ReloadPublish(i) \/ Drop(i) \/ DropReturn(i)
  \/ Release \/ Other \/ Dispatch

\* The system starts in Init and every step is one of Next.
Spec == Init /\ [][Next]_vars

\* Everyone has finished and nothing is left to publish.
Quiet ==
  /\ \A i \in Indexes: pc[i] \in {"Done", "Dropped"} \* every index settled or dropped
  /\ otherDone                                   \* the unrelated change happened
  /\ queue = <<>>                                \* every publication ran

\* ---------------------------------------------------------------- what must hold

\* Every variable stays in its range.
INV_TypeOK == TypeOK

\* A create that returned left its index in the cache, so its caller's next
\* request sees the definition it committed.
INV_ReturnSeesOwn ==
  /\ \A i \in Indexes: pc[i] \in CreatedPcs => cache[i] # "None"
  /\ \A i \in Indexes: pc[i] = "Dropped" => cache[i] = "None"

\* Once everyone has finished, the cache says what the store says about every
\* index: an index built on disk is used, and a failed one is listed failed.
INV_CacheMatchesStore ==
  Quiet => cache = store
====
