/-!
# EngineTickers.Mark: a mark moved by compare-and-swap adds each count once

The story, in plain words. Every open store's engine counts what it does (its
commits, say), and the node reports one running total per kind of count over
every store. Each store keeps a mark: how much of its count is in the total
already. To add a store's new counts, a publisher reads the count, reads the
mark, and, when the count is past the mark, swaps the mark from what it saw to
the count with one compare-and-swap. Only a swap that wins adds anything, and
it adds exactly the gap it closed. A swap that finds the mark moved by someone
else changes nothing.

A tiny example. The mark is 0 and the count is 2. One publisher read the count
a moment ago and saw 1; it swaps 0 to 1, wins and adds 1. Another saw 2 and
swaps 0 to 2: it loses, because the mark is 1 now. It looks again, swaps 1 to
2, wins and adds 1. The total grew by 2: once per count.

What must never happen:
* a swap that lost changes the mark or the total (`failed_attempt_changes_nothing`);
* the mark or the total goes back (`attempt_mark_grows`, `attempt_total_grows`,
  `run_mark_grows`, `run_total_grows`);
* the total grows by anything but the stretches the winning swaps closed
  (`attempt_gain`, `run_gain`, `attempt_total`, `run_total_eq_gains`).

A history here is any list of passes in the order they happened, each with
whatever its publisher saw and read: stale readings and stale marks included,
of any length. The total in these words is this store's share of the exported
total; other stores only add their own shares to the same number, and additions
commute, so these laws hold with them around.

The temporal story (reads and a close racing, the close leaving the map, the
add coming a step after the swap) is the TLA+ model
`proofs/tla/EngineTickers.tla`; this file carries the laws about the two shared
numbers that model checks only for tiny sizes.

Rust anchors: `Source::publish` (`crates/telemetry/src/engine_tickers.rs`): the
loop test `now > seen`, `published[at].compare_exchange_weak(seen, now, ..)`
and, on a win, `TOTALS[at].fetch_add(now - seen, ..)`, modeled here as their
sequential meaning with the add folded into the swap that won.
-/

namespace EngineTickers

-- The two shared numbers of one store, for one ticker.
structure Words where
  -- How much of the store's count is in the total already: `Source::published[at]`.
  mark : Nat
  -- The store's share of the exported total: `TOTALS[at]`.
  total : Nat
  -- Words can be compared and printed.
  deriving DecidableEq, Repr

-- One compare-and-swap of the mark from `seen` to `now`, with the add that
-- follows a win: the words after it, and whether it won.
def cas (w : Words) (seen now : Nat) : Words × Bool :=
  -- If the mark still holds what this publisher saw,
  if w.mark = seen then
    -- the swap writes the reading and wins, and the gap it closed is added;
    ({ mark := now, total := w.total + (now - seen) }, true)
  -- otherwise another publisher moved the mark first:
  else
    -- nothing changes, and the swap loses.
    (w, false)

-- One pass of the loop in `Source::publish`: the test `now > seen`, then the swap.
def attempt (w : Words) (seen now : Nat) : Words × Bool :=
  -- A reading past the mark this publisher saw tries the swap;
  if seen < now then cas w seen now
  -- a reading at or below it ends the loop and changes nothing.
  else (w, false)

-- Promise: a pass that did not win leaves both numbers exactly as they were.
theorem failed_attempt_changes_nothing (w : Words) (seen now : Nat)
    -- Given that the pass did not win,
    (lost : (attempt w seen now).2 = false) :
    -- the words after it are the words before it.
    (attempt w seen now).1 = w := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the mark still held what it saw.
  · by_cases same : w.mark = seen
    -- It did, so the swap won; computing `lost` turns it into `true = false`,
    -- which closes the goal.
    · simp [attempt, cas, ahead, same] at lost
    -- It did not, so the swap lost; computing the pass gives back `w`.
    · simp [attempt, cas, ahead, same]
  -- It was not, so the loop ended; computing the pass gives back `w`.
  · simp [attempt, cas, ahead]

