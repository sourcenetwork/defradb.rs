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

-- A stale preallocated identity is a request to decide again, never a storage failure.
inductive AllocationDecision where
  | install
  | reallocate
  deriving DecidableEq

-- Only an allocation at least as new as the durable peer incarnation may install.
def allocationDecision (current allocated : Nat) : AllocationDecision :=
  if allocated < current then .reallocate else .install

-- A newer committed incarnation sends the caller back to its bounded install loop.
theorem stale_allocation_redecides (current allocated : Nat) (h : allocated < current) :
    allocationDecision current allocated = .reallocate := by
  -- The deciding transaction observes the same strict ordering as the runtime.
  simp [allocationDecision, h]

-- Installing a fresh identity cannot move the durable incarnation backwards.
theorem install_preserves_incarnation (current allocated : Nat)
    (h : allocationDecision current allocated = .install) : current ≤ allocated := by
  -- An allocation older than the current incarnation cannot select installation.
  unfold allocationDecision at h
  split at h
  · contradiction
  · exact Nat.le_of_not_lt ‹¬ allocated < current›

-- A replay status update carries the incarnation chosen by its install.
structure ReplayRow where
  incarnation : Nat
  active : Bool
  deriving DecidableEq

-- A missing or replaced row is left alone; a matching row becomes inactive.
def deferReplay (row : Option ReplayRow) (installId : Nat) : Option ReplayRow :=
  match row with
  | none => none
  | some current =>
      if current.incarnation = installId then some { current with active := false } else row

-- Deferring an installed replay preserves the identity used by finish.
theorem defer_preserves_incarnation (row : ReplayRow) (id : Nat) :
    (deferReplay (some row) id).map ReplayRow.incarnation = some row.incarnation := by
  -- Both a matching update and a rejected update keep the original identity.
  simp only [deferReplay]
  split <;> rfl

-- A replay that returns after deletion cannot recreate the peer row.
theorem defer_does_not_resurrect (id : Nat) : deferReplay none id = none := by
  rfl

-- A replay from an older install cannot change a replacement's status.
theorem stale_defer_leaves_replacement (row : ReplayRow) (id : Nat)
    (h : row.incarnation ≠ id) : deferReplay (some row) id = some row := by
  simp [deferReplay, h]

end ReplicatorInstall
