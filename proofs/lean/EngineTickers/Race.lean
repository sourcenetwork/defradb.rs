import EngineTickers.Mark

/-!
# EngineTickers.Race: racing publishers, the close, and what one store can add

The story, in plain words. Several publishers add one store's new counts to the
exported total at once: reads of the totals, and the store's own close. Each
reads the count, reads the mark, and swaps the mark forward with one
compare-and-swap; a publisher whose swap lost takes the mark the swap found and
goes round again, until its swap wins or the mark is at or past its reading.
The close must leave nothing behind: when it is over, the mark covers every
count the store made, so every count reaches the total once.

A tiny example. Two reads see the count at 1 and at 2. Whichever order their
swaps land in, the mark ends at 2 and the total grows by 2. The same read
twice adds nothing the second time.

What must never happen:
* a publish ends with its reading not covered by the mark (`won_attempt_sets_reading`,
  `stop_means_covered`, `covered_stays_covered`, `publishAlone_covers`);
* the loop goes round forever (`lost_swap_shrinks_gap`);
* two publishes give a different mark or total depending on their order
  (`publishAlone_comm`), or a repeated publish adds again (`publishAlone_twice`);
* a store adds more than it counted, or, once its close covered its count, any
  other amount than that count (`fresh_export_adds_at_most`,
  `closed_export_adds_exactly`).

Two witnesses show what the guards are for: a plain store instead of the swap
adds one count twice (`blind_store_counts_twice`, against `swap_counts_once`),
and two exports of the same statistics add every count twice
(`shared_statistics_count_twice`).

The interleavings (a swap and its add as two steps, the close leaving the map,
reads that miss a closed store) are the TLA+ model `proofs/tla/EngineTickers.tla`.

Rust anchors: `Source::publish` and `EngineTickerExport::drop`
(`crates/telemetry/src/engine_tickers.rs`).
-/

namespace EngineTickers

-- Promise: a pass that won leaves the mark at the publisher's reading.
theorem won_attempt_sets_reading (w : Words) (seen now : Nat)
    -- Given that the pass won,
    (won : (attempt w seen now).2 = true) :
    -- the mark after it is the reading.
    (attempt w seen now).1.mark = now := by
  -- Split on whether the reading was past the mark the publisher saw.
  by_cases ahead : seen < now
  -- It was: split on whether the swap won.
  · by_cases same : w.mark = seen
    -- It won: computing the mark gives `now = now`.
    · simp [attempt, cas, ahead, same]
    -- It lost: computing `won` turns it into `false = true`, which closes the goal.
    · simp [attempt, cas, ahead, same] at won
  -- It was not: computing `won` turns it into `false = true`, which closes the goal.
  · simp [attempt, cas, ahead] at won

-- Promise: a loop that stops on its test, having seen a mark no later than the
-- current one, leaves its reading covered by the mark.
theorem stop_means_covered (w : Words) (seen now : Nat)
    -- Given that the reading was not past what the publisher saw,
    (stopped : ¬ seen < now)
    -- and that what it saw is at most the mark now (marks only grow, `run_mark_grows`),
    (old : seen ≤ w.mark) :
    -- the mark covers the reading.
    now ≤ w.mark := by
  -- Arithmetic on the two facts.
  omega

-- Promise: a mark that covers a reading keeps covering it, whatever passes come after.
theorem covered_stays_covered (w : Words) (now : Nat) (ops : List (Nat × Nat))
    -- Given that the mark covers the reading now,
    (covered : now ≤ w.mark) :
    -- it covers it after any history.
    now ≤ (run w ops).mark := by
  -- The mark only grows over any history,
  have grows := run_mark_grows w ops
  -- so it stays at or past the reading.
  omega

-- Promise: every lost swap shrinks the gap the loop still has to close, so the loop ends.
theorem lost_swap_shrinks_gap (w : Words) (seen now : Nat)
    -- Given that the reading was past what the publisher saw,
    (ahead : seen < now)
    -- that the pass lost,
    (lost : (attempt w seen now).2 = false)
    -- and that what it saw is at most the mark now (marks only grow),
    (old : seen ≤ w.mark) :
    -- the gap from the mark the swap found is smaller than the gap from what it saw.
    now - w.mark < now - seen := by
  -- The swap lost with the reading ahead, so the mark no longer held what was seen:
  have moved : w.mark ≠ seen := by
    -- suppose it still did;
    intro same
    -- then computing `lost` turns it into `true = false`, which closes the goal.
    simp [attempt, cas, ahead, same] at lost
  -- The mark moved past what was seen, and arithmetic does the rest.
  omega

