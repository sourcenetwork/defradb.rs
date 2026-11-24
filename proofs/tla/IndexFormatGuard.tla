---------------------------- MODULE IndexFormatGuard ----------------------------
\* Older and unstamped stores rebuild their secondary indexes into the tagged
\* keyspace. The old keys remain untouched because they can overlap other data.
\* Only after every rebuild succeeds may the current format be stamped and the
\* node open. A failed rebuild keeps the old stamp, so the next open retries.
\* Deferred position records may still name old keys, including after the
\* rebuild when a document is absent. Replace and drain may remove such a
\* record, but delete its referenced key only when it has the current index
\* prefix. A crash or rebuild failure leaves the stamp old and retry safe.
\* Each attempt uses a fresh reindex token. A pre-upgrade walk cursor is
\* never resumed, and a retry starts rebuilding each collection again.
\* A future format is refused before any migration writes.
\*
\* Rust: crates/database/db/src/definition/migration/format.rs
\* DB::check_index_format, DB::migrate_index_format, DB::rebuild_every_index,
\* DB::stamp_index_format; crates/database/db/src/database/mod.rs DB::finish_open;
\* crates/database/db/src/index/deferred/drain.rs and replace.rs.
EXTENDS FiniteSets
CONSTANTS Collections, Rebuild, StampLast, RefuseFuture, RetainLegacy, GuardRecordedKeys, FreshToken
ASSUME /\ Rebuild \in BOOLEAN /\ StampLast \in BOOLEAN /\ RefuseFuture \in BOOLEAN /\ RetainLegacy \in BOOLEAN /\ GuardRecordedKeys \in BOOLEAN /\ FreshToken \in BOOLEAN
VARIABLES stamp, initialStamp, rebuilt, phase, legacyIntact, wrote, oldRecords, staleProgress, attemptDone
vars == <<stamp, initialStamp, rebuilt, phase, legacyIntact, wrote, oldRecords, staleProgress, attemptDone>>
TypeOK == /\ stamp \in {"Old", "None", "Current", "Future"}
          /\ initialStamp \in {"Old", "None", "Current", "Future"}
          /\ rebuilt \subseteq Collections
          /\ phase \in {"Check", "Rebuild", "Open", "Failed", "Refused"}
          /\ legacyIntact \in BOOLEAN /\ wrote \in BOOLEAN
          /\ oldRecords \subseteq Collections /\ staleProgress \subseteq Collections
          /\ attemptDone \subseteq Collections
Init == /\ initialStamp \in {"Old", "None", "Current", "Future"}
        /\ stamp = initialStamp
        /\ rebuilt = IF stamp = "Current" THEN Collections ELSE {}
        /\ phase = "Check" /\ legacyIntact = TRUE /\ wrote = FALSE
        /\ oldRecords \in SUBSET Collections
        /\ staleProgress \in SUBSET Collections
        /\ attemptDone = IF stamp = "Current" THEN Collections ELSE {}
Check == /\ phase = "Check"
         /\ IF stamp = "Future" /\ RefuseFuture
               THEN /\ phase' = "Refused" /\ rebuilt' = rebuilt
               ELSE IF stamp = "Current" \/ stamp = "Future"
                    THEN /\ phase' = "Open" /\ rebuilt' = rebuilt
                    ELSE /\ phase' = "Rebuild"
                         /\ rebuilt' = IF FreshToken THEN {} ELSE staleProgress
         /\ attemptDone' = IF phase' = "Rebuild" THEN {} ELSE attemptDone
         /\ UNCHANGED <<stamp, initialStamp, legacyIntact, wrote, oldRecords, staleProgress>>
RebuildOne == /\ phase = "Rebuild" /\ Rebuild /\ rebuilt # Collections
              /\ \E c \in Collections \ rebuilt: /\ rebuilt' = rebuilt \cup {c}
                                                /\ attemptDone' = attemptDone \cup {c}
              /\ wrote' = TRUE /\ legacyIntact' = (legacyIntact /\ RetainLegacy)
              /\ UNCHANGED <<stamp, initialStamp, phase, oldRecords, staleProgress>>
Finish == /\ phase = "Rebuild"
          /\ (rebuilt = Collections \/ ~StampLast \/ ~Rebuild)
          /\ stamp' = "Current" /\ phase' = "Open" /\ wrote' = TRUE
          /\ UNCHANGED <<initialStamp, rebuilt, legacyIntact, oldRecords, staleProgress, attemptDone>>
Fail == /\ phase = "Rebuild" /\ phase' = "Failed"
        /\ UNCHANGED <<stamp, initialStamp, rebuilt, legacyIntact, wrote, oldRecords, staleProgress, attemptDone>>
Retry == /\ phase = "Failed" /\ phase' = "Check"
         /\ UNCHANGED <<stamp, initialStamp, rebuilt, legacyIntact, wrote, oldRecords, staleProgress, attemptDone>>
\* Replace during rebuilding or drain after open can consume a legacy record.
\* Its referenced key is ambiguous, so the guard must leave that key alone.
ConsumeOldRecord == /\ phase \in {"Rebuild", "Open"} /\ oldRecords # {}
                    /\ \E c \in oldRecords: oldRecords' = oldRecords \ {c}
                    /\ legacyIntact' = (legacyIntact /\ GuardRecordedKeys)
                    /\ wrote' = TRUE
                    /\ UNCHANGED <<stamp, initialStamp, rebuilt, phase, staleProgress, attemptDone>>
Next == Check \/ RebuildOne \/ Finish \/ Fail \/ Retry \/ ConsumeOldRecord
Spec == Init /\ [][Next]_vars
INV_TypeOK == TypeOK
INV_OpenCompatible == phase = "Open" => stamp = "Current" /\ rebuilt = Collections
\* The stale cursor must never count as work done in this attempt.
INV_FreshAttempt == phase = "Rebuild" => rebuilt \subseteq attemptDone
INV_FailureKeepsStamp == phase = "Failed" => stamp = initialStamp
INV_FutureRefusedUntouched == initialStamp = "Future" => phase = "Check" \/ (phase = "Refused" /\ ~wrote)
INV_LegacyIntact == legacyIntact
====
