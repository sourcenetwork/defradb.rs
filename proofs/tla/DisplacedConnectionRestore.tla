---- MODULE DisplacedConnectionRestore ----
\* Model the cache decision when an inbound connection is refused. The real
\* helper evicts by handle identity and restores only an open displaced handle
\* into an empty slot. A newer cached connection must remain untouched.
EXTENDS Naturals
CONSTANT Mode
ASSUME Mode \in {"correct", "noEvict", "restoreClosed"}
VARIABLES slot, displaced, displacedOpen, done
vars == <<slot, displaced, displacedOpen, done>>
Init == /\ slot \in {"empty", "refused", "fresh"}
        /\ displaced \in {"none", "sibling"}
        /\ displacedOpen \in BOOLEAN
        /\ done = FALSE
Evicted == IF slot = "refused" /\ Mode # "noEvict" THEN "empty" ELSE slot
Restore == /\ ~done
           /\ slot' = IF Evicted = "empty" /\ displaced = "sibling"
                           /\ (displacedOpen \/ Mode = "restoreClosed")
                      THEN "sibling" ELSE Evicted
           /\ done' = TRUE
           /\ UNCHANGED <<displaced, displacedOpen>>
Next == Restore
Spec == Init /\ [][Next]_vars
TypeOK == /\ slot \in {"empty", "refused", "fresh", "sibling"}
          /\ displaced \in {"none", "sibling"}
          /\ displacedOpen \in BOOLEAN /\ done \in BOOLEAN
NoRefused == done => slot # "refused"
NoClosedRestore == (done /\ ~displacedOpen) => slot # "sibling"
====
