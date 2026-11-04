-- `div_mono`, bucketing never reverses an order, comes from Arbitration.Estimators.
import Arbitration.Estimators

/-!
# Arbitration.DueOrder: a tie-break score reorders due roots only within one
# bucket of due time

The story, in plain words. On each pass the retry clock lines up the roots
that are due and hands free fetch slots out in that order. Today the order is
by due time (then CID). `proofs/tla/MC_SyncOwnership_Dispatch.tla` proves it
first-come first-served: an attempt starts only for the oldest due root that
can run (`ACT_AttemptsStartInDueOrder`), and an ended attempt leaves its root
behind every root already waiting (`ACT_EndedAttemptQueuesBehind`). Due times
are distinct instants, so an exact tie never happens and a score ordering ties
would never act.

The tie-break buckets due times by a width `q` (`TIE_BUCKET`, 1 s), measured
from the earliest due root of the pass, and orders by (bucket, score, due time):
roots due within one bucket of each other may be reordered by a score (fewer
attempts without progress first), roots further apart may not. That restates
`ACT_AttemptsStartInDueOrder` at bucket resolution: an attempt starts only for
a root due less than `q` after the oldest due root that can run. A root
re-armed a backoff of at least `q` after its attempt still goes behind every
root that waited, so `ACT_EndedAttemptQueuesBehind` stays exact for it; a
preempted root, due again at once, keeps it at bucket resolution. Every lemma
here holds for any width.

What must never happen:
* a score breaks due-time order between different buckets
  (`lex_keeps_due_order`, `bucket_keeps_order`, `bucket_order_any_score`);
* a score passes a root by a full bucket or more
  (`inversion_within_quantum`, `inversion_within_bucket`);
* equal scores in one bucket leave due-time order (`same_score_keeps_due_order`);
* a root re-armed at the end of its attempt, at least a bucket later, passes a
  root that became due during the attempt (`rearmed_root_goes_behind`).

Rust anchors: `TIE_BUCKET` and `tie_break_reorders` in
`crates/p2p/src/sync/arbitration/due_order.rs`, measured in
`SyncManager::observe_dispatch_pass` (`sync/manager/process/estimates.rs`).
Dispatch itself keeps due-time order; the pass only counts when the tie-break
would differ.
-/


namespace Arbitration

/-! ## Two keys: (due, score) -/

-- Lexicographic order on (due, score).
def before (a b : Nat × Nat) : Prop := a.1 < b.1 ∨ (a.1 = b.1 ∧ a.2 < b.2)

-- Promise: any score only orders equal due times.
theorem lex_keeps_due_order (da db sa sb : Nat) (h : da < db) : before (da, sa) (db, sb) :=
  -- The first key decides.
  Or.inl h

-- Promise: with due times bucketed by `q`, roots a full bucket apart keep their
-- order whatever the score.
theorem bucket_keeps_order {q da db : Nat} (sa sb : Nat) (hq : 0 < q) (h : da + q ≤ db) :
    before (da / q, sa) (db / q, sb) := by
  -- The bucket decides.
  left
  -- Bucketing never reverses an order.
  have := div_mono hq h
  -- One bucket later is exactly one more bucket.
  rw [Nat.add_div_right da hq] at this
  -- So `da / q < db / q`.
  omega

-- Promise: any inversion a score causes is within one bucket.
theorem inversion_within_quantum {q da db : Nat} (sa sb : Nat) (hq : 0 < q) (hlt : da < db)
    (hinv : before (db / q, sb) (da / q, sa)) : db < da + q := by
  -- Either `db` is within a bucket of `da`, or not.
  rcases Nat.lt_or_ge db (da + q) with h | h
  -- Within: done.
  · exact h
  -- Not within: the buckets order them the other way, a contradiction.
  · have := bucket_keeps_order sa sb hq h
    -- Unfold both orders.
    unfold before at this hinv
    -- Bucketing never reverses `da < db`.
    have := div_mono hq (Nat.le_of_lt hlt)
    -- The two orders cannot both hold.
    omega

-- Promise (`ACT_EndedAttemptQueuesBehind`, exact): a root re-armed at its
-- attempt end plus a backoff of at least one bucket goes behind every root
-- that became due during the attempt, whatever either score. Rust:
-- `TIE_BUCKET ≤ PENDING_RETRY_BASE`.
theorem rearmed_root_goes_behind {q tEnd b ta : Nat} (sa sr : Nat) (hq : 0 < q) (hb : q ≤ b)
    (hta : ta ≤ tEnd) : before (ta / q, sa) ((tEnd + b) / q, sr) :=
  -- The re-armed root is at least a bucket later.
  bucket_keeps_order sa sr hq (by omega)

/-! ## Three keys: (bucket, score, due) -/

-- Lexicographic order on (bucket, score, due), the tie-break's dispatch key.
def before3 (a b : Nat × Nat × Nat) : Prop :=
  -- An earlier bucket goes first;
  a.1 < b.1
  -- in one bucket a lower score goes first;
  ∨ (a.1 = b.1 ∧ a.2.1 < b.2.1)
  -- with equal scores the earlier due time goes first.
  ∨ (a.1 = b.1 ∧ a.2.1 = b.2.1 ∧ a.2.2 < b.2.2)

-- The key of a root due `d` after the pass's earliest root, with score `s`.
def key (q d s : Nat) : Nat × Nat × Nat := (d / q, s, d)

-- Promise: ordering by (bucket, score) keeps bucket order for any score.
theorem bucket_order_any_score {q da db : Nat} (sa sb : Nat) (h : da / q < db / q) :
    before3 (key q da sa) (key q db sb) :=
  -- The bucket decides.
  Or.inl h

-- Promise: a root ahead by a full bucket stays ahead, whatever the scores.
theorem bucket_apart_keeps_order {q da db : Nat} (sa sb : Nat) (hq : 0 < q) (h : da + q ≤ db) :
    before3 (key q da sa) (key q db sb) := by
  -- Bucketing never reverses an order.
  have := div_mono hq h
  -- One bucket later is exactly one more bucket.
  rw [Nat.add_div_right da hq] at this
  -- The buckets differ, so the bucket decides.
  exact bucket_order_any_score sa sb (by omega)

-- Promise (`ACT_AttemptsStartInDueOrder` at bucket resolution): when a root
-- due later goes first, it is due less than one bucket after the root it
-- passes.
theorem inversion_within_bucket {q da db : Nat} (sa sb : Nat) (hq : 0 < q) (hlt : da < db)
    (hinv : before3 (key q db sb) (key q da sa)) : db < da + q := by
  -- Either `db` is within a bucket of `da`, or not.
  rcases Nat.lt_or_ge db (da + q) with h | h
  -- Within: done.
  · exact h
  -- Not within: `da` would be first, a contradiction.
  · have := bucket_apart_keeps_order sa sb hq h
    -- Unfold both orders.
    unfold before3 key at this hinv
    -- Bucketing never reverses `da < db`.
    have := div_mono hq (Nat.le_of_lt hlt)
    -- The two orders cannot both hold.
    omega

-- Promise: with equal scores in one bucket, due-time order is kept.
theorem same_score_keeps_due_order {q da db : Nat} (s : Nat) (hb : da / q = db / q)
    (hlt : da < db) : before3 (key q da s) (key q db s) :=
  -- Same bucket, same score: the due time decides.
  Or.inr (Or.inr ⟨hb, rfl, hlt⟩)

end Arbitration
