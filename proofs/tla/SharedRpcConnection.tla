------------------------ MODULE SharedRpcConnection ------------------------
EXTENDS TLC, Integers
\* endpoint_streams::handle_incoming publishes an authenticated connection to
\* endpoint_rpc's shared cache before its post-registration admission check.
\* A restarted peer's new connection must replace an older transport-open
\* handle. Revocation still fences every borrow and closes late publication.
CONSTANTS AdoptInbound, PostCheck, OutboundCheck, CompareEviction, CheckMux, RetireClosed
\* Zero means no cached handle; one is the old path and two is the new one.
VARIABLES phase, cached, oldOpen, newOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming
\* These are the complete observations owned by this small connection model.
vars == <<phase, cached, oldOpen, newOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* The old path can look open even when its application has stopped answering.
Init == /\ phase = "start" /\ cached = 1 /\ oldOpen = TRUE /\ newOpen = FALSE
        /\ registered = FALSE /\ revoked = FALSE /\ badBorrow = FALSE /\ lateFailure = FALSE /\ legacy = FALSE /\ closedIncoming = FALSE
\* QUIC authenticates the replacement, then the existing inbound gate admits it.
Accept == /\ phase = "start" /\ ~revoked
          /\ phase' = "checked" /\ newOpen' = TRUE /\ legacy' \in {TRUE, FALSE}
          /\ UNCHANGED <<cached, oldOpen, registered, revoked, badBorrow, lateFailure, closedIncoming>>
\* Publish both retained handles before checking admission again, so revocation
\* either sees the new connection or is seen by the check which follows.
Publish == /\ phase = "checked" /\ phase' = "published" /\ registered' = TRUE
           /\ cached' = IF AdoptInbound /\ (~CheckMux \/ ~legacy) THEN 2 ELSE cached
           /\ UNCHANGED <<oldOpen, newOpen, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* A revoke which ran before publication must still close the new handle.
Fence == /\ phase = "published" /\ phase' = "finished"
         /\ newOpen' = IF PostCheck /\ revoked THEN FALSE ELSE newOpen
         /\ UNCHANGED <<cached, oldOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* The existing revoke owner closes all handles already visible in the peer map.
Revoke == /\ ~revoked /\ revoked' = TRUE /\ oldOpen' = FALSE
          /\ newOpen' = IF registered THEN FALSE ELSE newOpen
          /\ UNCHANGED <<phase, cached, registered, badBorrow, lateFailure, legacy, closedIncoming>>
\* A cache hit only borrows an open handle and never skips outbound admission.
Borrow == /\ cached # 0
          /\ ((cached = 1 /\ oldOpen) \/ (cached = 2 /\ newOpen))
          /\ (~OutboundCheck \/ ~revoked)
          /\ badBorrow' = (badBorrow \/ revoked)
          /\ UNCHANGED <<phase, cached, oldOpen, newOpen, registered, revoked, lateFailure, legacy, closedIncoming>>
\* An old request can report failure after replacement; it may evict only its
\* own handle, never the new connection which now occupies the same peer key.
OldFailure == /\ phase = "finished" /\ ~lateFailure /\ lateFailure' = TRUE
              /\ cached' = IF ~CompareEviction \/ cached = 1 THEN 0 ELSE cached
              /\ UNCHANGED <<phase, oldOpen, newOpen, registered, revoked, badBorrow, legacy, closedIncoming>>
\* Stream shutdown retires only this incoming handle from the shared cache.
IncomingClose == /\ phase = "finished" /\ ~closedIncoming
                 /\ closedIncoming' = TRUE /\ newOpen' = FALSE
                 /\ cached' = IF RetireClosed /\ cached = 2 THEN 0 ELSE cached
                 /\ UNCHANGED <<phase, oldOpen, registered, revoked, badBorrow, lateFailure, legacy>>
\* All races are arbitrary interleavings of these existing owners.
Next == Accept \/ Publish \/ Fence \/ Revoke \/ Borrow \/ OldFailure \/ IncomingClose
\* Stuttering leaves an idle transport unchanged rather than inventing work.
Spec == Init /\ [][Next]_vars
\* An admitted replacement is the shared path as soon as acceptance finishes.
FreshAcceptedShared == (phase = "finished" /\ ~revoked /\ ~lateFailure /\ ~legacy /\ newOpen) => cached = 2
\* A revoke during acceptance leaves no newly published open handle afterward.
RevokedAcceptanceClosed == (phase = "finished" /\ revoked) => ~newOpen
\* Revocation is checked even when a live connection is already cached.
NoRevokedBorrow == ~badBorrow
\* Completion of an older request cannot erase the replacement it did not use.
ReplacementSurvivesOldFailure == (phase = "finished" /\ ~revoked /\ lateFailure /\ ~legacy /\ newOpen) => cached = 2
\* Legacy mux replies use another stream and cannot become the modern RPC path.
CompatibleCacheOnly == legacy => cached # 2
\* Disconnect observation never retains the closed incoming path in the cache.
ClosedIncomingRetired == closedIncoming => cached # 2
=============================================================================
