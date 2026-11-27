----------------------- MODULE GenericDisconnectDial -----------------------
\* A caller explicitly disconnects a peer. A background dial can be issued
\* directly, without a discovery list. It must not clear that marker; only an
\* explicit local dial can clear it and permit reconnection.
\* Rust: crates/database/p2p/src/host/command_handler/network.rs handle_dial,
\* handle_explicit_dial, handle_disconnect.
EXTENDS Naturals
\* ClearOnGeneric plants the old generic-dial behavior.
CONSTANT ClearOnGeneric
\* marked records the explicit disconnect; bad records its generic removal.
VARIABLES marked, bad
\* Both variables form the state.
vars == <<marked, bad>>
\* The peer was deliberately disconnected before this trace.
Init == /\ marked = TRUE /\ bad = FALSE
\* A background dial must leave a marked peer alone.
GenericDial == /\ IF ClearOnGeneric /\ marked
                     THEN /\ marked' = FALSE /\ bad' = TRUE
                     ELSE /\ UNCHANGED <<marked, bad>>
\* An explicit local dial is allowed to clear the marker.
ExplicitDial == /\ marked /\ marked' = FALSE /\ UNCHANGED bad
\* Either kind of dial may be issued.
Next == GenericDial \/ ExplicitDial
\* The execution may also stutter.
Spec == Init /\ [][Next]_vars
\* Generic dialing never removes the explicit disconnect marker.
INV_GenericKeepsMarker == ~bad
=============================================================================
