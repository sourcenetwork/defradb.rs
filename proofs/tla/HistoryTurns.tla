---------------------------- MODULE HistoryTurns ----------------------------
\* Two history roots share two ancestors. Each committed turn advances one
\* cursor together with its writes. A restart may throw away bookkeeping,
\* never published blocks. The next walk must not apply an ancestor twice.
\* Rust: merge_handler/history.rs, history_registry.rs, and dispatch.rs.
EXTENDS Naturals, FiniteSets
\* Each bug removes one guard in a RED witness.
CONSTANT Bug
\* A one-slot registry is enough to expose racing admissions.
Roots == {1, 2}
\* Each root has two ancestors; both roots refer to the same document.
Nodes == {1, 2}
\* Registry versions change on admission/removal, not on ordinary page writes.
VARIABLES roots, registryVersion, cursor, versions, stage, observed, observedRegistry,
          published, count, events, reclaimed, lostActive, lostBlocks
\* All the state in one tuple for unchanged clauses.
vars == <<roots, registryVersion, cursor, versions, stage, observed, observedRegistry,
          published, count, events, reclaimed, lostActive, lostBlocks>>
\* Both callers start before admission, with an empty database.
Init == /\ roots = {} /\ registryVersion = 0
        /\ cursor = [r \in Roots |-> 0] /\ versions = [r \in Roots |-> 0]
        /\ stage = [r \in Roots |-> "new"] /\ observed = [r \in Roots |-> 0]
        /\ observedRegistry = [r \in Roots |-> 0]
        /\ published = {} /\ count = 0 /\ events = 0
        /\ reclaimed = FALSE /\ lostActive = FALSE /\ lostBlocks = FALSE
\* An admission attempt scans the registry at its own snapshot.
ReadAdmission(r) == /\ stage[r] = "new" /\ Cardinality(roots) < 1
                    /\ stage' = [stage EXCEPT ![r] = "admit"]
                    /\ observedRegistry' = [observedRegistry EXCEPT ![r] = registryVersion]
                    /\ UNCHANGED <<roots, registryVersion, cursor, versions, observed, published,
                                   count, events, reclaimed, lostActive, lostBlocks>>
\* Validating the scan refuses a concurrent new root, without a shared counter.
Admit(r) == /\ stage[r] = "admit"
            /\ (Bug = "admission" \/ observedRegistry[r] = registryVersion)
            /\ roots' = roots \cup {r} /\ registryVersion' = registryVersion + 1
            /\ stage' = [stage EXCEPT ![r] = "run"]
            /\ UNCHANGED <<cursor, versions, observed, observedRegistry, published,
                           count, events, reclaimed, lostActive, lostBlocks>>
\* The retry engine re-reads after another admission won.
RetryAdmission(r) == /\ stage[r] = "admit" /\ observedRegistry[r] # registryVersion
                     /\ stage' = [stage EXCEPT ![r] = "new"]
                     /\ UNCHANGED <<roots, registryVersion, cursor, versions, observed, observedRegistry,
                                    published, count, events, reclaimed, lostActive, lostBlocks>>
\* A turn publishes its next ancestor once and moves its cursor in the same commit.
Page(r) == /\ stage[r] = "run" /\ r \in roots /\ cursor[r] < 2
           /\ LET node == cursor[r] + 1 IN
              /\ published' = IF Bug = "atomic" THEN published ELSE published \cup {node}
              /\ count' = count + (IF node \notin published \/ Bug = "fence" THEN 1 ELSE 0)
              /\ events' = events + (IF node \notin published THEN 1 ELSE 0)
           /\ cursor' = [cursor EXCEPT ![r] = @ + 1]
           /\ versions' = [versions EXCEPT ![r] = @ + 1]
           /\ UNCHANGED <<roots, registryVersion, stage, observed, observedRegistry,
                          reclaimed, lostActive, lostBlocks>>