-- Promise: a pass never moves the mark back.
theorem attempt_mark_grows (w : Words) (seen now : Nat) :
    -- The mark after the pass is at least the mark before it.
    w.mark ≤ (attempt w seen now).1.mark := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing leaves `seen ≤ now`, which arithmetic closes from `ahead`.
    · simp [attempt, cas, ahead, same] <;> omega
    -- It lost: computing leaves the mark as it was.
    · simp [attempt, cas, ahead, same] <;> omega
  -- It was not: computing leaves the mark as it was.
  · simp [attempt, cas, ahead] <;> omega

-- Promise: a pass never takes the total back.
theorem attempt_total_grows (w : Words) (seen now : Nat) :
    -- The total after the pass is at least the total before it.
    w.total ≤ (attempt w seen now).1.total := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing leaves `total ≤ total + gap`, which arithmetic closes.
    · simp [attempt, cas, ahead, same] <;> omega
    -- It lost: computing leaves the total as it was.
    · simp [attempt, cas, ahead, same] <;> omega
  -- It was not: computing leaves the total as it was.
  · simp [attempt, cas, ahead] <;> omega

-- Promise: what a pass adds to the total is exactly how far it moved the mark.
theorem attempt_gain (w : Words) (seen now : Nat) :
    -- New total plus old mark equals old total plus new mark.
    (attempt w seen now).1.total + w.mark = w.total + (attempt w seen now).1.mark := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing leaves `total + (now - seen) + seen = total + now`,
    -- which arithmetic closes from `ahead`.
    · simp [attempt, cas, ahead, same] <;> omega
    -- It lost: computing leaves both sides the same.
    · simp [attempt, cas, ahead, same] <;> omega
  -- It was not: computing leaves both sides the same.
  · simp [attempt, cas, ahead] <;> omega

-- Promise: a pass leaves the mark either where it was or at the publisher's reading.
theorem attempt_mark_cases (w : Words) (seen now : Nat) :
    -- One of the two holds.
    (attempt w seen now).1.mark = w.mark ∨ (attempt w seen now).1.mark = now := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing makes the right side `now = now`, which is true.
    · simp [attempt, cas, ahead, same]
    -- It lost: computing makes the left side `mark = mark`, which is true.
    · simp [attempt, cas, ahead, same]
  -- It was not: computing makes the left side `mark = mark`, which is true.
  · simp [attempt, cas, ahead]

-- A history of passes, in the order they happened, each written as
-- (the mark its publisher saw, its reading); run them one after another.
def run : Words → List (Nat × Nat) → Words
  -- No passes: the words stay as they are.
  | w, [] => w
  -- One pass, then the rest of the history from wherever it left the words.
  | w, op :: rest => run (attempt w op.1 op.2).1 rest

-- Promise: over any history the total grows by exactly how far the mark moved.
theorem run_gain (w : Words) (ops : List (Nat × Nat)) :
    -- Final total plus first mark equals first total plus final mark.
    (run w ops).total + w.mark = w.total + (run w ops).mark := by
  -- Go through the history one pass at a time, for every starting words.
  induction ops generalizing w with
  -- No passes: unfolding `run` leaves both sides the same.
  | nil => simp only [run] <;> omega
  -- A first pass, then the rest:
  | cons op rest ih =>
    -- the first pass adds what it moved the mark (`attempt_gain`),
    have step := attempt_gain w op.1 op.2
    -- the rest adds what it moves the mark from there (the induction hypothesis),
    have more := ih (attempt w op.1 op.2).1
    -- unfold `run` one pass,
    simp only [run]
    -- and the two equations add up to the goal.
    omega

