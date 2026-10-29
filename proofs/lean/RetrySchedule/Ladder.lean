/-!
# RetrySchedule.Ladder: a replicator retry ladder always waits a configured time

The story, in plain words. When a push to a replicator fails, the node waits
before trying again. How long it waits comes from a ladder of intervals, in
seconds: the first retry waits the first interval, the second the second, and
once the ladder runs out every later retry waits the last one. The app may give
its own ladder (`replicator_retry_intervals`); otherwise the node uses Go's
default, 30 seconds doubling up to 32 minutes.

What must never happen:
* a ladder that is empty or holds a zero is accepted, so a retry would read past
  the ladder or not wait at all (`new_valid`, `new_accepts_valid`);
* an accepted ladder is changed on the way in (`new_keeps`);
* a retry waits a time that is not on the ladder, or waits no time
  (`cap_mem`, `cap_positive`);
* the retries do not climb the ladder in order, or do not stay on its last rung
  once it runs out (`cap_follows_ladder`, `cap_repeats_last`);
* the default ladder is one the node would refuse (`default_accepted`).

Rust anchors: `RetrySchedule::new` and `RetrySchedule::cap` in
`crates/storage/src/stores/retry_info.rs`, and `RETRY_INTERVALS_SECS` beside
them. `build_node` in `crates/embedded/src/node.rs` builds the ladder with `new`
and refuses to start on its error. Intervals are `u64` seconds; `Nat` here is the
same values without an upper limit, which no rule below needs.
-/

namespace RetrySchedule

-- A ladder the node may use: at least one interval, and every interval more
-- than zero seconds.
def Valid (ladder : List Nat) : Prop :=
  -- Something to wait,
  ladder ≠ [] ∧
  -- and every wait takes some time.
  ∀ interval ∈ ladder, 0 < interval

-- `RetrySchedule::new`: refuse an empty ladder or one holding a zero, and keep
-- any other exactly as given.
def new (intervals : List Nat) : Option (List Nat) :=
  -- Empty, or a zero somewhere: refused;
  if intervals = [] ∨ 0 ∈ intervals then none
  -- otherwise accepted as is.
  else some intervals

-- `RetrySchedule::cap`: the wait for retry number `attempt`, counting from zero.
-- Rust reads `ladder[min(attempt, len - 1)]`; walking the list one rung per
-- attempt, and stopping on the last rung, reads the same interval.
def cap : List Nat → Nat → Nat
  -- No ladder: never reached, since `new` refuses one.
  | [], _ => 0
  -- The last rung: every attempt from here on waits it.
  | [last], _ => last
  -- Not the last rung, and this is the attempt it is for: wait it.
  | first :: _ :: _, 0 => first
  -- Not the last rung, and a later attempt: look further up the ladder.
  | _ :: next :: rest, attempt + 1 => cap (next :: rest) attempt

-- `RETRY_INTERVALS_SECS`: Go's default ladder, 30 seconds doubling to 32 minutes.
def defaultLadder : List Nat := [30, 60, 120, 240, 480, 960, 1920]

-- Promise: whatever `new` accepts is exactly the ladder it was given.
theorem new_keeps (intervals ladder : List Nat)
    -- Given `new` accepted `intervals` as `ladder`,
    (h : new intervals = some ladder) :
    -- the ladder is the intervals, unchanged.
    ladder = intervals := by
  -- Look at which branch `new` took.
  unfold new at h
  split at h
  -- Refused: `none` is not `some`, so this case cannot happen.
  · simp at h
  -- Accepted: `some intervals = some ladder` means they are equal.
  · simp at h
    exact h.symm

-- Promise: whatever `new` accepts is a ladder the node may use.
theorem new_valid (intervals ladder : List Nat)
    -- Given `new` accepted `intervals` as `ladder`,
    (h : new intervals = some ladder) :
    -- that ladder is non-empty and every interval is positive.
    Valid ladder := by
  -- The ladder is the intervals themselves.
  have hsame := new_keeps intervals ladder h
  subst hsame
  -- Look at which branch `new` took.
  unfold new at h
  split at h
  -- Refused: cannot happen, as before.
  · simp at h
  -- Accepted: so the ladder was neither empty nor holding a zero.
  · rename_i hcheck
    -- Non-empty: if it were empty, the refusal check would have caught it.
    refine ⟨fun hempty => hcheck (Or.inl hempty), ?_⟩
    -- Positive: an interval of zero would have been caught too.
    intro interval hin
    apply Nat.pos_of_ne_zero
    intro hzero
    subst hzero
    exact hcheck (Or.inr hin)

