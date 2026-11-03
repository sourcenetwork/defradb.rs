import RetrySchedule.Ladder

/-!
# RetrySchedule.Delivery: a delivery returns a peer to the first rung, never later

The story, in plain words. Every document a node still owes a replicator peer
waits on one clock shared by the whole peer: how many retries it has charged
(its rung on the ladder), when it is next due, and the receiver's own
backpressure deadline, which no retry may come before. A retry pass that
delivers nothing climbs one rung (`bump`). When the peer acknowledges a
delivery, the clock goes back to what a fresh first-rung retry would be, but a
rung or deadline that is already lower stays (`restartAfterDelivery`). So a push
that fails right after an acknowledgement waits at most the first rung, not
the rung an earlier outage charged, while a peer that never delivers keeps
climbing.

What must never happen:
* after a delivery the clock is above the first rung (`restart_rung_le_one`);
* a delivery pushes the deadline later (`restart_never_later`);
* after a delivery the peer waits longer than the first rung, or than the
  receiver's backpressure deadline (`restart_within_first_rung`);
* a delivery lets a retry come before the receiver's backpressure deadline
  (`restart_respects_backpressure`);
* a failed pass leaves the clock on the same rung (`bump_climbs`).

Rust anchors: `RetryInfo::bump_with_schedule` and
`RetryInfo::restart_after_delivery` in `crates/storage/src/stores/retry_info.rs`;
`PeerstoreRetryAttempt::CompleteMarker` in `crates/storage/src/stores/peerstore.rs`
restarts the clock for an acknowledged completion that leaves markers behind.
Seconds are `u64` there and `Nat` here. The Rust jitter picks a delay in the
upper half of the rung; here `delay` is any wait up to the rung.
-/

namespace RetrySchedule

-- One peer's retry clock, as `RetryInfo` stores it.
structure Clock where
  -- Retries charged so far (`num_retries`).
  rung : Nat
  -- When the peer is next due (`next_retry_unix`).
  deadline : Nat
  -- The receiver's backpressure deadline (`not_before_unix`).
  notBefore : Nat

-- `bump_with_schedule`: due `delay` from `now`, never before the receiver's
-- deadline, and one rung up.
def bump (now delay : Nat) (c : Clock) : Clock :=
  { c with rung := c.rung + 1, deadline := max (now + delay) c.notBefore }

-- `restart_after_delivery`: the clock a fresh first-rung retry would have,
-- keeping the current rung and deadline wherever they are already lower.
def restartAfterDelivery (now delay : Nat) (c : Clock) : Clock :=
  -- A retry from rung zero, bumped once.
  let first := bump now delay { c with rung := 0 }
  -- The lower of each, field by field.
  { c with rung := min c.rung first.rung, deadline := min c.deadline first.deadline }

-- Promise: after a delivery the clock is on the first rung or below.
theorem restart_rung_le_one (now delay : Nat) (c : Clock) :
    (restartAfterDelivery now delay c).rung ≤ 1 := by
  -- The rung is the lower of the old rung and one.
  simp only [restartAfterDelivery, bump]
  omega

-- Promise: a delivery never makes the peer wait longer than it already did.
theorem restart_never_later (now delay : Nat) (c : Clock) :
    (restartAfterDelivery now delay c).deadline ≤ c.deadline := by
  -- The deadline is the lower of the old one and the first rung's.
  simp only [restartAfterDelivery, bump]
  omega

-- Promise: after a delivery the peer is due within the first rung, or at the
-- receiver's backpressure deadline if that is later.
theorem restart_within_first_rung (ladder : List Nat) (now delay : Nat) (c : Clock)
    -- Given the delay is at most the first rung's interval,
    (h : delay ≤ cap ladder 0) :
    -- the new deadline is no later than the first rung from now, or the
    -- receiver's deadline.
    (restartAfterDelivery now delay c).deadline ≤ max (now + cap ladder 0) c.notBefore := by
  -- Both candidates are bounded by the first rung's deadline.
  simp only [restartAfterDelivery, bump]
  omega

-- Promise: a delivery never moves a retry before the receiver's deadline.
theorem restart_respects_backpressure (now delay : Nat) (c : Clock)
    -- Given the clock already respects the receiver's deadline,
    (h : c.notBefore ≤ c.deadline) :
    -- it still does after the delivery.
    c.notBefore ≤ (restartAfterDelivery now delay c).deadline := by
  -- Both candidates are at or past the receiver's deadline.
  simp only [restartAfterDelivery, bump]
  omega

-- Promise: a failed pass climbs exactly one rung.
theorem bump_climbs (now delay : Nat) (c : Clock) :
    (bump now delay c).rung = c.rung + 1 := by
  -- That is what `bump` writes.
  rfl

end RetrySchedule
