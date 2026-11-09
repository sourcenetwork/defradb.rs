/-!
# DateTime text drops trailing zeros and nothing else

The story, in plain words. A DateTime is written in Go's RFC3339Nano form:
the seconds, then a dot and up to nine fraction digits, then the zone. Go
leaves out the zeros at the end of the fraction, and leaves out the dot too
when every fraction digit is zero: `.123000000` becomes `.123`, and
`.000000000` disappears. defradb.rs writes all nine digits first, then trims
them. This file proves the trim does what Go's form needs: what is left
never ends in a zero, the zeros it removed were only zeros at the end (so
padding them back gives the nine digits again, and the time is unchanged),
an all-zero fraction leaves nothing (so the dot goes), and a fraction with
any non-zero digit keeps at least one digit (so the dot stays).

Rust owner: crates/document/src/encoding.rs `trim_rfc3339_trailing_zeros`
(the trim of the digits between the dot and the zone) and
`format_time_rfc3339_nano` (which skips the fraction when the nanoseconds
are zero). Mutation responses use it since c41d3bf82; the tests are
crates/document/tests/encoding_tests.rs and the mutation DateTime scenarios.
The model works on the fraction digits as a list of characters: it proves the
trim, not how the Rust code finds the dot and the zone in the string.
-/

namespace Rfc3339Nano

-- The trim: reverse the digits, drop the zeros now at the front, reverse back.
def trim (digits : List Char) : List Char :=
  (digits.reverse.dropWhile (· == '0')).reverse

-- The zeros the trim removed: the leading zeros of the reversed digits.
def removed (digits : List Char) : List Char :=
  digits.reverse.takeWhile (· == '0')

-- Promise: the trimmed digits never end in a zero.
theorem no_trailing_zero (digits : List Char) : (trim digits).getLast? ≠ some '0' := by
  -- The last of the trimmed digits is the first of the reversed rest, which
  -- `dropWhile` left because it is not a zero.
  unfold trim
  rw [List.getLast?_reverse]
  intro h
  have := List.head?_dropWhile_not (· == '0') digits.reverse
  rw [h] at this
  simp at this

-- Promise: every removed character is a zero.
theorem removed_all_zero (digits : List Char) : ∀ c ∈ removed digits, c = '0' := by
  -- `takeWhile` only keeps characters that pass its test, here `== '0'`.
  intro c hc
  have := List.all_eq_true.mp (List.all_takeWhile (p := (· == '0'))) c hc
  simpa using this

-- Promise: the trimmed digits followed by the removed zeros are the original
-- digits, so the trim changes how the time is written, never the time.
theorem trim_restores (digits : List Char) :
    trim digits ++ (removed digits).reverse = digits := by
  -- Reversed, this is `takeWhile ++ dropWhile`, which is the whole list.
  unfold trim removed
  rw [← List.reverse_append, List.takeWhile_append_dropWhile, List.reverse_reverse]

-- Dropping zeros from a list of only zeros leaves nothing.
theorem drop_zeros_of_all_zero (l : List Char) (h : ∀ c ∈ l, c = '0') :
    l.dropWhile (· == '0') = [] := by
  -- One character at a time: each is a zero, so each is dropped.
  induction l with
  | nil => rfl
  | cons a t ih =>
    have ha : a = '0' := h a (by simp)
    have ht : ∀ c ∈ t, c = '0' := fun c hc => h c (by simp [hc])
    simp [List.dropWhile_cons, ha, ih ht]

-- Promise: a fraction of only zeros trims to nothing, so the dot is dropped.
theorem all_zero_trims_to_nothing (digits : List Char) (h : ∀ c ∈ digits, c = '0') :
    trim digits = [] := by
  -- Every reversed character is a zero, so `dropWhile` drops them all.
  unfold trim
  rw [drop_zeros_of_all_zero _ (fun c hc => h c (List.mem_reverse.mp hc))]
  rfl

-- Promise: a fraction with any non-zero digit keeps a digit, so the dot stays.
theorem nonzero_keeps_a_digit (digits : List Char) (h : ∃ c ∈ digits, c ≠ '0') :
    trim digits ≠ [] := by
  -- If nothing were left, the digits would be only the removed zeros.
  intro empty
  obtain ⟨c, hc, hne⟩ := h
  have whole := trim_restores digits
  rw [empty, List.nil_append] at whole
  rw [← whole, List.mem_reverse] at hc
  exact hne (removed_all_zero digits c hc)

-- The cases the Rust doc comments name, checked by evaluation.
example : trim "123000000".toList = "123".toList := by decide
example : trim "100000000".toList = "1".toList := by decide
example : trim "000000000".toList = [] := by decide
example : trim "000000001".toList = "000000001".toList := by decide

end Rfc3339Nano
