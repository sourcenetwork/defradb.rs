-------------------------- MODULE PeerConnection --------------------------
EXTENDS TLC
\* An authenticated connection can accept requests before its queued
\* PeerConnected notification reaches the coordinator. Pull authorization
\* uses the live transport on a cache miss; push authorization is unchanged.
CONSTANT ReadTransport
VARIABLES connected, observed, deniedLive
vars == <<connected, observed, deniedLive>>
\* This handshake begins before the first connection is accepted.
Init == /\ connected = FALSE /\ observed = FALSE /\ deniedLive = FALSE
\* The transport accepts the authenticated peer first.
Connect == /\ ~connected /\ connected' = TRUE
           /\ UNCHANGED <<observed, deniedLive>>
\* The asynchronous lifecycle notification catches up later.
Observe == /\ connected /\ ~observed /\ observed' = TRUE
           /\ UNCHANGED <<connected, deniedLive>>
\* A pull request can arrive between those steps. The red variant ignores
\* the live transport and records a false denial in that interval.
Pull == /\ deniedLive' = (connected /\ ~(observed \/ (ReadTransport /\ connected)))
        /\ UNCHANGED <<connected, observed>>
Next == Connect \/ Observe \/ Pull
Spec == Init /\ [][Next]_vars
\* A connected peer is never denied just because its notification is queued.
NoSpuriousDenial == ~deniedLive
=============================================================================
