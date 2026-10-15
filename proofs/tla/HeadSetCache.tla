---- MODULE HeadSetCache ----
EXTENDS HeadSet

\* The durable transitions are the existing HeadSet operators. Cache publication
\* and snapshot capture share an atomic boundary, not an application-transaction
\* lock. A real cache copies raw values unchanged and has a byte budget; eviction
\* is always permitted. An old reader pins its own immutable version.
CONSTANT CacheMode
ASSUME CacheMode \in {"Coherent", "StaleFill", "WriterSnapshot", "AbortPublish", "HistoryScan"}

VARIABLES resident, cached, epoch, txnStore, txnCached, txnWarm, txnEpoch,
          readDone, resolved, checked, readActual, readExpected, deleted,
          lastWarm, lastWork, lastLogical

cacheVars == <<vars, resident, cached, epoch, txnStore, txnCached, txnWarm,
               txnEpoch, readDone, resolved, checked, readActual, readExpected,
               deleted, lastWarm, lastWork, lastLogical>>
Current == HeadState(headKeys, supersedes)
Rows(s) == Cardinality(s.heads) + Cardinality(s.markers)
CacheInit ==
  /\ Init
  /\ resident = FALSE
  /\ cached = HeadState({}, {})
  /\ epoch = 0
  /\ txnStore = [w \in Writers |-> HeadState({}, {})]
  /\ txnCached = txnStore
  /\ txnWarm = [w \in Writers |-> FALSE]
  /\ txnEpoch = [w \in Writers |-> 0]
  /\ readDone = {}
  /\ resolved = {}
  /\ checked = FALSE
  /\ readActual = {}
  /\ readExpected = {}
  /\ deleted = {}
  /\ lastWarm = FALSE
  /\ lastWork = 0
  /\ lastLogical = 0

CacheBegin(w) ==
  /\ w \notin snapshotted
  /\ snapshotted' = snapshotted \cup {w}
  /\ txnStore' = [txnStore EXCEPT ![w] = Current]
  /\ txnCached' = [txnCached EXCEPT ![w] = cached]
  /\ txnWarm' = [txnWarm EXCEPT ![w] = resident]
  /\ txnEpoch' = [txnEpoch EXCEPT ![w] = epoch]
  /\ UNCHANGED <<committed, aborted, observed, walked, headKeys, supersedes,
                  parents, writeLog, pruned, resident, cached, epoch, readDone,
                  resolved, checked, readActual, readExpected, deleted,
                  lastWarm, lastWork, lastLogical>>

Base(w) == IF txnWarm[w] THEN txnCached[w] ELSE txnStore[w]
CacheRead(w) ==
  /\ w \in snapshotted \ readDone
  /\ observed' = [observed EXCEPT ![w] = HeadStateHeads(Base(w))]
  /\ walked' = [walked EXCEPT ![w] = txnStore[w].heads]
  /\ readDone' = readDone \cup {w}
  /\ checked' = TRUE
  /\ readActual' = HeadStateHeads(Base(w))
  /\ readExpected' = HeadStateHeads(txnStore[w])
  /\ lastWarm' = txnWarm[w]
  /\ lastLogical' = Rows(Base(w))
  /\ lastWork' = Rows(Base(w)) +
       IF ~txnWarm[w] \/ CacheMode = "HistoryScan" THEN Cardinality(deleted) ELSE 0
  /\ UNCHANGED <<committed, aborted, snapshotted, headKeys, supersedes, parents,
                  writeLog, pruned, resident, cached, epoch, txnStore, txnCached,
                  txnWarm, txnEpoch, resolved, deleted>>

\* A cold reader may finish after another transaction committed. Publish the
\* captured base only; pending writes must never leak through a cold fill.
CacheFill(w) ==
  /\ w \in readDone
  /\ ~txnWarm[w]
  /\ ~resident
  /\ (txnEpoch[w] = epoch \/ CacheMode = "StaleFill")
  /\ cached' = txnStore[w]
  /\ resident' = TRUE
  /\ UNCHANGED <<vars, epoch, txnStore, txnCached, txnWarm, txnEpoch,
                  readDone, resolved, checked, readActual, readExpected, deleted,
                  lastWarm, lastWork, lastLogical>>

