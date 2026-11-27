import OrderEncoding.Bytes

/-!
# OrderEncoding.JsonEmptyArray — the empty-array marker in a JSON index key

The story. A JSON index writes one key per value it finds inside a document:
the value's path, then two bytes that end the path (`1`, `47`), then the value.
A value starts with a byte that names its type: null, false, true, a number
(negative, zero, positive or NaN) or a string. An empty array has no value to
write, so a query asking for "every element passes" (`_all`), which an empty
array always passes, could never find it. The fix writes one extra key for
each empty array: the path its elements would have, the path end, and then a
marker byte instead of a value.

What can happen: a scan reads the keys between two bounds in byte order, and a
read decodes a key back into the value it holds.

What must never happen:
* the marker starts like some real value, so a lookup of that value (an `_eq`)
  would find the marker, or a stored value would read back as the marker;
* the marker breaks the order of the real values, which every range scan
  depends on;
* the marker leaves its path, so a scan of another path meets it;
* the marker does not read back as itself.

What this file proves:
* no value starts with the marker, ascending or descending
  (`marker_distinct_asc`, `marker_distinct_desc`);
* ascending, the marker sorts after every value on its path; descending,
  before every one (`value_lt_marker_asc`, `marker_lt_value_desc`), and values
  keep exactly the bytes they had (`values_unchanged_asc`,
  `values_unchanged_desc`), so their order is the one `OrderEncoding` proves;
* the marker stays between the path's lowest and highest bound
  (`marker_inside_path_asc`, `marker_inside_path_desc`);
* the marker reads back as itself, and every value reads back as it did
  before (`decode_marker_asc`, `decode_value_asc`, and the descending pair).

Both directions write the path ascending (`OrderEncoding.JsonPath`), so the
array indexes in front of the marker read back by the ascending rule in a
descending key too: an index is the byte 136, which no UTF-8 name starts with
(`JsonPath.utf8_lead_is_not_array_marker`).

The RED side: a marker on null's byte reads a stored null back as the marker
(`neg_marker_on_null_reads_null_as_marker`), and a marker between the numbers
and the strings sorts below a string (`neg_marker_inside_values_breaks_order`).

Rust owner: crates/engine/storage/src/encoding/json.rs `JSON_EMPTY_ARRAY`,
`JSON_EMPTY_ARRAY_DESC`, `encode_scalar_ascending`, `encode_scalar_descending`,
`decode_scalar_ascending`, `decode_scalar_descending`, `encode_json_descending`,
`decode_json_descending`;
the markers come from
crates/building-blocks/document/src/json_leaf.rs `json_empty_array_markers`.
-/

namespace OrderEncoding

namespace JsonEmptyArray

-- The kinds of JSON value an index key can hold, named by their first byte.
inductive Tag where
  -- a JSON null
  | null
  -- the boolean false
  | fals
  -- the boolean true
  | tru
  -- a number that is not a number (never in JSON, kept to be safe)
  | nan
  -- a negative number
  | neg
  -- zero
  | zero
  -- a positive number
  | pos
  -- a string
  | str
  -- let Lean compare two tags and list the cases for `decide`
  deriving DecidableEq, Repr

-- The first byte of a value written ascending (`encode_*_ascending`).
def ascByte : Tag → Nat
  -- `ENCODED_NULL`
  | .null => 0
  -- `FALSE_MARKER`
  | .fals => 9
  -- `TRUE_MARKER`
  | .tru => 10
  -- `FLOAT64_NAN`
  | .nan => 1
  -- `FLOAT64_NEG`
  | .neg => 2
  -- `FLOAT64_ZERO`
  | .zero => 3
  -- `FLOAT64_POS`
  | .pos => 4
  -- `BYTES_MARKER`
  | .str => 6

-- The first byte of a value written descending (`encode_*_descending`).
def descByte : Tag → Nat
  -- `ENCODED_NULL_DESC`
  | .null => 255
  -- a descending bool is the ascending one of its opposite: false writes `TRUE_MARKER`
  | .fals => 10
  -- and true writes `FALSE_MARKER`
  | .tru => 9
  -- `FLOAT64_NAN_DESC`
  | .nan => 5
  -- a descending number is the ascending one of its negation: negative becomes positive
  | .neg => 4
  -- zero stays zero
  | .zero => 3
  -- positive becomes negative
  | .pos => 2
  -- `BYTES_DESC_MARKER`
  | .str => 7

-- `JSON_EMPTY_ARRAY`: the marker written ascending.
def markerAsc : Nat := 254

