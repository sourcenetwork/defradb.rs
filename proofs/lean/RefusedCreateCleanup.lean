-- Vera can refuse a create whose row and unique reservation already exist.
-- The cleanup spends the refusal only for the short ID named on it.
-- Correspondence: DB::purge_refused_create in access/refused_create.rs.
namespace RefusedCreateCleanup

-- A reservation names its owner, or is free.
abbrev Reservation := Option Nat

-- The same decision applies to the row and its unique reservation.
def release (refused : Nat) (owner : Reservation) : Reservation :=
  if owner = some refused then none else owner

-- A refusal releases the reservations held by its own create.
theorem release_own (refused : Nat) : release refused (some refused) = none := by
  simp [release]

-- Replaying a refusal leaves a free reservation free.
theorem release_twice (refused : Nat) (owner : Reservation) :
    release refused (release refused owner) = release refused owner := by
  by_cases h : owner = some refused <;> simp [release, h]

-- A refusal cannot release a reservation held by a replacement.
theorem release_other (refused replacement : Nat) (different : replacement ≠ refused) :
    release refused (some replacement) = some replacement := by
  simp [release, different]

end RefusedCreateCleanup
