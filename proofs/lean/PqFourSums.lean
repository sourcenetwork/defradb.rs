/-!
# PqFourSums: four running sums add up to the one running sum

The story, in plain words. A PQ distance is the sum of one table lookup per
code byte. The code used to add every lookup to one running total, so each
add waited on the one before it. It now keeps four totals: lookups 0, 4, 8,
... go to the first, 1, 5, 9, ... to the second, and so on, and a code whose
length is not a multiple of four sends its last one to three lookups to the
first totals. At the end the four totals are added up.

A tiny example. Lookups [1, 2, 3, 4, 5]. One total: 15. Four totals: the
quad [1, 2, 3, 4] gives (1, 2, 3, 4), the leftover 5 goes to the first, so
(6, 2, 3, 4), which adds up to 15.

What must never happen: the four totals add up to something other than the
one total (`four_eq_sum`). Forgetting the leftover lookups loses part of the
distance whenever the code length is not a multiple of four
(`dropped_tail_loses_lookups`).

The model adds exact numbers. The code adds `f64`s, where changing the order
of the adds can change the last bit of the result; that rounding is outside
this model and is covered by measuring recall.

Rust anchor: `ProductQuantizer::distance`
(`crates/db/src/index/vector/quantize/pq.rs`).
-/

namespace PqFourSums

-- The four totals.
abbrev Acc := Nat × Nat × Nat × Nat

-- The four totals added up, first to last.
def total : Acc → Nat
  | (a, b, c, d) => a + b + c + d

-- Whole quads first, one lookup to each total; then the one to three
-- leftover lookups, to the first totals.
def four : List Nat → Acc → Acc
  -- a whole quad: one lookup to each total, then the rest
  | x0 :: x1 :: x2 :: x3 :: rest, (a, b, c, d) => four rest (a + x0, b + x1, c + x2, d + x3)
  -- three left over
  | [x0, x1, x2], (a, b, c, d) => (a + x0, b + x1, c + x2, d)
  -- two left over
  | [x0, x1], (a, b, c, d) => (a + x0, b + x1, c, d)
  -- one left over
  | [x0], (a, b, c, d) => (a + x0, b, c, d)
  -- nothing left over
  | [], acc => acc

-- One running total, the old code.
def one (xs : List Nat) : Nat := xs.foldl (· + ·) 0

-- The one running total is the plain sum.
theorem one_eq_sum (xs : List Nat) : one xs = xs.sum := by
  -- a fold from any start adds the start to the sum
  have h : ∀ (s : Nat) (ys : List Nat), ys.foldl (· + ·) s = s + ys.sum := by
    intro s ys
    induction ys generalizing s with
    | nil => simp
    | cons y ys ih => simp [List.foldl_cons, ih]; omega
  simpa [one] using h 0 xs

-- From any four totals, the totals after the lookups are the totals before
-- plus the sum of the lookups.
theorem total_four : ∀ (xs : List Nat) (acc : Acc), total (four xs acc) = total acc + xs.sum
  -- a whole quad, then the rest
  | x0 :: x1 :: x2 :: x3 :: rest, (a, b, c, d) => by
    simp only [four]
    rw [total_four rest]
    simp only [total, List.sum_cons]; omega
  -- three, two, one or no leftover lookups
  | [x0, x1, x2], (a, b, c, d) => by simp [four, total]; omega
  | [x0, x1], (a, b, c, d) => by simp [four, total]; omega
  | [x0], (a, b, c, d) => by simp [four, total]; omega
  | [], (a, b, c, d) => by simp [four, total]

-- Every code: four totals from zero add up to the one running total.
theorem four_eq_sum (xs : List Nat) : total (four xs (0, 0, 0, 0)) = one xs := by
  rw [total_four, one_eq_sum]
  simp [total]

-- The planted bug: whole quads only, the leftover lookups dropped.
def quadsOnly : List Nat → Acc → Acc
  -- a whole quad, as before
  | x0 :: x1 :: x2 :: x3 :: rest, (a, b, c, d) => quadsOnly rest (a + x0, b + x1, c + x2, d + x3)
  -- anything shorter than a quad is forgotten
  | _, acc => acc

-- Five lookups: the fifth is lost, so the distance comes out short.
theorem dropped_tail_loses_lookups :
    total (quadsOnly [1, 2, 3, 4, 5] (0, 0, 0, 0)) ≠ one [1, 2, 3, 4, 5] := by
  decide

end PqFourSums
