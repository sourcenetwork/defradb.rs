/-!
# A DocID's UUID text is checked the way gofrs/uuid checks it

The story, in plain words. A DocID is `bae-` followed by a UUID written as
text. Go reads that text with gofrs/uuid, so a bad relation ID is refused
with gofrs's words, and some tests expect those exact words. defradb.rs reads
it with its own copy of gofrs's rule. This file writes that rule down and
proves what it promises:

* the only lengths it accepts are 32, 34, 36, 38, 41 and 45 (32 hex digits;
  the canonical 36 with hyphens; either one in braces; either one after
  `urn:uuid:`), and any other length is refused by naming the length;
* every text it accepts carries exactly 32 hex digits, which are the UUID's
  16 bytes, in every one of its six forms;
* the "length" refusal is given only for a length gofrs never accepts, so a
  text of a right length but a wrong shape is told about its shape.

Rust owner: crates/defra-core/src/doc_id.rs `parse_uuid`, called by
`ParsedDocId::from_string`, which relation IDs go through on set
(crates/query/src/plan/mutation/create_conversions.rs). Its tests are
`a_bad_uuid_is_refused_in_gofrs_words` and
`every_gofrs_form_parses_to_the_same_uuid`. The model leaves out how the
message quotes the text (Go's `%q`, Rust's `{:?}`), which agree for ASCII.
-/

namespace DocIdText

-- Why a text is refused, in the three ways gofrs says it.
inductive Refusal where
  -- `uuid: incorrect UUID length N in string "..."`
  | length (n : Nat)
  -- `uuid: incorrect UUID format in string "..."`
  | format
  -- `uuid: invalid UUID format`
  | invalid
  deriving DecidableEq, Repr

-- A hex digit, either case, as gofrs's `fromHexChar` accepts.
def isHex (c : Char) : Bool :=
  ('0' ≤ c ∧ c ≤ '9') ∨ ('a' ≤ c ∧ c ≤ 'f') ∨ ('A' ≤ c ∧ c ≤ 'F')

-- The positions of the four hyphens in the canonical form.
def hyphens : List Nat := [8, 13, 18, 23]

-- The text inside its wrapping: braces or the URN prefix, by length.
def unwrap (s : List Char) : Except Refusal (List Char) :=
  -- 32 and 36 are bare.
  if s.length = 32 ∨ s.length = 36 then .ok s
  -- 34 and 38 must be wrapped in braces.
  else if s.length = 34 ∨ s.length = 38 then
    if s.head? = some '{' ∧ s.getLast? = some '}' then .ok ((s.drop 1).dropLast)
    else .error .format
  -- 41 and 45 must start with `urn:uuid:`.
  else if s.length = 41 ∨ s.length = 45 then
    if s.take 9 = "urn:uuid:".toList then .ok (s.drop 9) else .error .format
  -- Any other length is refused by naming it.
  else .error (.length s.length)

-- The 32 hex digits inside the unwrapped text: the canonical form loses its
-- hyphens, which must be at their four places.
def digits (inner : List Char) : Except Refusal (List Char) :=
  if inner.length = 36 then
    -- Every hyphen must be where the canonical form puts it.
    if hyphens.all (fun i => inner[i]? = some '-') then
      .ok ((List.range 36).filterMap fun i => if i ∈ hyphens then none else inner[i]?)
    else .error .format
  else .ok inner

-- The whole check: unwrap, take the digits, and require every one to be hex.
def parse (s : List Char) : Except Refusal (List Char) := do
  -- Unwrap braces or the URN prefix, or refuse the length or the shape.
  let inner ← unwrap s
  -- Take the digits out of the canonical form, or refuse misplaced hyphens.
  let hex ← digits inner
  -- Refuse any character that is not a hex digit.
  if hex.all isHex then .ok hex else .error .invalid

-- The lengths gofrs accepts.
def accepted : List Nat := [32, 34, 36, 38, 41, 45]

-- Promise: a "length" refusal names the text's own length, and only a length
-- gofrs never accepts.
theorem length_refusal_only_for_unaccepted (s : List Char) (n : Nat)
    (h : unwrap s = .error (.length n)) : n = s.length ∧ n ∉ accepted := by
  -- Walk the branches of `unwrap`: only the last refuses a length.
  unfold unwrap at h
  by_cases h1 : s.length = 32 ∨ s.length = 36
  · rw [if_pos h1] at h; simp at h
  rw [if_neg h1] at h
  by_cases h2 : s.length = 34 ∨ s.length = 38
  · rw [if_pos h2] at h; split at h <;> simp at h
  rw [if_neg h2] at h
  by_cases h3 : s.length = 41 ∨ s.length = 45
  · rw [if_pos h3] at h; split at h <;> simp at h
  rw [if_neg h3] at h
  simp only [Except.error.injEq, Refusal.length.injEq] at h
  subst h
  simp only [accepted, List.mem_cons, List.mem_nil_iff, or_false]
  simp only [not_or] at h1 h2 h3
  refine ⟨trivial, ?_⟩
  omega

