/-!
A selective-transfer cache keeps active sends and completed sends together.
Completed sends are a warm cache: pressure may reclaim them, but never an
active send. With no pressure their warm access stays intact. Durable rooted
authorization remains the recovery owner after a warm entry is reclaimed.
Rust: sync/coordinator/selective_car_access.rs register and allows_root.
Concurrent admission, completion and expiry are in SelectiveCarCapacity.tla.
-/
namespace SelectiveCarCapacity

-- Admit one active send and keep every completed entry that still fits.
def register (active warm limit : Nat) : Option (Nat × Nat) :=
  if active < limit then some (active + 1, min warm (limit - active - 1)) else none

-- Completed entries cannot refuse a send when active sends leave room.
theorem warm_does_not_refuse (active warm limit : Nat) (h : active < limit) :
    ∃ next, register active warm limit = some next := by
  -- The admission guard depends on active sends, not warm entries.
  simp [register, h]

-- Admission adds exactly the new active send, without evicting an old one.
theorem active_preserved (active warm limit : Nat) (next : Nat × Nat)
    (h : register active warm limit = some next) : next.1 = active + 1 := by
  -- A successful result is the checked pair.
  unfold register at h
  split at h
  · cases h; rfl
  · cases h

-- The active and retained completed entries together fit the same limit.
theorem admitted_bounded (active warm limit : Nat) (next : Nat × Nat)
    (h : register active warm limit = some next) : next.1 + next.2 ≤ limit := by
  -- The retained count is at most the room left by active sends.
  unfold register at h
  split at h
  · cases h
    have retained := Nat.min_le_right warm (limit - active - 1)
    omega
  · cases h

-- Without pressure the completed cache is unchanged.
theorem warm_preserved_without_pressure (active warm limit : Nat)
    (h : active + warm < limit) : register active warm limit = some (active + 1, warm) := by
  -- Every old warm entry fits after adding the new active send.
  have fits : active < limit := by omega
  have room : warm ≤ limit - active - 1 := by omega
  simp [register, fits, Nat.min_eq_left room]

-- A limit occupied entirely by active sends cannot register another send.
theorem full_active_refused (active warm limit : Nat) (h : limit ≤ active) :
    register active warm limit = none := by
  -- No completed entry is allowed to replace an active capability.
  simp [register, Nat.not_lt.mpr h]

-- A grant names one peer and exact root; only completed grants can expire.
structure Grant where
  id : Nat
  peer : Nat
  root : Nat
  active : Bool
  expired : Bool
  deriving DecidableEq

-- Reclaiming an observed completed ID never erases an active capability.
def reclaim (cache : List Grant) (victim : Nat) : List Grant :=
  cache.filter (fun grant => grant.id != victim || grant.active)

-- Every active grant survives a completed-entry reclamation.
theorem reclaim_preserves_active (cache : List Grant) (victim : Nat) (grant : Grant)
    (member : grant ∈ cache) (active : grant.active = true) :
    grant ∈ reclaim cache victim := by
  -- The active part of the retention predicate is true for this grant.
  simp [reclaim, List.mem_filter, member, active]

-- Cached authority stays scoped to the admitted peer and exact root.
def authorized (grant : Grant) (peer root : Nat) : Prop :=
  grant.peer = peer ∧ grant.root = root ∧ (grant.active = true ∨ grant.expired = false)

-- A cache hit cannot cross peer or root scope.
theorem authorized_scope (grant : Grant) (peer root : Nat)
    (h : authorized grant peer root) : grant.peer = peer ∧ grant.root = root := by
  -- Both scopes are required by the same cache predicate.
  exact ⟨h.1, h.2.1⟩

-- Expired completed entries cannot grant access.
theorem expired_completed_refused (grant : Grant) (peer root : Nat)
    (active : grant.active = false) (expired : grant.expired = true) :
    ¬authorized grant peer root := by
  -- Neither active authority nor a live completed window remains.
  simp [authorized, active, expired]

end SelectiveCarCapacity