-- `JSON_EMPTY_ARRAY_DESC`: its complement, written descending.
def markerDesc : Nat := 255 - markerAsc

-- What sits after the path end in a key: a value of some kind with its body, or the marker.
inductive Entry where
  -- a real value: its kind and the bytes after its first byte
  | value (t : Tag) (body : Bytes)
  -- the empty-array marker
  | emptyArray

-- The bytes an entry is written as, ascending.
def encAsc : Entry → Bytes
  -- a value is its first byte, then its body
  | .value t body => ascByte t :: body
  -- the marker is one byte
  | .emptyArray => [markerAsc]

-- The bytes an entry is written as, descending.
def encDesc : Entry → Bytes
  -- a value is its descending first byte, then its body
  | .value t body => descByte t :: body
  -- the marker is one byte
  | .emptyArray => [markerDesc]

-- The two bytes that end a path (`ESCAPED_TERM`, `JSON_PATH_END`).
def pathEnd : Bytes := [1, 47]

-- A whole key: the path bytes, the path end, then the entry. Descending keys
-- have the same layout: only the entry is written descending.
def key (path : Bytes) (entry : Bytes) : Bytes := path ++ (pathEnd ++ entry)

/-! ## A shared prefix does not change an order -/

-- Promise: putting the same bytes in front of two keys keeps their order, both ways.
theorem lex_append_left : ∀ (p a b : Bytes), (p ++ a) <ₗ (p ++ b) ↔ a <ₗ b
  -- with nothing in front, there is nothing to show
  | [], _, _ => Iff.rfl
  -- with one more byte in front, equal heads leave the rest to decide
  | x :: p, a, b => by
    -- `x :: p ++ a` is `x :: (p ++ a)`, and equal heads compare by their tails
    simp only [List.cons_append]
    -- the tails are the shorter case
    exact lex_cons_iff.trans (lex_append_left p a b)

/-! ## The marker starts like no value -/

-- Promise: no value written ascending starts with the ascending marker.
theorem marker_distinct_asc (t : Tag) : ascByte t ≠ markerAsc := by
  -- check each kind of value
  cases t <;> decide

-- Promise: no value written descending starts with the descending marker.
theorem marker_distinct_desc (t : Tag) : descByte t ≠ markerDesc := by
  -- check each kind of value
  cases t <;> decide

/-! ## Order: values keep theirs, the marker sorts at one end -/

-- Promise: ascending, a value is written exactly as before the marker existed.
theorem values_unchanged_asc (t : Tag) (body : Bytes) :
    encAsc (.value t body) = ascByte t :: body := rfl

-- Promise: descending, a value is written exactly as before the marker existed.
theorem values_unchanged_desc (t : Tag) (body : Bytes) :
    encDesc (.value t body) = descByte t :: body := rfl

-- Promise: ascending, every value on a path sorts before that path's marker.
theorem value_lt_marker_asc (path body : Bytes) (t : Tag) :
    key path (encAsc (.value t body)) <ₗ key path (encAsc .emptyArray) := by
  -- the path and its end are shared, so compare what follows
  unfold key
  rw [lex_append_left, lex_append_left]
  -- the first bytes differ, and the value's is smaller
  exact Lex.head (by cases t <;> decide)

-- Promise: descending, a path's marker sorts before every value on it.
theorem marker_lt_value_desc (path body : Bytes) (t : Tag) :
    key path (encDesc .emptyArray) <ₗ key path (encDesc (.value t body)) := by
  -- the path and its end are shared, so compare what follows
  unfold key
  rw [lex_append_left, lex_append_left]
  -- the first bytes differ, and the marker's is smaller
  exact Lex.head (by cases t <;> decide)

/-! ## The marker stays inside its path -/

-- The lowest bound of a path ascending (`PathMin`): the path end and a null.
def pathMinAsc (path : Bytes) : Bytes := key path [0]

-- The highest bound of a path ascending (`PathMax`): the path end with its last byte raised.
def pathMaxAsc (path : Bytes) : Bytes := path ++ [1, 48]

-- Promise: ascending, the marker lies above the path's lowest bound and below its highest.
theorem marker_inside_path_asc (path : Bytes) :
    pathMinAsc path <ₗ key path (encAsc .emptyArray)
    ∧ key path (encAsc .emptyArray) <ₗ pathMaxAsc path := by
  -- both sides share the path, so compare what follows it
  unfold pathMinAsc pathMaxAsc key
  constructor
  -- after the path end, null's 0 is below the marker's 254
  · rw [lex_append_left]; exact Lex.cons (Lex.cons (Lex.head (by decide)))
  -- the path end's 47 is below the bound's 48
  · rw [lex_append_left]; exact Lex.cons (Lex.head (by decide))

