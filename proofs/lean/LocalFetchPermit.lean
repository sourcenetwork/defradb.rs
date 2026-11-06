/-!
A completed local DAG can wait for the merge channel to make room. The fetch
returns its reserved peer permit before that wait, without changing the channel.
Rust: p2p/src/sync/coordinator/dag_fetcher.rs `poll_fetch_dag`.
Concurrency and the second fetch are modeled by `LocalFetchPermit.tla`.
-/
namespace LocalFetchPermit

-- The context's Option<OwnedSemaphorePermit> and the bounded queue's state.
structure State where
  held : Bool
  queueFull : Bool

-- Taking and dropping the option returns the context's permit.
def release (s : State) : State := { s with held := false }

-- A locally complete fetch has no peer permit when it starts the send.
theorem released_before_send (s : State) : (release s).held = false := by
  rfl

-- Releasing a permit cannot itself make room in the merge queue.
theorem release_preserves_queue (s : State) : (release s).queueFull = s.queueFull := by
  rfl

end LocalFetchPermit
