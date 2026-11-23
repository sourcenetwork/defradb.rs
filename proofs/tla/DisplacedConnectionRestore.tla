---- MODULE DisplacedConnectionRestore ----
\* A peer has one slot in the RPC connection cache. An inbound connection was
\* published into that slot and then refused, and the publish may have pushed
\* an older sibling connection out of the slot. The refusal path now cleans
\* up: it removes the refused handle, and it puts the sibling back only if the
\* sibling is still open and the slot is empty. Meanwhile a newer connection
\* may already have taken the slot. What must never happen: the refused handle
\* stays cached, a closed sibling is cached again, or a newer connection loses
\* the slot.
\*
\* Code: crates/database/p2p/src/iroh/endpoint_rpc.rs
\* restore_displaced_connection and evict_connection.
\* Naturals is the standard module the checker loads for this spec.
EXTENDS Naturals
\* Mode picks the code under test: the real one, or one of four planted bugs.
CONSTANT Mode
\* "correct" is the code; "noEvict" skips the eviction; "restoreClosed" skips
\* the closed check; "evictAny" evicts whatever is cached instead of only the
\* refused handle; "plainInsert" restores with a plain insert instead of
\* insert_if_absent.
ASSUME Mode \in {"correct", "noEvict", "restoreClosed", "evictAny", "plainInsert"}
\* slot: what the cache holds for the peer. displaced: the handle the refused
\* publish pushed out, if any. displacedOpen: whether that handle is still
\* open. done: the cleanup has run.
VARIABLES slot, displaced, displacedOpen, done
\* All variables, for the "nothing else changes" part of the spec.
vars == <<slot, displaced, displacedOpen, done>>
\* Every starting point: the slot is empty, still holds the refused handle, or
\* already holds a newer connection;
Init == /\ slot \in {"empty", "refused", "fresh"}
\* the refused publish displaced nothing or displaced a sibling;
        /\ displaced \in {"none", "sibling"}
\* that sibling is open or already closed;
        /\ displacedOpen \in BOOLEAN
\* and the cleanup has not run yet.
        /\ done = FALSE
\* evict_connection: remove the refused handle if it is the one cached (the
\* stable_id check), and leave anything else alone. The "noEvict" bug leaves
\* it in; the "evictAny" bug drops the identity check and empties any slot.
Evicted == CASE Mode = "noEvict" -> slot
             [] Mode = "evictAny" -> "empty"
             [] OTHER -> IF slot = "refused" THEN "empty" ELSE slot
\* restore_displaced_connection runs once.
Restore == /\ ~done
\* After the eviction, the sibling goes back only into an empty slot
\* (insert_if_absent; the "plainInsert" bug writes over whatever is there),
\* only if there is a sibling,
           /\ slot' = IF (Evicted = "empty" \/ Mode = "plainInsert")
                           /\ displaced = "sibling"
\* and only while it is open. The "restoreClosed" bug drops this check.
                           /\ (displacedOpen \/ Mode = "restoreClosed")
\* Otherwise the slot keeps what the eviction left.
                      THEN "sibling" ELSE Evicted
\* The cleanup is finished.
           /\ done' = TRUE
\* The cleanup does not change what was displaced or whether it is open.
           /\ UNCHANGED <<displaced, displacedOpen>>
\* The cleanup is the only step.
Next == Restore
\* Start in Init, then take Next steps or stand still.
Spec == Init /\ [][Next]_vars
\* Every variable stays one of its allowed values.
TypeOK == /\ slot \in {"empty", "refused", "fresh", "sibling"}
          /\ displaced \in {"none", "sibling"}
          /\ displacedOpen \in BOOLEAN /\ done \in BOOLEAN
\* After the cleanup the refused handle is never in the cache.
NoRefused == done => slot # "refused"
\* After the cleanup a closed sibling is never in the cache.
NoClosedRestore == (done /\ ~displacedOpen) => slot # "sibling"
\* A newer connection that held the slot before the cleanup still holds it.
\* The slot only ever becomes "fresh" in Init, so "fresh" at the end is the
\* same connection; this says the cleanup never turns it into anything else.
FreshKept == [][slot = "fresh" => slot' = "fresh"]_vars
====
