import OrderEncoding.Bytes
import OrderEncoding.Markers

/-!
# OrderEncoding.JsonPath — JSON path layout and descending range sentinels

The JSON index keeps the path bytes in ascending order and inverts only the
typed value. A normal descending entry is therefore

    path ++ [separator, pathEnd] ++ value

where `separator = ESCAPED_TERM` and `pathEnd = '/'`. The path itself is a
prefix, so it cannot be inverted without losing the unambiguous path/value
boundary (and, for array indices, without colliding with a longer property
name).

Rust anchors:
* `crates/engine/storage/src/encoding/json.rs`
  `encode_json_descending`, `encode_path_ascending`, `decode_json_descending`.
* `crates/engine/storage/src/encoding/json_array_path_tests.rs`
  round-trip, collision, and sentinel-order checks.

The two sentinels used by a descending range scan bracket every normal value
of that path:

* `PathMax = path ++ [separator, pathEnd]` is a strict prefix of a normal
  entry, so it sorts below every nonempty value.
* `PathMin = path ++ [separator, pathEnd + 1]` differs from a normal entry at
  the separator byte (`'/' + 1 > '/'`), so it sorts above every value.
-/

namespace OrderEncoding.JsonPath

open OrderEncoding

/-- `ESCAPED_TERM`, the byte before the JSON path terminator. -/
def separator : Nat := 1

/-- `'/'`, the JSON path terminator. -/
def pathEnd : Nat := 47

/-- A descending JSON entry: the ascending path, separator, terminator, value. -/
def entry (path value : Bytes) : Bytes :=
  path ++ separator :: pathEnd :: value

/-- The byte-order lower sentinel for a descending JSON path. -/
def pathMax (path : Bytes) : Bytes :=
  path ++ [separator, pathEnd]

/-- The byte-order upper sentinel for a descending JSON path. -/
def pathMin (path : Bytes) : Bytes :=
  path ++ [separator, pathEnd + 1]

/-- Appending the same path prefix preserves lexicographic order. -/
theorem append_lex_iff (path xs ys : Bytes) :
    xs <ₗ ys ↔ path ++ xs <ₗ path ++ ys := by
  induction path with
  | nil => exact Iff.rfl
  | cons byte rest ih =>
    simp only [List.cons_append]
    rw [lex_cons_iff, ih]

/-- `PathMax` is below every normal descending JSON entry with a nonempty
    value. -/
theorem path_max_below_value (path value : Bytes) (hne : value ≠ []) :
    pathMax path <ₗ entry path value := by
  have hvalue : [] <ₗ value := by
    cases value with
    | nil => exact absurd rfl hne
    | cons _ _ => exact Lex.nil
  unfold pathMax entry
  exact
    (append_lex_iff path [separator, pathEnd] (separator :: pathEnd :: value)).mp
      (Lex.cons (Lex.cons hvalue))

/-- `PathMin` is above every normal descending JSON entry. -/
theorem path_min_above_value (path value : Bytes) :
    entry path value <ₗ pathMin path := by
  unfold entry pathMin
  exact
    (append_lex_iff path (separator :: pathEnd :: value) [separator, pathEnd + 1]).mp
      (Lex.cons (Lex.head (by decide)))

/-- The two sentinels bracket every normal descending JSON entry. -/
theorem descending_json_sentinels_bracket_values
    (path value : Bytes) (hne : value ≠ []) :
    pathMax path <ₗ entry path value ∧ entry path value <ₗ pathMin path :=
  ⟨path_max_below_value path value hne, path_min_above_value path value⟩

/-- A valid UTF-8 leading byte is never the zero-index marker, which lies
    among UTF-8 continuation bytes. This models the first-byte distinction. -/
theorem utf8_lead_is_not_array_marker (byte : Nat)
    (h : byte ≤ 127 ∨ (194 ≤ byte ∧ byte ≤ 244)) : byte ≠ 136 := by
  omega

/-! ## Where an empty array entry sits

An empty array inside a JSON value is stored at its path with the value
`EMPTY_ARRAY` (17) in an ascending index and `EMPTY_ARRAY_DESC` (18) in a
descending one. A range scan of the path runs between the path's two
sentinels, so it passes over these entries and skips them; the `_all` scan
looks them up by their exact key. What must never happen: an empty array entry
lands outside its path's sentinels, where a scan of a neighbouring path would
read it as one of its own values.

Rust anchors: `encode_scalar_ascending`, `encode_scalar_descending` in
`crates/engine/storage/src/encoding/json.rs`, and
`an_empty_array_round_trips_between_its_path_sentinels` in
`crates/engine/storage/src/encoding/json_array_path_tests.rs`.
-/

/-- A descending empty array entry sits between the two sentinels of its path. -/
theorem descending_empty_array_bracketed (path : Bytes) :
    -- above `PathMax` and below `PathMin`
    pathMax path <ₗ entry path [mEmptyArrayDesc] ∧
      entry path [mEmptyArrayDesc] <ₗ pathMin path :=
  -- the value is one byte, so it is not empty and the general bracket applies
  descending_json_sentinels_bracket_values path [mEmptyArrayDesc] (by simp)

/-- An ascending empty array entry sits between the two ascending sentinels:
    above `PathMin`, which ends with the null byte 0, and below `PathMax`, which
    writes the next separator byte, the same bytes `pathMin` names above. -/
theorem ascending_empty_array_bracketed (path : Bytes) :
    -- above the ascending `PathMin`, whose value byte is the null 0
    entry path [mNull] <ₗ entry path [mEmptyArray] ∧
      -- and below the ascending `PathMax`
      entry path [mEmptyArray] <ₗ pathMin path := by
  -- both parts
  constructor
  · -- the shared path drops out; then the separator and terminator match, and
    -- the value byte decides: 0 < 17
    exact (append_lex_iff path _ _).mp
      (Lex.cons (Lex.cons (Lex.head (by decide))))
  · -- the general fact that every entry is below the next separator byte
    exact path_min_above_value path [mEmptyArray]

end OrderEncoding.JsonPath
