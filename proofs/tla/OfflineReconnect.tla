------------------------- MODULE OfflineReconnect -------------------------
\* Model the backlog owner's checks against the latest peer connection state.
EXTENDS Naturals  \* Use natural-number operators.
\* Switches plant stale commands, failure clearing, and expired-entry bugs.
CONSTANT FencePark, FenceReconnect, PreserveFailure, ClearExpired  \* Choose which guards the configuration enables.
\* Track connection, cooldown, queued commands, and observed safety violations.
VARIABLES connected, cooldown, parkPending, connectPending, badPark, badClear, badExpired  \* Keep connection, cooldown, pending commands and violations.
\* Group every variable for unchanged and temporal expressions.
vars == <<connected, cooldown, parkPending, connectPending, badPark, badClear, badExpired>>  \* Group the state for temporal operators.
\* Initially the peer is offline, has no cooldown, and has no pending commands.
Init == /\ connected = FALSE /\ cooldown = "none"  \* Start disconnected without a cooldown.
        /\ parkPending = FALSE /\ connectPending = FALSE  \* Start with no queued commands.
        /\ badPark = FALSE /\ badClear = FALSE /\ badExpired = FALSE  \* No safety violation has occurred.
\* A worker observes an offline peer and queues a park.
ObserveOffline == /\ ~connected /\ ~parkPending  \* A worker sees a disconnected peer.
                  /\ parkPending' = TRUE  \* Queue the observation for the owner.
                  /\ UNCHANGED <<connected, cooldown, connectPending, badPark, badClear, badExpired>>  \* Leave the other variables unchanged.
\* Connecting updates the shared state before queuing a reconnect command.
Connect == /\ ~connected /\ connected' = TRUE /\ connectPending' = TRUE  \* Connect first, then queue its event.
           /\ UNCHANGED <<cooldown, parkPending, badPark, badClear, badExpired>>  \* Leave the other variables unchanged.
\* Disconnection can occur before the queued reconnect is consumed.
Disconnect == /\ connected /\ connected' = FALSE  \* Disconnect before an event may be handled.
              /\ UNCHANGED <<cooldown, parkPending, connectPending, badPark, badClear, badExpired>>  \* Leave the other variables unchanged.
\* A transport failure or capacity response creates a protected cooldown.
Failure == /\ cooldown' = "failure"  \* A transport failure creates a protected cooldown.
           /\ UNCHANGED <<connected, parkPending, connectPending, badPark, badClear, badExpired>>  \* Leave the other variables unchanged.
\* An old failure cooldown may expire before the peer is parked offline.
ExpireFailure == /\ cooldown = "failure" /\ cooldown' = "expired"  \* Allow that cooldown to expire.
                 /\ UNCHANGED <<connected, parkPending, connectPending, badPark, badClear, badExpired>>  \* Leave the other variables unchanged.
\* Ignore a stale park after reconnect; otherwise record an offline cooldown.
ProcessPark == /\ parkPending /\ parkPending' = FALSE  \* Take the next offline observation.
               /\ IF connected /\ FencePark  \* Check whether the observation is stale.
                     THEN /\ UNCHANGED cooldown /\ UNCHANGED badPark /\ UNCHANGED badExpired  \* Ignore a stale observation.
                     ELSE /\ cooldown' = (IF cooldown = "failure" \/ (cooldown = "expired" /\ ~ClearExpired) THEN "failure" ELSE "offline")  \* Preserve a live failure; replace an expired one.
                          /\ badPark' = (badPark \/ connected)  \* Remember an incorrectly applied stale park.
                          /\ badExpired' = (badExpired \/ (cooldown = "expired" /\ ~ClearExpired))  \* Remember an incorrectly retained expired cooldown.
               /\ UNCHANGED <<connected, connectPending, badClear>>  \* Leave the other variables unchanged.
\* A current reconnect clears only an offline cooldown, preserving live failures.
ProcessConnect == /\ connectPending /\ connectPending' = FALSE  \* Take the next reconnect event.
                  /\ IF connected \/ ~FenceReconnect  \* Check that reconnect is still current.
                        THEN /\ cooldown' = (IF cooldown = "offline" \/ ~PreserveFailure THEN "none" ELSE cooldown)  \* Clear only the offline cooldown.
                             /\ badClear' = (badClear \/ (cooldown = "failure" /\ ~PreserveFailure) \/ (~connected /\ cooldown = "offline" /\ ~FenceReconnect))  \* Remember a cleared live failure or stale reconnect.
                        ELSE /\ UNCHANGED <<cooldown, badClear>>  \* Ignore an outdated reconnect.
                  /\ UNCHANGED <<connected, parkPending, badPark, badExpired>>  \* Leave the other variables unchanged.
\* The owner and peer can take any of the transitions above.
Next == ObserveOffline \/ Connect \/ Disconnect \/ Failure \/ ExpireFailure \/ ProcessPark \/ ProcessConnect  \* Allow each action as the next step.
\* Permit stuttering while maintaining the transition relation.
Spec == Init /\ [][Next]_vars  \* Start at Init and allow actions or stuttering.
\* An observation made before reconnect must not create a new offline park.
INV_NoStalePark == ~badPark  \* No stale park is applied.
\* A reconnect must not discard a live failure or a newer offline state.
INV_KeepFailure == ~badClear  \* No protected cooldown is cleared.
\* An expired failure must not prevent a later offline park from clearing.
INV_ExpiredCanClear == ~badExpired  \* An expired cooldown does not prevent clearing.
=============================================================================