-- A publish with nobody in between: read the mark, then one pass from it.
def publishAlone (w : Words) (now : Nat) : Words :=
  -- The pass sees the mark exactly as it is.
  (attempt w w.mark now).1

-- Promise: a pass from a freshly read mark (what the loop does after it takes
-- the mark a lost swap found) leaves the reading covered.
theorem publishAlone_covers (w : Words) (now : Nat) :
    -- The mark after it is at or past the reading.
    now ≤ (publishAlone w now).mark := by
  -- Split on whether the reading is past the mark.
  by_cases ahead : w.mark < now
  -- It is: the swap wins and the mark becomes the reading.
  · simp [publishAlone, attempt, cas, ahead] <;> omega
  -- It is not: nothing changes, and the mark was already at or past the reading.
  · simp [publishAlone, attempt, cas, ahead] <;> omega

-- Promise: publishing the same reading twice adds nothing the second time.
theorem publishAlone_twice (w : Words) (now : Nat) :
    -- A second publish of the same reading leaves the words as the first left them.
    publishAlone (publishAlone w now) now = publishAlone w now := by
  -- Split on whether the reading is past the mark.
  by_cases ahead : w.mark < now
  -- It is: the first publish moves the mark to the reading, and the second finds `now < now` false.
  · simp [publishAlone, attempt, cas, ahead, Nat.lt_irrefl]
  -- It is not: neither publish changes anything.
  · simp [publishAlone, attempt, cas, ahead]

-- Promise: two publishes leave the same mark in either order.
theorem publishAlone_comm_mark (w : Words) (a b : Nat) :
    -- Reading `a` then `b` gives the mark that `b` then `a` gives.
    (publishAlone (publishAlone w a) b).mark = (publishAlone (publishAlone w b) a).mark := by
  -- Split on every order the old mark and the two readings can be in,
  by_cases ha : w.mark < a <;> by_cases hb : w.mark < b <;>
    by_cases hab : a < b <;> by_cases hba : b < a <;>
    -- compute both sides in each case, and arithmetic closes what is left
    -- or finds the case impossible.
    simp [publishAlone, attempt, cas, ha, hb, hab, hba] <;> omega

-- Promise: two publishes add the same amount to the total in either order.
theorem publishAlone_comm_total (w : Words) (a b : Nat) :
    -- Reading `a` then `b` gives the total that `b` then `a` gives.
    (publishAlone (publishAlone w a) b).total = (publishAlone (publishAlone w b) a).total := by
  -- Split on every order the old mark and the two readings can be in,
  by_cases ha : w.mark < a <;> by_cases hb : w.mark < b <;>
    by_cases hab : a < b <;> by_cases hba : b < a <;>
    -- compute both sides in each case, and arithmetic closes what is left
    -- or finds the case impossible.
    simp [publishAlone, attempt, cas, ha, hb, hab, hba] <;> omega

-- Promise: racing publishes agree whatever order their swaps land in: same mark, same total.
theorem publishAlone_comm (w : Words) (a b : Nat) :
    -- The marks agree,
    (publishAlone (publishAlone w a) b).mark = (publishAlone (publishAlone w b) a).mark ∧
    -- and so do the totals.
    (publishAlone (publishAlone w a) b).total = (publishAlone (publishAlone w b) a).total :=
  -- The two halves are the two promises just above.
  ⟨publishAlone_comm_mark w a b, publishAlone_comm_total w a b⟩

-- Every reading in a history is at most `c`, the count the engine has made.
def readingsAtMost (c : Nat) : List (Nat × Nat) → Prop
  -- An empty history read nothing.
  | [] => True
  -- The first pass read at most `c`, and so did the rest.
  | op :: rest => op.2 ≤ c ∧ readingsAtMost c rest

