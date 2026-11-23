------------------------- MODULE OfflineReconnect -------------------------
\* BacklogOwner processes offline-park and reconnect commands after consulting
\* PeerStateTracker. A failed send or capacity response must keep its cooldown.
\* The connection event writes connected before queuing its command. The owner
\* serializes commands and rechecks that state when it consumes them.
EXTENDS Naturals
CONSTANT FencePark, FenceReconnect, PreserveFailure
VARIABLES connected, cooldown, parkPending, connectPending, badPark, badClear
vars == <<connected, cooldown, parkPending, connectPending, badPark, badClear>>
Init == /\ connected = FALSE /\ cooldown = "none"
        /\ parkPending = FALSE /\ connectPending = FALSE
        /\ badPark = FALSE /\ badClear = FALSE
ObserveOffline == /\ ~connected /\ ~parkPending
                  /\ parkPending' = TRUE
                  /\ UNCHANGED <<connected, cooldown, connectPending, badPark, badClear>>
Connect == /\ ~connected /\ connected' = TRUE /\ connectPending' = TRUE
           /\ UNCHANGED <<cooldown, parkPending, badPark, badClear>>
Disconnect == /\ connected /\ connected' = FALSE
              /\ UNCHANGED <<cooldown, parkPending, connectPending, badPark, badClear>>
Failure == /\ cooldown' = "failure"
           /\ UNCHANGED <<connected, parkPending, connectPending, badPark, badClear>>
ProcessPark == /\ parkPending /\ parkPending' = FALSE
               /\ IF connected /\ FencePark
                     THEN /\ UNCHANGED cooldown /\ UNCHANGED badPark
                     ELSE /\ cooldown' = (IF cooldown = "failure" THEN "failure" ELSE "offline")
                          /\ badPark' = (badPark \/ connected)
               /\ UNCHANGED <<connected, connectPending, badClear>>
ProcessConnect == /\ connectPending /\ connectPending' = FALSE
                  /\ IF connected \/ ~FenceReconnect
                        THEN /\ cooldown' = (IF cooldown = "offline" \/ ~PreserveFailure THEN "none" ELSE cooldown)
                             /\ badClear' = (badClear \/ (cooldown = "failure" /\ ~PreserveFailure) \/ (~connected /\ cooldown = "offline" /\ ~FenceReconnect))
                        ELSE /\ UNCHANGED <<cooldown, badClear>>
                  /\ UNCHANGED <<connected, parkPending, badPark>>
Next == ObserveOffline \/ Connect \/ Disconnect \/ Failure \/ ProcessPark \/ ProcessConnect
Spec == Init /\ [][Next]_vars
INV_NoStalePark == ~badPark
INV_KeepFailure == ~badClear
=============================================================================