CacheCommit(w) ==
  /\ w \in readDone \ resolved
  /\ LET next == AppendHeadState(Current, w, observed[w]) IN
       /\ headKeys' = next.heads
       /\ supersedes' = next.markers
  /\ parents' = [parents EXCEPT ![w] = observed[w]]
  /\ committed' = committed \cup {w}
  /\ resolved' = resolved \cup {w}
  /\ epoch' = epoch + 1
  /\ cached' = IF resident THEN
       AppendHeadState(IF CacheMode = "WriterSnapshot" THEN txnStore[w] ELSE cached,
                       w, observed[w]) ELSE cached
  /\ UNCHANGED <<aborted, snapshotted, observed, walked, writeLog, pruned,
                  resident, txnStore, txnCached, txnWarm, txnEpoch, readDone,
                  checked, readActual, readExpected, deleted,
                  lastWarm, lastWork, lastLogical>>

CacheAbort(w) ==
  /\ w \in readDone \ resolved
  /\ resolved' = resolved \cup {w}
  /\ aborted' = aborted \cup {w}
  /\ cached' = IF resident /\ CacheMode = "AbortPublish"
       THEN AppendHeadState(cached, w, observed[w]) ELSE cached
  /\ UNCHANGED <<committed, snapshotted, observed, walked, headKeys, supersedes,
                  parents, writeLog, pruned, resident, epoch, txnStore, txnCached,
                  txnWarm, txnEpoch, readDone, checked, readActual, readExpected,
                  deleted, lastWarm, lastWork, lastLogical>>

CacheReadOwn(w) ==
  /\ w \in readDone \ resolved
  /\ checked' = TRUE
  /\ readActual' = HeadStateHeads(AppendHeadState(Base(w), w, observed[w]))
  /\ readExpected' = HeadStateHeads(AppendHeadState(txnStore[w], w, observed[w]))
  /\ UNCHANGED <<vars, resident, cached, epoch, txnStore, txnCached, txnWarm,
                  txnEpoch, readDone, resolved, deleted, lastWarm, lastWork, lastLogical>>

CachePrune(b) ==
  /\ b \in headKeys \ DerivedHeads
  /\ LET next == PruneHeadState(Current, b) IN
       /\ headKeys' = next.heads
       /\ supersedes' = next.markers
  /\ pruned' = pruned \cup {b}
  /\ deleted' = deleted \cup {HeadKey(b)} \cup
       {SupersedeKey(p, c) : <<p, c>> \in {m \in supersedes : m[1] = b}}
  /\ cached' = IF resident THEN PruneHeadState(cached, b) ELSE cached
  /\ epoch' = epoch + 1
  /\ UNCHANGED <<committed, aborted, snapshotted, observed, walked, parents,
                  writeLog, resident, txnStore, txnCached, txnWarm, txnEpoch,
                  readDone, resolved, checked, readActual, readExpected,
                  lastWarm, lastWork, lastLogical>>

CacheEvict ==
  /\ resident
  /\ resident' = FALSE
  /\ UNCHANGED <<vars, cached, epoch, txnStore, txnCached, txnWarm, txnEpoch,
                  readDone, resolved, checked, readActual, readExpected, deleted,
                  lastWarm, lastWork, lastLogical>>

CacheNext ==
  \/ \E w \in Writers : CacheBegin(w) \/ CacheRead(w) \/ CacheFill(w) \/
                         CacheCommit(w) \/ CacheAbort(w) \/ CacheReadOwn(w)
  \/ \E b \in Blocks : CachePrune(b)
  \/ CacheEvict
CacheSpec == CacheInit /\ [][CacheNext]_cacheVars

INV_EpochSnapshot == \A w \in snapshotted : txnEpoch[w] = epoch => txnStore[w] = Current
INV_CacheExact == resident => cached = Current
INV_CapturedCacheExact == \A w \in snapshotted : txnWarm[w] => txnCached[w] = txnStore[w]
INV_CacheReadExact == checked => readActual = readExpected
INV_CacheWarmWork == lastWarm => lastWork = lastLogical
\* DAG truth must not be inferred from reclaimed head keys or markers.
INV_CacheDagExact == DerivedHeads = DagHeads
INV_CacheDisjoint == \A a, b \in readDone : a # b => WriteSet(a) \cap WriteSet(b) = {}
====