-- Promise: over any history the mark never goes back.
theorem run_mark_grows (w : Words) (ops : List (Nat × Nat)) :
    -- The final mark is at least the first.
    w.mark ≤ (run w ops).mark := by
  -- Go through the history one pass at a time, for every starting words.
  induction ops generalizing w with
  -- No passes: unfolding `run` leaves `mark ≤ mark`.
  | nil => simp only [run] <;> omega
  -- A first pass, then the rest:
  | cons op rest ih =>
    -- the first pass does not move the mark back,
    have step := attempt_mark_grows w op.1 op.2
    -- nor does the rest,
    have more := ih (attempt w op.1 op.2).1
    -- unfold `run` one pass,
    simp only [run]
    -- and the two steps chain.
    omega

-- Promise: over any history the total never goes back.
theorem run_total_grows (w : Words) (ops : List (Nat × Nat)) :
    -- The final total is at least the first.
    w.total ≤ (run w ops).total := by
  -- Go through the history one pass at a time, for every starting words.
  induction ops generalizing w with
  -- No passes: unfolding `run` leaves `total ≤ total`.
  | nil => simp only [run] <;> omega
  -- A first pass, then the rest:
  | cons op rest ih =>
    -- the first pass does not take the total back,
    have step := attempt_total_grows w op.1 op.2
    -- nor does the rest,
    have more := ih (attempt w op.1 op.2).1
    -- unfold `run` one pass,
    simp only [run]
    -- and the two steps chain.
    omega

-- The sum of a list of numbers.
def sumOf : List Nat → Nat
  -- The empty list sums to zero.
  | [] => 0
  -- A first number plus the sum of the rest.
  | x :: xs => x + sumOf xs

-- The sum of two lists put end to end is the sum of each.
theorem sumOf_append (xs ys : List Nat) :
    -- Summing the joined list equals adding the two sums.
    sumOf (xs ++ ys) = sumOf xs + sumOf ys := by
  -- Go through the first list one number at a time.
  induction xs with
  -- Empty first list: both sides reduce to the sum of the second.
  | nil => simp [sumOf]
  -- A first number, then the rest: unfold, use the induction hypothesis,
  -- and arithmetic regroups the sum.
  | cons x xs ih => simp [sumOf, ih] <;> omega

-- The stretches the winning passes of a history added, in order.
def gains : Words → List (Nat × Nat) → List Nat
  -- No passes: nothing was added.
  | _, [] => []
  -- A winning pass added its reading minus what it saw; a losing one added nothing.
  | w, op :: rest =>
      (if (attempt w op.1 op.2).2 then [op.2 - op.1] else []) ++ gains (attempt w op.1 op.2).1 rest

-- Promise: one pass adds to the total its stretch if it won, and nothing if it lost.
theorem attempt_total (w : Words) (seen now : Nat) :
    -- The total after the pass is the total before plus the stretch it added.
    (attempt w seen now).1.total
      = w.total + sumOf (if (attempt w seen now).2 then [now - seen] else []) := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing both sides gives `total + (now - seen)`.
    · simp [attempt, cas, ahead, same, sumOf] <;> omega
    -- It lost: computing both sides gives the old total.
    · simp [attempt, cas, ahead, same, sumOf] <;> omega
  -- It was not: computing both sides gives the old total.
  · simp [attempt, cas, ahead, sumOf] <;> omega

-- Promise: after any history the total is the first total plus the sum of
-- the stretches the winning swaps claimed.
theorem run_total_eq_gains (w : Words) (ops : List (Nat × Nat)) :
    -- The final total is the first total plus every accepted stretch.
    (run w ops).total = w.total + sumOf (gains w ops) := by
  -- Go through the history one pass at a time, for every starting words.
  induction ops generalizing w with
  -- No passes: nothing was added and the total is the first one.
  | nil => simp [run, gains, sumOf]
  -- A first pass, then the rest:
  | cons op rest ih =>
    -- the first pass adds its own stretch (`attempt_total`),
    have step := attempt_total w op.1 op.2
    -- the rest adds the stretches of its winners (the induction hypothesis),
    have more := ih (attempt w op.1 op.2).1
    -- unfold `run` and `gains` one pass and split the sum of the joined list,
    simp only [run, gains, sumOf_append]
    -- and the two equations add up to the goal.
    omega

end EngineTickers
