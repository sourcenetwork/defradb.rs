/-!
# ServiceRecord.Pointer: the pointer a node publishes reads back as written

The story, in plain words. A node cannot fit its whole record into the
small text slot its discovery entry offers, so it publishes a pointer
instead: `dsr1:`, its record's sequence number as 16 hex digits, `:`, and
the first 8 bytes of the record's hash as 16 more hex digits. A reader that
sees a higher sequence number than its copy, or a different hash, knows its
copy is stale and asks the node for the record.

What must never happen:
* a pointer reads back with a different sequence number or hash than was
  written (`parse_print`);
* a hex digit reads back as a different value (`hexValue_hexChar`), or a
  run of digits as a different number (`readHex_hexDigits`).

Characters are modelled as their byte values.

Rust anchors: `Pointer::print`, `Pointer::parse` in
`crates/p2p/src/iroh/records/pointer.rs`.
-/

namespace ServiceRecord

-- `n` written as `width` base-16 digits, most significant first; digits
-- beyond the width are dropped, so `n` must fit.
def hexDigits : Nat → Nat → List Nat
  -- No width left: no digits;
  | 0, _ => []
  -- otherwise the higher digits, then the lowest.
  | w + 1, n => hexDigits w (n / 16) ++ [n % 16]

-- Read base-16 digits, most significant first, back into a number.
def readHex (digits : List Nat) : Nat := digits.foldl (fun acc d => acc * 16 + d) 0

-- Promise: a number that fits the width reads back as itself.
theorem readHex_hexDigits (w n : Nat)
    -- Given `n` fits in `w` hex digits,
    (h : n < 16 ^ w) :
    -- writing then reading gives `n`.
    readHex (hexDigits w n) = n := by
  induction w generalizing n with
  -- Width 0: only 0 fits, and no digits read as 0.
  | zero =>
    simp only [Nat.pow_zero] at h
    simp [hexDigits, readHex]
    omega
  -- One more digit: the higher digits read back as `n / 16`, then the low digit is added.
  | succ w ih =>
    have hq : n / 16 < 16 ^ w := by
      rw [Nat.pow_succ] at h
      omega
    have := ih (n / 16) hq
    simp only [readHex, hexDigits, List.foldl_append, List.foldl_cons, List.foldl_nil] at this ⊢
    rw [this]
    omega

-- Every digit written is a single hex digit.
theorem hexDigits_lt (w n : Nat) : ∀ d ∈ hexDigits w n, d < 16 := by
  induction w generalizing n with
  -- No digits: nothing to check.
  | zero => simp [hexDigits]
  -- The higher digits by induction; the low digit is a remainder by 16.
  | succ w ih =>
    intro d hd
    simp only [hexDigits, List.mem_append, List.mem_singleton] at hd
    rcases hd with hd | hd
    · exact ih (n / 16) d hd
    · omega

-- The digit count matches the width.
theorem hexDigits_length (w n : Nat) : (hexDigits w n).length = w := by
  induction w generalizing n with
  | zero => rfl
  | succ w ih => simp [hexDigits, ih]

-- A hex digit's character: `0`-`9`, then lowercase `a`-`f`.
def hexChar (d : Nat) : Nat := if d < 10 then 48 + d else 87 + d

-- A character's hex value; nothing for any other character.
def hexValue (c : Nat) : Option Nat :=
  -- `0`-`9`;
  if 48 ≤ c ∧ c ≤ 57 then some (c - 48)
  -- `a`-`f`;
  else if 97 ≤ c ∧ c ≤ 102 then some (c - 87)
  -- anything else is not a hex digit.
  else none

-- Promise: every hex digit's character reads back as that digit.
theorem hexValue_hexChar (d : Nat) (h : d < 16) : hexValue (hexChar d) = some d := by
  -- Split on whether the digit is a decimal digit or a letter, then count.
  unfold hexChar hexValue
  split <;> split <;> (try split) <;> simp_all <;> omega

-- Read characters as hex digits; nothing if any is not one.
def readChars : List Nat → Option (List Nat)
  -- No characters: no digits;
  | [] => some []
  -- otherwise this character's value, then the rest.
  | c :: cs => (hexValue c).bind fun d => (readChars cs).map (d :: ·)

