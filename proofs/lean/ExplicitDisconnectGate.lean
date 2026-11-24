/-!
# Explicit disconnect admission

A disconnect marks one peer. A background connection is admitted only when
that peer is unmarked; an explicit reconnect removes its own marker without
changing any other peer's marker. This models the gate's membership decisions,
not the transport or the delivery of connection events.

Rust: `DisconnectGate` in `crates/database/p2p/src/behaviour/disconnect_gate.rs`.
-/
namespace ExplicitDisconnectGate

def Marks := Nat → Bool

def block (marks : Marks) (peer : Nat) : Marks :=
  fun candidate => if candidate = peer then true else marks candidate

def unblock (marks : Marks) (peer : Nat) : Marks :=
  fun candidate => if candidate = peer then false else marks candidate

def admits (marks : Marks) (peer : Nat) : Bool := !(marks peer)

theorem blocked_peer_is_refused (marks : Marks) (peer : Nat) :
    admits (block marks peer) peer = false := by
  simp [admits, block]

theorem explicit_reconnect_is_admitted (marks : Marks) (peer : Nat) :
    admits (unblock marks peer) peer = true := by
  simp [admits, unblock]

theorem other_peer_is_unchanged (marks : Marks) (peer other : Nat) (h : other ≠ peer) :
    unblock marks peer other = marks other ∧ block marks peer other = marks other := by
  simp [block, unblock, h]

end ExplicitDisconnectGate
