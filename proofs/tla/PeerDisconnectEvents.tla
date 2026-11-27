----------------------- MODULE PeerDisconnectEvents -----------------------
\* A host can announce a peer connection or reject it before announcement.
\* Closing the last connection must emit a disconnect only for a peer whose
\* connection was announced. Otherwise consumers see a spurious disconnect.
\* Rust: crates/database/p2p/src/host/p2p_host/swarm.rs
\* P2PHost::handle_connection_closed.
EXTENDS Naturals
\* EmitUnannounced plants the old unconditional disconnect event.
CONSTANT EmitUnannounced
\* announced says the connected event was sent; bad records a spurious event.
VARIABLES announced, bad
\* Both variables are part of the state.
vars == <<announced, bad>>
\* No connection has been announced or incorrectly closed.
Init == /\ announced = FALSE /\ bad = FALSE
\* A connection passes admission and is announced.
Open == /\ ~announced /\ announced' = TRUE /\ UNCHANGED bad
\* Closing the last connection emits an event only when it was announced.
Close == /\ bad' = (bad \/ (EmitUnannounced /\ ~announced))
         /\ announced' = FALSE
\* The host handles either event.
Next == Open \/ Close
\* The system may also stutter.
Spec == Init /\ [][Next]_vars
\* An unannounced connection never causes a disconnect event.
INV_NoSpuriousDisconnect == ~bad
=============================================================================