-- Promise: `new` accepts every ladder the node may use, as it is.
theorem new_accepts_valid (ladder : List Nat)
    -- Given a non-empty ladder of positive intervals,
    (h : Valid ladder) :
    -- `new` accepts it unchanged.
    new ladder = some ladder := by
  -- `new` takes its accepting branch once the refusal check is false.
  unfold new
  rw [if_neg]
  -- The refusal check is false: the ladder is not empty and holds no zero.
  intro hcheck
  cases hcheck with
  -- Empty: but it is not.
  | inl hempty => exact h.1 hempty
  -- Holds a zero: but every interval is positive, and zero is not.
  | inr hzero => exact Nat.lt_irrefl 0 (h.2 0 hzero)

-- Promise: every retry waits one of the ladder's own intervals.
theorem cap_mem : ∀ (ladder : List Nat) (attempt : Nat),
    -- Given the ladder is not empty,
    ladder ≠ [] →
    -- the wait for any attempt is one of its intervals.
    cap ladder attempt ∈ ladder
  -- No ladder: ruled out.
  | [], _, hne => absurd rfl hne
  -- The last rung: the wait is that rung.
  | [last], _, _ => by simp [cap]
  -- The attempt this rung is for: the wait is this rung.
  | first :: next :: rest, 0, _ => by simp [cap]
  -- A later attempt: the wait is on the rest of the ladder, so on the ladder.
  | first :: next :: rest, attempt + 1, _ => by
    have hrest := cap_mem (next :: rest) attempt (by simp)
    simp only [cap]
    exact List.mem_cons_of_mem first hrest

-- Promise: on a ladder `new` accepted, every retry waits some time.
theorem cap_positive (ladder : List Nat) (attempt : Nat)
    -- Given the ladder may be used,
    (h : Valid ladder) :
    -- the wait for any attempt is more than zero.
    0 < cap ladder attempt :=
  -- The wait is on the ladder, and every interval there is positive.
  h.2 _ (cap_mem ladder attempt h.1)

-- Promise: while the ladder lasts, retry number `attempt` waits rung `attempt`.
theorem cap_follows_ladder : ∀ (ladder : List Nat) (attempt : Nat)
    -- Given the attempt is still within the ladder,
    (h : attempt < ladder.length),
    -- it waits that rung.
    cap ladder attempt = ladder[attempt]
  -- No ladder: no attempt is within it.
  | [], _, h => absurd h (Nat.not_lt_zero _)
  -- The last rung: the only attempt within it is the first.
  | [last], attempt, h => by
    have hzero : attempt = 0 := by simp at h; omega
    subst hzero
    simp [cap]
  -- The attempt this rung is for: it is the first rung.
  | first :: next :: rest, 0, _ => by simp [cap]
  -- A later attempt: one rung up on the rest of the ladder.
  | first :: next :: rest, attempt + 1, h => by
    have hrest : attempt < (next :: rest).length := by
      simp only [List.length_cons] at h ⊢
      omega
    simp only [cap]
    rw [cap_follows_ladder (next :: rest) attempt hrest]
    simp

-- Promise: once the ladder runs out, every retry waits its last rung.
theorem cap_repeats_last : ∀ (ladder : List Nat) (attempt : Nat)
    -- Given the ladder is not empty,
    (hne : ladder ≠ [])
    -- and the attempt is at or past its last rung,
    (_h : ladder.length - 1 ≤ attempt),
    -- it waits the last rung.
    cap ladder attempt = ladder.getLast hne
  -- No ladder: ruled out.
  | [], _, hne, _ => absurd rfl hne
  -- One rung: every attempt waits it.
  | [last], _, _, _ => by simp [cap]
  -- Two or more rungs, at the first: not at or past the last, so impossible.
  | first :: next :: rest, 0, _, h => by
    simp only [List.length_cons] at h
    omega
  -- Two or more rungs, a later attempt: the rest of the ladder ends the same.
  | first :: next :: rest, attempt + 1, _, h => by
    have hrest : (next :: rest).length - 1 ≤ attempt := by
      simp only [List.length_cons] at h ⊢
      omega
    simp only [cap]
    rw [cap_repeats_last (next :: rest) attempt (by simp) hrest]
    simp [List.getLast_cons]

-- Promise: the node accepts its own default ladder, unchanged.
theorem default_accepted : new defaultLadder = some defaultLadder := by
  -- Check it: seven positive intervals.
  decide

end RetrySchedule
