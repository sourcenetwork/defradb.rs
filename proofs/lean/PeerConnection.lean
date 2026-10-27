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

-- Each peer has at most one selected shared handle, which may later close.
def Cache := Nat → Option Nat

-- Publication replaces only the authenticated peer's selected handle.
def remember (cache : Cache) (connection : Connection) : Cache :=
  if connection.compatible then
    fun peer => if peer = connection.peer then some connection.handle else cache peer
  else cache

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
    remember cache connection connection.peer = some connection.handle := by
  -- Substituting the authenticated peer takes the replacement branch.
  simp [remember, compatible]

-- Remembering a connection cannot change another peer's selected path.
theorem other_peer_is_unchanged (cache : Cache) (connection : Connection) (peer : Nat)
    (different : peer ≠ connection.peer) :
    remember cache connection peer = cache peer := by
  -- Distinct peer labels take the branch which retains the old selection.
  cases compatible : connection.compatible <;> simp [remember, compatible, different]

-- A legacy mux cannot replace the selected modern protocol connection.
theorem incompatible_does_not_replace (cache : Cache) (connection : Connection)
    (legacy : connection.compatible = false) : remember cache connection = cache := by
  -- Legacy streams retain their separate reply path and leave this cache alone.
  simp [remember, legacy]

-- A cached handle grants no ability to send to a revoked peer.
theorem revoked_cannot_borrow (transportOpen : Bool) (selected : Option Nat) :
    usable true transportOpen selected = none := by
  -- Revocation wins independently of the transport's open observation.
  simp [usable]

-- A peer which remains admitted borrows the replacement without a second dial.
theorem admitted_replacement_is_usable (cache : Cache) (connection : Connection)
    (compatible : connection.compatible = true) :
    usable false true (remember cache connection connection.peer) = some connection.handle := by
  -- Acceptance picks this handle; neither of the refusal observations applies.
  simp [usable, remember, compatible]

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
