/-!
The story. A node serves CAR files to peers catching up on its history, but
only `capacity` serves run at once: that is the serve lane. A request that
finds the lane full waits in a waiting room no larger than the lane, unless
the room (or its peer's share of it) is full, and then it is answered busy.
A finished serve hands its slot straight to a waiting request. A waiting
request whose deadline passes leaves the room and is answered busy.

What this file promises: whatever happens, the lane never counts more serves
than it has slots, the room never holds more than its bound, and no request
waits while a slot is free. TLA+ (`proofs/tla/CarServeAdmission.tla`) checks
the races between peers and which peer a freed slot goes to.

Rust owner: crates/database/p2p/src/sync/car_serve_admission.rs
ServeAdmission::offer, ServeAdmission::release, ServeAdmission::pop_expired.
-/

namespace CarServeAdmission

-- The lane as counts: its slots, the serves running, the requests waiting.
structure Lane where
  -- How many serves may run at once (`capacity`, at least one in Rust).
  capacity : Nat
  -- How many serves run now (`active_total`).
  active : Nat
  -- How many requests wait in the room (`waiting.len()`).
  waiting : Nat

-- A lane is healthy when it runs no more than its slots, its room holds no
-- more than the lane has slots, and a request waits only while all slots run.
def Lane.Healthy (l : Lane) : Prop :=
  l.active ≤ l.capacity ∧ l.waiting ≤ l.capacity ∧ (0 < l.waiting → l.active = l.capacity)

-- What became of an offered request (`ServeOffer`).
inductive Offer where
  -- Served at once.
  | admitted
  -- Waiting in the room.
  | waiting
  -- Answered busy.
  | shed
  deriving DecidableEq

-- Offer a request. `peerHasShare` says whether its peer has a place left in
-- its quarter of the room. A free slot serves it; otherwise it waits if the
-- room and its peer's share have space; otherwise it is shed.
def offer (l : Lane) (peerHasShare : Bool) : Offer × Lane :=
  if l.active < l.capacity then (.admitted, { l with active := l.active + 1 })
  else if l.waiting < l.capacity ∧ peerHasShare = true then
    (.waiting, { l with waiting := l.waiting + 1 })
  else (.shed, l)

-- A serve ends. With a request waiting, the slot passes to it (the result is
-- `true`, and the running count stays); otherwise the slot is just returned.
def release (l : Lane) : Bool × Lane :=
  if 0 < l.waiting then (true, { l with waiting := l.waiting - 1 })
  else (false, { l with active := l.active - 1 })

-- A waiting request's deadline passes: it leaves the room.
def expire (l : Lane) : Lane := { l with waiting := l.waiting - 1 }

-- An empty lane is healthy.
theorem empty_healthy (c : Nat) : (Lane.mk c 0 0).Healthy := by
  simp [Lane.Healthy]

-- Offering a request keeps a healthy lane healthy.
theorem offer_healthy (l : Lane) (s : Bool) (h : l.Healthy) : (offer l s).2.Healthy := by
  obtain ⟨ha, hw, hf⟩ := h
  unfold offer
  split
  · -- A free slot: it is taken, so no request can be waiting beside it.
    refine ⟨by simp; omega, by simpa using hw, ?_⟩
    intro hpos
    have := hf (by simpa using hpos)
    simp; omega
  · split
    · -- The lane is full: the request joins a room that had space.
      refine ⟨by simpa using ha, by simp; omega, ?_⟩
      intro _
      simp; omega
    · -- Shed: nothing changes.
      exact ⟨ha, hw, hf⟩

-- A request waits only when every slot is running.
theorem offer_waits_only_when_full (l : Lane) (s : Bool) (h : l.Healthy)
    (hw : (offer l s).1 = .waiting) : l.active = l.capacity := by
  obtain ⟨ha, _, _⟩ := h
  unfold offer at hw
  split at hw
  · simp at hw
  · omega

-- A request that finds a free slot is always served at once.
theorem offer_admits_when_free (l : Lane) (s : Bool) (free : l.active < l.capacity) :
    (offer l s).1 = .admitted := by
  simp [offer, free]

-- A request is shed only when the lane is full and the room, or its peer's
-- share of the room, is full too.
theorem offer_sheds_only_when_room_full (l : Lane) (s : Bool)
    (hs : (offer l s).1 = .shed) :
    l.capacity ≤ l.active ∧ (l.capacity ≤ l.waiting ∨ s = false) := by
  unfold offer at hs
  split at hs
  · simp at hs
  · split at hs
    · simp at hs
    · rename_i full room
      refine ⟨by omega, ?_⟩
      cases s <;> simp_all <;> omega

-- Ending a serve keeps a healthy lane healthy.
theorem release_healthy (l : Lane) (h : l.Healthy) : (release l).2.Healthy := by
  obtain ⟨ha, hw, hf⟩ := h
  unfold release
  split
  · -- The slot passes to a waiting request: the lane stays full.
    rename_i hpos
    have full := hf hpos
    exact ⟨by simpa using ha, by simp; omega, fun _ => by simpa using full⟩
  · -- Nobody waits: the slot is returned and the room stays empty.
    rename_i hnone
    refine ⟨by simp; omega, by simp; omega, ?_⟩
    intro hpos
    simp at hpos
    omega

-- A freed slot never sits idle while a request waits.
theorem release_grants_a_waiter (l : Lane) (waiting : 0 < l.waiting) :
    (release l).1 = true := by
  simp [release, waiting]

-- A waiting request whose deadline passes keeps a healthy lane healthy.
theorem expire_healthy (l : Lane) (h : l.Healthy) : (expire l).Healthy := by
  obtain ⟨ha, hw, hf⟩ := h
  refine ⟨by simpa [expire] using ha, by simp [expire]; omega, ?_⟩
  intro hpos
  simp [expire] at hpos ⊢
  exact hf (by omega)

end CarServeAdmission