\* Failed or cancelled attempts emit nothing and leave the cursor untouched.
Abort(r) == /\ stage[r] = "run" /\ Bug = "events"
            /\ events' = events + 1
            /\ UNCHANGED <<roots, registryVersion, cursor, versions, stage, observed,
                           observedRegistry, published, count, reclaimed, lostActive, lostBlocks>>
\* A full history releases its registry entry after bounded bookkeeping cleanup.
Done(r) == /\ stage[r] = "run" /\ cursor[r] = 2
           /\ roots' = roots \ {r} /\ registryVersion' = registryVersion + 1
           /\ stage' = [stage EXCEPT ![r] = "done"]
           /\ UNCHANGED <<cursor, versions, observed, observedRegistry, published,
                          count, events, reclaimed, lostActive, lostBlocks>>
\* Maintenance observes an idle cursor only under full registry pressure.
ObserveIdle(r) == /\ r \in roots /\ ~reclaimed /\ stage[r] = "run"
                  /\ versions[r] < 3
                  /\ observed' = [observed EXCEPT ![r] = versions[r]]
                  /\ stage' = [stage EXCEPT ![r] = "observed"]
                  /\ UNCHANGED <<roots, registryVersion, cursor, versions, observedRegistry,
                                 published, count, events, reclaimed, lostActive, lostBlocks>>
\* Work can resume after an idle observation but before cleanup commits.
Resume(r) == /\ stage[r] = "observed"
             /\ versions' = [versions EXCEPT ![r] = @ + 1]
             /\ stage' = [stage EXCEPT ![r] = "resumed"]
             /\ UNCHANGED <<roots, registryVersion, cursor, observed, observedRegistry,
                            published, count, events, reclaimed, lostActive, lostBlocks>>
\* The cleanup transaction validates the observed cursor and never removes blocks.
Reclaim(r) == /\ stage[r] \in {"observed", "resumed"} /\ ~reclaimed
              /\ (Bug = "reclaim" \/ observed[r] = versions[r])
              /\ roots' = roots \ {r} /\ registryVersion' = registryVersion + 1
              /\ cursor' = [cursor EXCEPT ![r] = 0]
              /\ stage' = [stage EXCEPT ![r] = "new"] /\ reclaimed' = TRUE
              /\ lostActive' = (lostActive \/ observed[r] # versions[r])
              /\ lostBlocks' = (lostBlocks \/ (Bug = "blocks" /\ published # {}))
              /\ UNCHANGED <<versions, observed, observedRegistry, published, count, events>>
\* A stale maintenance attempt abandons cleanup; the worker keeps its progress.
KeepActive(r) == /\ stage[r] = "resumed" /\ observed[r] # versions[r]
                 /\ stage' = [stage EXCEPT ![r] = "run"]
                 /\ UNCHANGED <<roots, registryVersion, cursor, versions, observed, observedRegistry,
                                published, count, events, reclaimed, lostActive, lostBlocks>>
\* All schedules include admission races, interrupted walks and maintenance.
Next == \E r \in Roots : ReadAdmission(r) \/ Admit(r) \/ RetryAdmission(r) \/ Page(r)
                        \/ Abort(r) \/ Done(r) \/ ObserveIdle(r) \/ Resume(r) \/ Reclaim(r) \/ KeepActive(r)
\* Concurrent admissions cannot exceed the durable cursor capacity.
INV_Slots == Cardinality(roots) <= 1
\* One CID contributes its counter delta at most once, including after restart.
INV_Once == count = Cardinality(published)
\* An advanced cursor cannot claim a frame whose writes never committed.
INV_Cursor == \A r \in Roots : \A n \in 1..cursor[r] : n \in published
\* Only committed publications emit updates.
INV_Events == events = Cardinality(published)
\* Maintenance cannot erase a cursor changed since it was observed idle.
INV_Active == ~lostActive
\* Maintenance deletes bookkeeping, not document or block data.
INV_Blocks == ~lostBlocks
=============================================================================
