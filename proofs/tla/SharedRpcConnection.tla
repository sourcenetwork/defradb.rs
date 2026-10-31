------------------------ MODULE SharedRpcConnection ------------------------
\* Accepted authenticated paths become available to reverse RPC before the
\* final admission check. Revocation must close late publication; an older
\* failure cannot remove a replacement. This model names the modern slot;
\* LegacyRpcCache covers reuse and selection of the separate legacy fallback.
EXTENDS TLC, Integers
\* Each switch removes one runtime safeguard for its negative control.
CONSTANTS AdoptInbound, PostCheck, OutboundCheck, CompareEviction, CheckMux, RetireClosed
\* Slots name handles: zero is empty, one is old, and two is newly accepted.
VARIABLES phase, cached, oldOpen, newOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming
\* The complete state includes publication, transport observations and cleanup.
vars == <<phase, cached, oldOpen, newOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* Acceptance has not started.
Init == /\ phase = "start"
        \* Exercise both first-inbound adoption and replacement of an old path.
        /\ cached \in {0, 1}
        \* Only the old-cache scenario starts with an older transport.
        /\ oldOpen = (cached = 1)
        \* No new transport exists yet.
        /\ newOpen = FALSE
        \* Revocation cannot see a new peer-map registration yet.
        /\ registered = FALSE
        \* The peer begins admitted.
        /\ revoked = FALSE
        \* No request has borrowed a revoked path.
        /\ badBorrow = FALSE
        \* No old request has reported its delayed failure.
        /\ lateFailure = FALSE
        \* The negotiated mux is chosen when the connection arrives.
        /\ legacy = FALSE
        \* The new stream loop has not closed.
        /\ closedIncoming = FALSE
\* QUIC has authenticated a connection before the first admission check.
Accept == /\ phase = "start"
          \* A previously revoked peer is not accepted.
          /\ ~revoked
          \* The first admission check has completed.
          /\ phase' = "checked"
          \* The newly accepted transport is open.
          /\ newOpen' = TRUE
          \* Either supported mux version may be negotiated.
          /\ legacy' \in {TRUE, FALSE}
          \* Acceptance alone changes neither cache selection nor revocation.
          /\ UNCHANGED <<cached, oldOpen, registered, revoked, badBorrow, lateFailure, closedIncoming>>
\* Both retained handles are published before the final admission check.
Publish == /\ phase = "checked"
           \* Publication precedes the final check.
           /\ phase' = "published"
           \* Revocation can now find the new handle in the peer map.
           /\ registered' = TRUE
           \* Only a modern connection belongs in the modern slot.
           /\ cached' = IF AdoptInbound /\ (~CheckMux \/ ~legacy) THEN 2 ELSE cached
           \* Publication grants no permission and closes no transport.
           /\ UNCHANGED <<oldOpen, newOpen, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* Recheck admission after publishing so a missed revoke cannot leave a live path.
Fence == /\ phase = "published"
         \* Acceptance is complete after this recheck.
         /\ phase' = "finished"
         \* A revoke observed here closes the new transport.
         /\ newOpen' = IF PostCheck /\ revoked THEN FALSE ELSE newOpen
         \* Rechecking changes no other owner.
         /\ UNCHANGED <<cached, oldOpen, registered, revoked, badBorrow, lateFailure, legacy, closedIncoming>>
\* The existing revocation owner closes every handle already published to it.
Revoke == /\ ~revoked
          \* Subsequent requests must see refusal.
          /\ revoked' = TRUE
          \* The older known path is closed.
          /\ oldOpen' = FALSE
          \* An unpublished handle can only be closed by the later recheck.
          /\ newOpen' = IF registered THEN FALSE ELSE newOpen
          \* Revocation does not invent cache publication or request completion.
          /\ UNCHANGED <<phase, cached, registered, badBorrow, lateFailure, legacy, closedIncoming>>
\* Borrowing a cache hit still requires an open transport and current admission.
Borrow == /\ cached # 0
          \* The selected handle must be the transport observed open.
          /\ ((cached = 1 /\ oldOpen) \/ (cached = 2 /\ newOpen))
          \* The negative control removes only outbound admission.
          /\ (~OutboundCheck \/ ~revoked)
          \* Remember any request admitted after revocation.
          /\ badBorrow' = (badBorrow \/ revoked)
          \* Borrowing neither selects another handle nor changes transport state.
          /\ UNCHANGED <<phase, cached, oldOpen, newOpen, registered, revoked, lateFailure, legacy, closedIncoming>>
\* An old request may fail after a replacement was already accepted.
OldFailure == /\ phase = "finished"
              \* Report that old failure only once.
              /\ ~lateFailure
              \* Remember that the late completion occurred.
              /\ lateFailure' = TRUE
              \* Eviction compares stable handles rather than just peer labels.
              /\ cached' = IF ~CompareEviction \/ cached = 1 THEN 0 ELSE cached
              \* A failed old request cannot close another transport.
              /\ UNCHANGED <<phase, oldOpen, newOpen, registered, revoked, badBorrow, legacy, closedIncoming>>
\* The incoming stream loop eventually observes its transport closing.
IncomingClose == /\ phase = "finished"
                 \* A single handle is retired only once.
                 /\ ~closedIncoming
                 \* Record the close observation.
                 /\ closedIncoming' = TRUE
                 \* The incoming transport is no longer open.
                 /\ newOpen' = FALSE
                 \* Retire only the matching currently selected handle.
                 /\ cached' = IF RetireClosed /\ cached = 2 THEN 0 ELSE cached
                 \* Other owners and the older transport are untouched.
                 /\ UNCHANGED <<phase, oldOpen, registered, revoked, badBorrow, lateFailure, legacy>>
\* Every existing owner can act between the others' steps.
Next == Accept \/ Publish \/ Fence \/ Revoke \/ Borrow \/ OldFailure \/ IncomingClose
\* Idle periods do not invent networking work.
Spec == Init /\ [][Next]_vars
\* The first modern inbound connection is selected even from an empty cache.
FirstInboundShared == (phase = "published" /\ ~revoked /\ ~legacy) => cached = 2
\* A completed modern acceptance immediately supplies the shared RPC path.
FreshAcceptedShared == (phase = "finished" /\ ~revoked /\ ~lateFailure /\ ~legacy /\ newOpen) => cached = 2
\* Revocation racing acceptance leaves no newly published open transport.
RevokedAcceptanceClosed == (phase = "finished" /\ revoked) => ~newOpen
\* No request can use the cache to bypass outbound revocation.
NoRevokedBorrow == ~badBorrow
\* A late failure of an older handle cannot erase a newer usable path.
ReplacementSurvivesOldFailure == (phase = "finished" /\ ~revoked /\ lateFailure /\ ~legacy /\ newOpen) => cached = 2
\* Legacy publication never installs a legacy handle in the modern slot.
CompatibleCacheOnly == legacy => cached # 2
\* Stream shutdown does not leave its own closed handle selected.
ClosedIncomingRetired == closedIncoming => cached # 2
=============================================================================