-- Promise: a mark never passes the count when no reading does.
theorem run_mark_at_most (c : Nat) (ops : List (Nat × Nat)) :
    -- For every starting words whose mark is at most `c`, and every history
    -- whose passes read at most `c`, the final mark is at most `c`.
    ∀ (w : Words), w.mark ≤ c → readingsAtMost c ops → (run w ops).mark ≤ c := by
  -- Go through the history one pass at a time.
  induction ops with
  -- No passes: name the words and the starting fact, unfold `run`, and the fact is the goal.
  | nil => intro w start _; simp only [run] <;> omega
  -- A first pass, then the rest:
  | cons op rest ih =>
    -- name the words, the starting fact and the bound,
    intro w start bounded
    -- unfold the bound into the first reading's bound and the rest's,
    simp only [readingsAtMost] at bounded
    -- and name the two halves;
    obtain ⟨here, later⟩ := bounded
    -- the first pass leaves the mark where it was or at its reading, both at most `c`;
    have next : (attempt w op.1 op.2).1.mark ≤ c := by
      -- look at both cases, and arithmetic closes each.
      rcases attempt_mark_cases w op.1 op.2 with h | h <;> omega
    -- unfold `run` one pass,
    simp only [run]
    -- and the rest of the history stays at most `c` by the induction hypothesis.
    exact ih (attempt w op.1 op.2).1 next later

-- Promise: a store whose mark starts at zero, and whose readings never pass the
-- count `c` its engine made, adds at most `c` to the total: no count twice.
theorem fresh_export_adds_at_most (w : Words) (ops : List (Nat × Nat)) (c : Nat)
    -- Given a mark that starts at zero (a new export)
    (fresh : w.mark = 0)
    -- and readings never past `c`,
    (bounded : readingsAtMost c ops) :
    -- the total grows by at most `c`.
    (run w ops).total ≤ w.total + c := by
  -- The total grows by exactly how far the mark moved,
  have gain := run_gain w ops
  -- and the mark ends at most at `c`;
  have most := run_mark_at_most c ops w (by omega) bounded
  -- arithmetic puts the two together.
  omega

-- Promise: if, besides, the close left the mark at `c` or past it, the store
-- added exactly `c`: every count once.
theorem closed_export_adds_exactly (w : Words) (ops : List (Nat × Nat)) (c : Nat)
    -- Given a mark that starts at zero,
    (fresh : w.mark = 0)
    -- readings never past `c`,
    (bounded : readingsAtMost c ops)
    -- and a final mark that covers `c` (what the close leaves),
    (closed : c ≤ (run w ops).mark) :
    -- the total grew by exactly `c`.
    (run w ops).total = w.total + c := by
  -- The total grows by exactly how far the mark moved,
  have gain := run_gain w ops
  -- and the mark ends at most at `c`, so exactly at `c`;
  have most := run_mark_at_most c ops w (by omega) bounded
  -- arithmetic puts them together.
  omega

-- What a plain store would do instead of the swap: write the reading and add
-- the gap from what was seen, whatever the mark holds now.
def blindStore (w : Words) (seen now : Nat) : Words :=
  -- The mark becomes the reading, and the gap from the seen mark is added, unchecked.
  { mark := now, total := w.total + (now - seen) }

-- Witness: without the swap, two publishers that saw the same mark both add the same count.
theorem blind_store_counts_twice :
    -- One count (the reading 1), added twice: the total is 2.
    (blindStore (blindStore { mark := 0, total := 0 } 0 1) 0 1).total = 2 := by
  -- Compute both stores.
  decide

-- Witness: with the swap, the second of those two publishers loses, and the count is added once.
theorem swap_counts_once :
    -- The same two publishers through the swap: the total is 1.
    (attempt (attempt { mark := 0, total := 0 } 0 1).1 0 1).1.total = 1 := by
  -- Compute both passes.
  decide

-- Witness: two exports of the same statistics each start their mark at zero, so
-- each adds the same count: one count, added twice.
theorem shared_statistics_count_twice :
    -- Both exports publish the one count; together they add 2.
    (publishAlone { mark := 0, total := 0 } 1).total
      + (publishAlone { mark := 0, total := 0 } 1).total = 2 := by
  -- Compute both publishes.
  decide

end EngineTickers
