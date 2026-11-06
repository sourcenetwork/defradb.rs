--------------------- MODULE HostMutationProjection ---------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES enabled, source, direct, other, unauthorized
vars == <<enabled, source, direct, other, unauthorized>>

Init == /\ enabled = FALSE /\ source = FALSE
        /\ direct = TRUE /\ other = TRUE /\ unauthorized = FALSE

Publish(route, authorized) ==
  /\ (authorized \/ Bug = "Authority")
  /\ enabled' = ~enabled
  /\ source' = IF Bug = "Atomic" \/ (Bug = "Raw" /\ route = "raw") \/
                    (Bug = "Replicated" /\ route = "replicated") \/
                    (Bug = "Schema" /\ route = "schema")
                THEN source ELSE ~enabled
  /\ direct' = IF Bug = "Direct" THEN FALSE ELSE direct
  /\ other' = IF Bug = "Other" THEN FALSE ELSE other
  /\ unauthorized' = (unauthorized \/ ~authorized)

Next == \E route \in {"managed", "raw", "replicated", "schema"}, authorized \in BOOLEAN :
          Publish(route, authorized)
Spec == Init /\ [][Next]_vars
INV_Atomic == enabled = source
INV_Independent == direct /\ other
INV_Authority == ~unauthorized
=============================================================================