-- The lowest bound of a path descending (`PathMax`): the path end and nothing after it.
def pathLowDesc (path : Bytes) : Bytes := path ++ [1, 47]

-- The highest bound of a path descending (`PathMin`): the path end with its last byte raised, then a descending null.
def pathHighDesc (path : Bytes) : Bytes := path ++ [1, 48, 255]

-- Promise: descending, the marker lies above the path's lowest bound and below its highest.
theorem marker_inside_path_desc (path : Bytes) :
    pathLowDesc path <ₗ key path (encDesc .emptyArray)
    ∧ key path (encDesc .emptyArray) <ₗ pathHighDesc path := by
  -- both sides share the path, so compare what follows it
  unfold pathLowDesc pathHighDesc key
  constructor
  -- the bound is the path end alone, a strict prefix of the marker's key
  · rw [lex_append_left]; exact Lex.cons (Lex.cons Lex.nil)
  -- the path end's 47 is below the bound's 48
  · rw [lex_append_left]; exact Lex.cons (Lex.head (by decide))

/-! ## Reading a key back -/

-- A decoder reads the entry at the front of some bytes and returns it with the bytes left over.
abbrev Decoder := Bytes → Option (Entry × Bytes)

-- The decoder with the marker: the marker byte first, anything else to the value decoder `dec`.
def decodeWith (marker : Nat) (dec : Decoder) : Decoder
  -- nothing to read
  | [] => none
  -- the marker byte reads as the marker; any other byte goes to `dec` untouched
  | b :: rest => if b = marker then some (.emptyArray, rest) else dec (b :: rest)

-- Promise: ascending, the marker reads back as itself, with the rest left over.
theorem decode_marker_asc (dec : Decoder) (rest : Bytes) :
    decodeWith markerAsc dec (encAsc .emptyArray ++ rest) = some (.emptyArray, rest) := by
  -- the first byte is the marker
  simp [decodeWith, encAsc]

-- Promise: ascending, a value reads back exactly as the value decoder alone reads it.
theorem decode_value_asc (dec : Decoder) (t : Tag) (body rest : Bytes) :
    decodeWith markerAsc dec (encAsc (.value t body) ++ rest)
      = dec (encAsc (.value t body) ++ rest) := by
  -- the first byte is the value's, which is not the marker
  simp [decodeWith, encAsc, marker_distinct_asc t]

-- Promise: descending, the marker reads back as itself, with the rest left over.
theorem decode_marker_desc (dec : Decoder) (rest : Bytes) :
    decodeWith markerDesc dec (encDesc .emptyArray ++ rest) = some (.emptyArray, rest) := by
  -- the first byte is the marker
  simp [decodeWith, encDesc]

-- Promise: descending, a value reads back exactly as the value decoder alone reads it.
theorem decode_value_desc (dec : Decoder) (t : Tag) (body rest : Bytes) :
    decodeWith markerDesc dec (encDesc (.value t body) ++ rest)
      = dec (encDesc (.value t body) ++ rest) := by
  -- the first byte is the value's, which is not the marker
  simp [decodeWith, encDesc, marker_distinct_desc t]

/-! ## RED: nearby markers that fail -/

-- Promise, RED: a marker on null's byte 0 reads a stored null back as the
-- marker, so an `_all` lookup would find every null and a read would lose it.
theorem neg_marker_on_null_reads_null_as_marker (dec : Decoder) (body rest : Bytes) :
    decodeWith 0 dec (encAsc (.value .null body) ++ rest) = some (.emptyArray, body ++ rest) := by
  -- null's first byte is 0, the planted marker
  simp [decodeWith, encAsc, ascByte]

-- Promise, RED: a marker on byte 5, between the numbers and the strings, sorts
-- below a string, so a scan from a number to the top of the path no longer
-- tells the marker from the values by its place.
theorem neg_marker_inside_values_breaks_order (path body : Bytes) :
    ¬ (key path (ascByte .str :: body) <ₗ key path [5]) := by
  -- the path and its end are shared, so compare what follows
  unfold key
  rw [lex_append_left, lex_append_left]
  -- a string's 6 is above 5, so the string cannot come first
  intro h
  cases h with
  -- the first bytes decide: 6 < 5 is false
  | head h => exact absurd h (by decide)

end JsonEmptyArray

end OrderEncoding