-- Promise: a text of an accepted length is never told about its length.
theorem accepted_length_never_says_length (s : List Char) (n : Nat)
    (hlen : s.length ∈ accepted) : unwrap s ≠ .error (.length n) := by
  -- If it were, the length would be outside the accepted list.
  intro h
  exact (length_refusal_only_for_unaccepted s n h).2 ((length_refusal_only_for_unaccepted s n h).1 ▸ hlen)

-- Unwrapping leaves the 32 hex digits or the 36 canonical characters.
theorem unwrap_length (s inner : List Char) (h : unwrap s = .ok inner) :
    inner.length = 32 ∨ inner.length = 36 := by
  -- Each accepting branch strips exactly its wrapping.
  unfold unwrap at h
  by_cases h1 : s.length = 32 ∨ s.length = 36
  · rw [if_pos h1] at h
    simp only [Except.ok.injEq] at h
    subst h
    exact h1
  rw [if_neg h1] at h
  by_cases h2 : s.length = 34 ∨ s.length = 38
  · rw [if_pos h2] at h
    split at h
    · simp only [Except.ok.injEq] at h
      subst h
      simp only [List.length_dropLast, List.length_drop]
      omega
    · simp at h
  rw [if_neg h2] at h
  by_cases h3 : s.length = 41 ∨ s.length = 45
  · rw [if_pos h3] at h
    split at h
    · simp only [Except.ok.injEq] at h
      subst h
      simp only [List.length_drop]
      omega
    · simp at h
  rw [if_neg h3] at h
  simp at h

-- Which of the 36 canonical positions hold a digit, not a hyphen.
def isDigitAt (i : Nat) : Bool := !(hyphens.contains i)

-- The canonical form has 32 digit positions.
theorem digit_positions : (List.range 36).countP isDigitAt = 32 := by decide

-- Promise: taking the digits out of an unwrapped text leaves exactly 32.
theorem digits_length (inner hex : List Char) (hin : inner.length = 32 ∨ inner.length = 36)
    (h : digits inner = .ok hex) : hex.length = 32 := by
  unfold digits at h
  split at h
  · split at h
    · -- The canonical form keeps one character per digit position, and
      -- every position below 36 holds a character.
      rename_i h36 _
      simp only [Except.ok.injEq] at h
      subst h
      rw [List.length_filterMap_eq_countP, ← digit_positions]
      apply List.countP_congr
      intro i hi
      have hi36 : i < inner.length := by simp at hi; omega
      simp only [isDigitAt, List.contains_eq_mem, hyphens]
      split <;> simp_all [List.getElem?_eq_getElem hi36]
    · simp at h
  · -- The bare 32: the digits are the text itself.
    rename_i h36
    simp only [Except.ok.injEq] at h
    subst h
    omega

-- Promise: every text the check accepts carries exactly 32 hex digits, the
-- UUID's 16 bytes, whichever of the six forms it was written in.
theorem parse_accepts (s hex : List Char) (h : parse s = .ok hex) :
    hex.length = 32 ∧ hex.all isHex = true := by
  -- Follow the three steps; only the path where all three succeed returns.
  unfold parse at h
  cases hu : unwrap s with
  | error e => simp [hu, bind, Except.bind] at h
  | ok inner =>
    cases hd : digits inner with
    | error e => simp [hu, hd, bind, Except.bind] at h
    | ok d =>
      simp only [hu, hd, bind, Except.bind] at h
      split at h
      · rename_i hall
        simp only [Except.pure, Except.ok.injEq] at h
        subst h
        exact ⟨digits_length inner d (unwrap_length s inner hu) hd, hall⟩
      · simp at h

-- The canonical example from the tests, and the failures they check.
example : parse "c94acbfa-dd53-40d0-97f3-29ce16c333fc".toList
    = .ok "c94acbfadd5340d097f329ce16c333fc".toList := rfl
example : parse "35953ca-518d-9e6b-9ce6cd00eff5".toList = .error (.length 30) := rfl
example : parse "z94acbfa-dd53-40d0-97f3-29ce16c333fc".toList = .error .invalid := rfl
example : parse "c94acbfa_dd53-40d0-97f3-29ce16c333fc".toList = .error .format := rfl
example : parse "{c94acbfadd5340d097f329ce16c333fc}".toList
    = .ok "c94acbfadd5340d097f329ce16c333fc".toList := rfl
example : parse "urn:uuid:c94acbfadd5340d097f329ce16c333fc".toList
    = .ok "c94acbfadd5340d097f329ce16c333fc".toList := rfl

end DocIdText
