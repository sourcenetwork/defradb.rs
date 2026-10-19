/-!
An install has one identity and one pending outcome. A finish changes only its
own pending record. Repeating it, or finishing an older identity, changes nothing.
Rust: replicator_install_store.rs. ReplicatorInstall.tla models concurrent steps.
-/
namespace ReplicatorInstall

-- A record is either still running or carries its terminal failure/success.
structure Record where
  id : Nat
  pending : Bool
  failure : Option String
  deriving DecidableEq

-- The finish transaction compares both the identity and pending flag.
def finish (r : Record) (id : Nat) (failure : Option String) : Record :=
  if r.id = id ∧ r.pending = true then { r with pending := false, failure } else r

-- A completed install is never completed again.
theorem finish_once (r : Record) (id : Nat) (failure : Option String) :
    finish (finish r id failure) id failure = finish r id failure := by
  -- Either the first finish matches, or both finishes leave the record untouched.
  unfold finish
  split <;> simp_all

-- A delayed finish from an older install cannot change its replacement.
theorem stale_finish (r : Record) (id : Nat) (failure : Option String)
    (h : r.id ≠ id) : finish r id failure = r := by
  -- The identity part of the transaction's predicate is false.
  simp [finish, h]

-- A completed record keeps its outcome even if a duplicate supplies another one.
theorem outcome_stable (r : Record) (id : Nat) (failure : Option String)
    (h : r.pending = false) : finish r id failure = r := by
  -- Completed records fail the pending part of the predicate.
  simp [finish, h]

end ReplicatorInstall
