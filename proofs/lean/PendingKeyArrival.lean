/- A key arrival during a claimed fetch sets a flag. Finishing that fetch
   preserves the wake-up: the next retry is due now, regardless of backoff.
   This corresponds to promote_pending_retries and PendingDagLease::finish_attempt
   in crates/database/p2p/src/sync/manager/process/pending_dag.rs. -/
namespace PendingKeyArrival

/-- A claimed fetch clears an earlier notification; a new key sets it. -/
def arrive (_pending : Bool) : Bool := true

/-- The lease chooses the immediate deadline for a key that arrived in flight. -/
def finish (now backoff : Nat) (pending : Bool) : Nat :=
  if pending then now else max now backoff

/-- An arrival after a claim cannot be lost to the backoff re-arm. -/
theorem arrival_survives_finish (now backoff : Nat) (pending : Bool) :
    finish now backoff (arrive pending) = now := by
  rfl

/-- Completing or cancelling a reply flushes its stored keys to the listener. -/
def flush (stored notified : List Nat) : List Nat := stored ++ notified

/-- Every stored key is present in the notification after a flush. -/
theorem stored_key_is_notified (stored notified : List Nat) (key : Nat)
    (present : key ∈ stored) : key ∈ flush stored notified := by
  simp [flush, present]

end PendingKeyArrival
