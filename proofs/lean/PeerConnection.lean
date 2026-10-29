/- Connected peers may pull heads. A queued PeerConnected event must not
   hide a live authenticated connection; it never grants push permission.
   Models RuntimeAuthorizer.peer_connected and check_pushlog_access_str. -/
namespace PeerConnection

/-- Pull connection evidence includes the live transport on a cache miss. -/
def connected (observed live : Bool) : Bool := observed || live

/-- Push permission still requires its independent collection grant. -/
def mayPush (authorized : Bool) : Bool := authorized

/-- A live transport connection is sufficient even before observation. -/
theorem live_is_connected (observed : Bool) : connected observed true = true := by
  cases observed <;> rfl

/-- No connection evidence grants nothing. -/
theorem absent_is_not_connected : connected false false = false := rfl

/-- An authenticated connection does not grant push permission. -/
theorem connection_does_not_grant_push : mayPush false = false := rfl

end PeerConnection

/- endpoint_rpc::remember_connection keys an authenticated handle by its own
   remote peer. endpoint_streams::handle_incoming publishes that handle before
   rechecking admission. These rules choose a path; they confer no authority.
   The cache below names handles, while `usable` separately observes closure. -/
namespace PeerConnection.SharedRpc

-- QUIC supplies both the remote peer and this connection's stable handle.
structure Connection where
  -- A peer label is authenticated by the handshake, never a request payload.
  peer : Nat
  -- A handle distinguishes older and newer connections for the same peer.
  handle : Nat
  -- Only the current mux sends replies on the original RPC stream.
  compatible : Bool

-- Each protocol retains a path; a modern path always wins selection.
structure Paths where
  -- The modern mux is preferred while it remains open.
  modern : Option Nat
  -- A legacy-only peer can still reuse its authenticated connection.
  legacy : Option Nat

-- Connections are kept under their authenticated peer labels.
def Cache := Nat → Paths

-- A modern path takes precedence over the legacy fallback.
def selected (paths : Paths) : Option Nat := paths.modern.or paths.legacy

-- Publication replaces only the authenticated peer's selected handle.
def remember (cache : Cache) (connection : Connection) : Cache :=
  fun peer =>
    -- A publication cannot affect another authenticated peer.
    if peer = connection.peer then
      -- Modern and legacy publications change only their own slots.
      if connection.compatible then { cache peer with modern := some connection.handle }
      else { cache peer with legacy := some connection.handle }
    -- All unrelated peers retain their exact prior paths.
    else cache peer

-- Closure and current revocation are checked independently of cache selection.
def usable (revoked transportOpen : Bool) (selected : Option Nat) : Option Nat :=
  if revoked || !transportOpen then none else selected

-- A revoke missed by publication must be observed by the final acceptance check.
def finishAcceptance (revoked transportOpen : Bool) : Bool := transportOpen && !revoked

-- A failing request can remove its own handle, never a newer selected handle.
def evict (selected : Option Nat) (failedHandle : Nat) : Option Nat :=
  if selected = some failedHandle then none else selected

-- An accepted replacement immediately becomes the shared handle of its peer.
theorem replacement_is_selected (cache : Cache) (connection : Connection)
    (compatible : connection.compatible = true) :
    selected (remember cache connection connection.peer) = some connection.handle := by
  -- Substituting the authenticated peer takes the replacement branch.
  simp [selected, remember, compatible, Option.or]

-- Remembering a connection cannot change another peer's selected path.
theorem other_peer_is_unchanged (cache : Cache) (connection : Connection) (peer : Nat)
    (different : peer ≠ connection.peer) :
    remember cache connection peer = cache peer := by
  -- Distinct peer labels take the branch which retains the old selection.
  cases compatible : connection.compatible <;> simp [remember, compatible, different]

-- A legacy mux cannot replace the selected modern protocol connection.
theorem incompatible_does_not_replace (cache : Cache) (connection : Connection)
    (legacy : connection.compatible = false) :
    (remember cache connection connection.peer).modern = (cache connection.peer).modern := by
  -- A legacy publication changes only the fallback slot.
  simp [remember, legacy]

-- An empty modern slot still permits reuse of an authenticated legacy path.
theorem legacy_is_reused (cache : Cache) (connection : Connection)
    (legacy : connection.compatible = false)
    (empty : (cache connection.peer).modern = none) :
    selected (remember cache connection connection.peer) = some connection.handle := by
  -- The legacy slot is selected only because the modern slot is empty.
  simp [selected, remember, legacy, empty, Option.or]

-- A modern path remains selected when another legacy connection arrives.
theorem modern_is_preferred (cache : Cache) (connection : Connection) (handle : Nat)
    (legacy : connection.compatible = false)
    (present : (cache connection.peer).modern = some handle) :
    selected (remember cache connection connection.peer) = some handle := by
  -- Selection sees the unchanged modern slot before consulting the fallback.
  simp [selected, remember, legacy, present, Option.or]

-- A cached handle grants no ability to send to a revoked peer.
theorem revoked_cannot_borrow (transportOpen : Bool) (selected : Option Nat) :
    usable true transportOpen selected = none := by
  -- Revocation wins independently of the transport's open observation.
  simp [usable]

-- A peer which remains admitted borrows the replacement without a second dial.
theorem admitted_replacement_is_usable (cache : Cache) (connection : Connection)
    (compatible : connection.compatible = true) :
    usable false true (selected (remember cache connection connection.peer)) = some connection.handle := by
  -- Acceptance picks this handle; neither of the refusal observations applies.
  simp [usable, selected, remember, compatible, Option.or]

-- The final check closes a connection published after revocation ran.
theorem revoke_during_acceptance_closes (transportOpen : Bool) :
    finishAcceptance true transportOpen = false := by
  -- An observed revoke makes the final open flag false for either input.
  simp [finishAcceptance]

-- An old failure cannot erase a replacement which has a different stable id.
theorem old_failure_preserves_replacement (old fresh : Nat) (different : fresh ≠ old) :
    evict (some fresh) old = some fresh := by
  -- The selected handle is different, so eviction leaves it alone.
  simp [evict, different]

-- Closing the currently selected handle removes that handle from the cache.
theorem closed_selected_is_removed (handle : Nat) : evict (some handle) handle = none := by
  -- The selected stable id matches the closed handle, so its entry is retired.
  simp [evict]

end PeerConnection.SharedRpc
