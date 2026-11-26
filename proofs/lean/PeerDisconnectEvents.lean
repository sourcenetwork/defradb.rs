/-!
A connection rejected before it was announced must not produce a peer
Disconnect event. A previously announced peer does produce one when its last
connection closes.

Rust owner: crates/database/p2p/src/host/p2p_host/swarm.rs
P2PHost::handle_connection_closed.
-/
namespace PeerDisconnectEvents

-- Whether the last close emits a disconnect for an announced peer.
def emitsDisconnect (announced : Bool) : Bool := announced

-- A rejected, unannounced connection has no disconnect event.
theorem unannounced_is_silent : emitsDisconnect false = false := rfl

-- The last close of an announced peer has a disconnect event.
theorem announced_is_reported : emitsDisconnect true = true := rfl
end PeerDisconnectEvents
