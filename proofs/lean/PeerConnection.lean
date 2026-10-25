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
