/-!
Saved rooted walks are an optional cache. A reservation has to fit before it
is saved, and releasing it restores its charge. A stale eviction may remove
only the exact cursor it observed. Neither saving nor refusing to save a
cursor changes the grants returned by the walk.

Rust: car_authorization_budget.rs and car_authorization_registry.rs.
The concurrent steps are modeled by RootedAuthorization.tla.
-/
namespace RootedAuthorization

-- Reserving returns the new charge only when it fits the shared limit.
def reserve (used cost limit : Nat) : Option Nat :=
  if used + cost <= limit then some (used + cost) else none

-- A successful reservation stays within the shared limit.
theorem reserve_bounded (used cost limit next : Nat)
    (h : reserve used cost limit = some next) : next <= limit := by
  -- Split on the very same fit check made by the reservation.
  unfold reserve at h
  split at h
  -- Success names exactly the checked sum.
  · cases h; assumption
  -- Failure cannot equal a successful reservation.
  · cases h

-- Releasing a reservation returns exactly the charge that preceded it.
theorem release_restores (used cost : Nat) : used + cost - cost = used := by
  -- Adding and subtracting the same natural amount cancel.
  exact Nat.add_sub_cancel used cost

-- Maintenance removes a cursor only when its generation is the one observed.
def evict (current : Option Nat) (observed : Nat) : Option Nat :=
  if current = some observed then none else current

-- An observation of a predecessor leaves its replacement intact.
theorem replacement_survives (current observed : Nat) (h : Not (current = observed)) :
    evict (some current) observed = some current := by
  -- Unequal identities do not satisfy the removal predicate.
  simp [evict, h]

-- A walk returns its grants alongside an optional reservation, not instead of them.
def finish (grants : List Nat) (used cost limit : Nat) : List Nat × Option Nat :=
  (grants, reserve used cost limit)

-- Cache pressure cannot remove any of the walk's reached grants.
theorem grants_independent_of_retention (grants : List Nat) (used cost limit : Nat) :
    (finish grants used cost limit).1 = grants := by
  -- The cache's fit check affects only the second component.
  rfl

end RootedAuthorization