-- Promise: hex digits written as characters read back as the same digits.
theorem readChars_map (ds : List Nat) (h : ∀ d ∈ ds, d < 16) :
    readChars (ds.map hexChar) = some ds := by
  induction ds with
  -- No digits: nothing to read.
  | nil => rfl
  -- The first digit reads back, then the rest by induction.
  | cons d ds ih =>
    have hd := hexValue_hexChar d (h d (List.mem_cons_self d ds))
    have hrest := ih (fun x hx => h x (List.mem_cons_of_mem d hx))
    simp [readChars, hd, hrest]

-- `n` as 16 hex characters.
def hex16 (n : Nat) : List Nat := (hexDigits 16 n).map hexChar

-- The pointer's fixed start: `dsr1:`.
def prefixBytes : List Nat := [100, 115, 114, 49, 58]

-- The separator between the two fields: `:`.
def colon : Nat := 58

-- A pointer: the sequence number and the first 8 bytes of the record's hash.
structure Pointer where
  seq : Nat
  hash : Nat

-- Write a pointer: the start, the sequence number, a colon, the hash.
def print (p : Pointer) : List Nat := prefixBytes ++ hex16 p.seq ++ [colon] ++ hex16 p.hash

-- Read a pointer: the start, 16 hex characters, a colon, 16 more, then nothing.
def parse (bytes : List Nat) : Option Pointer :=
  if bytes.take 5 = prefixBytes ∧ (bytes.drop 5).length = 33 ∧ (bytes.drop 21).head? = some colon then
    match readChars ((bytes.drop 5).take 16), readChars ((bytes.drop 22).take 16) with
    -- Both fields are hex: the pointer they name;
    | some s, some h => some ⟨readHex s, readHex h⟩
    -- either is not: no pointer.
    | _, _ => none
  else none

-- Promise: a pointer whose fields fit 8 bytes each reads back as itself.
theorem parse_print (p : Pointer)
    -- Given the sequence number and the hash each fit in 8 bytes,
    (hs : p.seq < 16 ^ 16) (hh : p.hash < 16 ^ 16) :
    -- printing then parsing gives the same pointer.
    parse (print p) = some p := by
  -- Each field is exactly 16 characters, all of them hex.
  have ls : (hex16 p.seq).length = 16 := by simp [hex16, hexDigits_length]
  have lh : (hex16 p.hash).length = 16 := by simp [hex16, hexDigits_length]
  have rs := readChars_map (hexDigits 16 p.seq) (hexDigits_lt 16 p.seq)
  have rh := readChars_map (hexDigits 16 p.hash) (hexDigits_lt 16 p.hash)
  -- Cut the printed bytes at the field boundaries.
  have d5 : (print p).drop 5 = hex16 p.seq ++ [colon] ++ hex16 p.hash := by
    simp [print, prefixBytes, List.append_assoc]
  have t5 : (print p).take 5 = prefixBytes := by
    simp [print, prefixBytes, List.append_assoc]
  have d21 : (print p).drop 21 = [colon] ++ hex16 p.hash := by
    have : (print p).drop 21 = ((print p).drop 5).drop 16 := by simp [List.drop_drop]
    rw [this, d5, List.append_assoc, List.drop_left' ls]
  have d22 : (print p).drop 22 = hex16 p.hash := by
    have : (print p).drop 22 = ((print p).drop 21).drop 1 := by simp [List.drop_drop]
    rw [this, d21, List.singleton_append, List.drop_succ_cons, List.drop_zero]
  have f1 : ((print p).drop 5).take 16 = hex16 p.seq := by
    rw [d5, List.append_assoc, List.take_left' ls]
  -- Every check passes, and both fields read back.
  unfold parse
  rw [t5, f1, d5, d21, d22]
  have len : (hex16 p.seq ++ [colon] ++ hex16 p.hash).length = 33 := by
    simp only [List.length_append, ls, lh, List.length_singleton]
  have th : (hex16 p.hash).take 16 = hex16 p.hash := List.take_of_length_le (by omega)
  have head : ([colon] ++ hex16 p.hash).head? = some colon := by
    rw [List.singleton_append, List.head?_cons]
  rw [if_pos ⟨rfl, len, head⟩, th]
  -- Both fields read back as their digits, and the digits as their numbers.
  have rs' : readChars (hex16 p.seq) = some (hexDigits 16 p.seq) := rs
  have rh' : readChars (hex16 p.hash) = some (hexDigits 16 p.hash) := rh
  rw [rs', rh']
  simp only [readHex_hexDigits 16 p.seq hs, readHex_hexDigits 16 p.hash hh]

end ServiceRecord
