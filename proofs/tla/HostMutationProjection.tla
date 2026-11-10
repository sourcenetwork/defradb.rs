--------------------- MODULE HostMutationProjection ---------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES enabled, source, direct, other, unauthorized, unregistered
vars == <<enabled, source, direct, other, unauthorized, unregistered>>

Init == /\ enabled = FALSE /\ source = FALSE
        /\ direct = TRUE /\ other = TRUE /\ unauthorized = FALSE /\ unregistered = FALSE

Publish(route, authorized, registered) ==
  /\ (authorized \/ Bug = "Authority")
  /\ (registered \/ Bug = "Owner")
  /\ enabled' = ~enabled
  /\ source' = IF Bug = "Atomic" \/ (Bug = "Raw" /\ route = "raw") \/
                    (Bug = "Replicated" /\ route = "replicated") \/
                    (Bug = "Schema" /\ route = "schema")
                THEN source ELSE ~enabled
  /\ direct' = IF Bug = "Direct" THEN FALSE ELSE direct
  /\ other' = IF Bug = "Other" THEN FALSE ELSE other
  /\ unauthorized' = (unauthorized \/ ~authorized)
  /\ unregistered' = (unregistered \/ ~registered)

Next == \E route \in {"managed", "raw", "replicated", "schema"}, authorized, registered \in BOOLEAN :
          Publish(route, authorized, registered)
Spec == Init /\ [][Next]_vars
INV_Atomic == enabled = source
INV_Independent == direct /\ other
INV_Authority == ~unauthorized
INV_Registered == ~unregistered
=============================================================================
