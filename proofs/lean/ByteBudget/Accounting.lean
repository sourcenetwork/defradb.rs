/-!
# ByteBudget.Accounting: the inbound byte budget never loses or invents bytes

The story, in plain words. Before a node reads a message body into memory, it
takes room for that many bytes from two pools: the budget of the message's
class, and the share its connection may use of that class. It keeps that room
(a lease) until the message has been handled, then gives it back.

What must never happen:
* a pool hands out more bytes than it has (`acquire_within_capacity`);
* free bytes plus leased bytes stop adding up to the pool's size
  (`acquire_conserves`, `release_conserves`);
* a failed admission keeps bytes from either pool (`admit_failure_keeps_nothing`);
* bytes stay missing once every lease has been given back (`release_all_restores`).

The temporal story (stuck handlers, stalled senders, deadlines) is the TLA+ model
`proofs/tla/InboundAdmission.tla`; this file carries the arithmetic that model
takes on trust by giving every body size 1.

Rust anchors: `InboundBudget::admit` and `InboundLease` (its `Drop` gives the
room back) in `crates/p2p/src/iroh/inbound_budget.rs`. Each pool is a
`regolith::sync::Semaphore` counted in bytes; it is modeled here as its free count.
-/

namespace ByteBudget

-- Sum of the bytes held by a list of leases.
def held (leases : List Nat) : Nat := leases.foldr (· + ·) 0

-- Take `n` bytes from a pool with `free` bytes: nothing if there is not enough room,
-- else the pool's new free count.
def tryAcquire (free n : Nat) : Option Nat :=
  -- Enough room: the pool loses exactly `n` free bytes;
  if n ≤ free then some (free - n)
  -- not enough: nothing is taken.
  else none

-- Give `n` bytes back to a pool.
def release (free n : Nat) : Nat := free + n

-- Promise: a successful take leaves free plus held equal to the pool's size.
theorem acquire_conserves (cap free n free' : Nat) (leases : List Nat)
    -- Given the pool adds up before the take,
    (hinv : free + held leases = cap)
    -- and the take of `n` bytes succeeded,
    (h : tryAcquire free n = some free') :
    -- the new lease joins the held list and the pool still adds up.
    free' + held (n :: leases) = cap := by
  -- Unfold the take and split on whether there was room.
  unfold tryAcquire at h
  split at h
  -- There was room: the new free count is `free - n`, and arithmetic closes it.
  · cases h; simp only [held, List.foldr] at *; omega
  -- There was no room: the take returned nothing, which contradicts success.
  · cases h

-- Promise: a successful take never leaves more held than the pool's size.
theorem acquire_within_capacity (cap free n free' : Nat) (leases : List Nat)
    -- Given the pool adds up and the take succeeded,
    (hinv : free + held leases = cap)
    (h : tryAcquire free n = some free') :
    -- everything held, the new lease included, fits in the pool.
    held (n :: leases) ≤ cap := by
  -- It follows from conservation: held is cap minus a free count.
  have := acquire_conserves cap free n free' leases hinv h
  omega

-- Promise: giving a lease back keeps the pool adding up.
theorem release_conserves (cap free n : Nat) (leases : List Nat)
    -- Given the pool adds up while the lease of `n` bytes is held,
    (hinv : free + held (n :: leases) = cap) :
    -- after giving it back, the free count rises by `n` and the pool still adds up.
    release free n + held leases = cap := by
  -- Unfold both sides; it is plain addition.
  simp only [release, held, List.foldr] at *
  omega

-- Promise: once every lease is given back, the pool is full again.
theorem release_all_restores (cap free : Nat) (leases : List Nat)
    -- Given the pool adds up with these leases held,
    (hinv : free + held leases = cap) :
    -- giving them all back, one at a time, leaves `cap` free bytes.
    leases.foldl release free = cap := by
  -- Induct on the leases, carrying the free count along.
  induction leases generalizing free with
  -- No leases held: free is already cap.
  | nil => simpa [held] using hinv
  -- One lease then the rest: give the first back and use the rest's promise.
  | cons n rest ih =>
    -- The rest's promise needs the pool to add up after the first release.
    apply ih
    -- That is release_conserves for the first lease.
    simpa [release] using release_conserves cap free n rest hinv

-- Admission takes from the connection share first, then from the class budget.
-- It succeeds only if both have room; if the class has none, the share is given back.
def admitBoth (share cls n : Nat) : Option (Nat × Nat) :=
  -- Take from the share;
  match tryAcquire share n with
  -- no room in the share: nothing is taken from either pool.
  | none => none
  -- room in the share: now take from the class budget,
  | some share' =>
    match tryAcquire cls n with
    -- no room in the class: the share's bytes go back, so nothing is kept;
    | none => none
    -- room in both: both pools lose `n` free bytes.
    | some cls' => some (share', cls')

-- Promise: a failed admission keeps no bytes, which means both pools are as they were.
-- In Rust this is the share permit dropping on the early return.
theorem admit_failure_keeps_nothing (share cls n : Nat)
    -- Given the admission failed,
    (h : admitBoth share cls n = none) :
    -- one of the two pools did not have room for `n` bytes.
    n > share ∨ n > cls := by
  -- Look at both cases of "was there room in the share?".
  by_cases hs : n ≤ share
  -- Room in the share: then the class must have had none, or admission would succeed.
  · by_cases hc : n ≤ cls
    -- Room in both contradicts the failure we were given.
    · simp [admitBoth, tryAcquire, hs, hc] at h
    -- No room in the class: that is the right side of the promise.
    · exact Or.inr (by omega)
  -- No room in the share: that is the left side of the promise.
  · exact Or.inl (by omega)

-- Promise: a successful admission takes exactly `n` from each pool.
theorem admit_success_takes_n (share cls n share' cls' : Nat)
    -- Given the admission succeeded with new free counts `share'` and `cls'`,
    (h : admitBoth share cls n = some (share', cls')) :
    -- both pools had room and each lost exactly `n` free bytes.
    n ≤ share ∧ n ≤ cls ∧ share' = share - n ∧ cls' = cls - n := by
  -- Look at both cases of "was there room?" for each pool.
  by_cases hs : n ≤ share <;> by_cases hc : n ≤ cls
  -- Room in both: admission returns `share - n` and `cls - n`, which is the promise.
  · simp [admitBoth, tryAcquire, hs, hc] at h; omega
  -- Any pool without room makes admission fail, which contradicts the success we were given.
  all_goals simp [admitBoth, tryAcquire, hs, hc] at h

end ByteBudget
