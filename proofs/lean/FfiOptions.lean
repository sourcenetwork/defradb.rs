/-!
The story: a program calls the DefraDB C library with an options struct. The
program was built against one version of the header, the library against
another, so their structs can differ in length: a struct only ever grows at
its end, and zero is every field's default. The program passes its struct's
bytes and how many there are. The library reads its own struct from them.
Every byte of a struct belongs to a field: the structs have no hidden padding
(a gap is filled with a reserved field), so a byte past an older struct's end
is always part of a field added since, never filler. Every struct's length is
a multiple of 8, and no field crosses an 8-byte boundary, so a length that is
a multiple of 8 always ends between two fields.

What must never happen: a byte the program set, inside the library's struct,
read as something else; a field the program's older struct does not have read
as anything but zero (its default); a field the program set that the library
does not know silently ignored; a newer program that leaves its new fields
unset read differently from an older one; a length that ends inside a field
accepted, so that half a field (half a pointer) is read.

Rust owners:
  crates/entry-points/ffi/src/options.rs   read (the size-prefix copy), ZeroDefault
-/

namespace FfiOptions

-- The caller's bytes, as a list of byte values.
abbrev Bytes := List Nat

-- Byte `i` of the caller's struct, or zero past its end.
def byteAt (given : Bytes) (i : Nat) : Nat :=
  -- a list read past its end answers zero
  given.getD i 0

-- Whether the caller's length ends between two fields: every field boundary
-- the structs can end on is a multiple of 8.
def endsBetweenFields (given : Bytes) : Prop :=
  -- the length is a whole number of 8-byte words
  given.length % 8 = 0

-- Whether the library accepts the caller's struct when its own struct is
-- `known` bytes long: the caller's length ends between fields, and every
-- byte past `known` the caller gave is zero.
def accepts (given : Bytes) (known : Nat) : Prop :=
  -- the length does not stop inside a field
  endsBetweenFields given ∧
  -- and no byte of the caller's beyond the library's struct is set
  ∀ i, known ≤ i → i < given.length → byteAt given i = 0

-- Byte `i` of the struct the library reads: the caller's byte inside its
-- own struct (zero past the caller's end), and nothing past its own end.
def readAt (given : Bytes) (known : Nat) (i : Nat) : Nat :=
  -- inside the library's struct the caller's byte, or zero past the caller's end
  if i < known then byteAt given i else 0

-- A byte the caller gave, inside the library's struct, is read as given.
theorem keeps_what_the_caller_gave (given : Bytes) (known i : Nat)
    (inside : i < known) (h : i < given.length) :
    readAt given known i = given[i] := by
  -- inside the library's struct the read is the caller's byte
  simp [readAt, byteAt, inside, List.getD_eq_getElem?_getD, List.getElem?_eq_getElem h]

-- A field past the caller's struct, the caller being older, reads as zero:
-- its default.
theorem older_caller_gets_defaults (given : Bytes) (known i : Nat)
    (past : given.length ≤ i) :
    readAt given known i = 0 := by
  -- past the caller's end there is nothing to read, inside or outside
  unfold readAt byteAt
  split <;> simp [List.getD_eq_getElem?_getD, List.getElem?_eq_none past]

-- A set byte past the library's struct means the caller set a field this
-- library does not know: the struct is refused, not silently cut short.
theorem refuses_an_unknown_set_field (given : Bytes) (known i : Nat)
    (past : known ≤ i) (h : i < given.length) (set : byteAt given i ≠ 0) :
    ¬ accepts given known := by
  -- the set byte is exactly what the second half of `accepts` rules out
  intro ok
  exact set (ok.2 i past h)

-- A length that is not a multiple of 8 stops inside a field, so the struct is
-- refused: the library never reads half a field.
theorem refuses_a_length_inside_a_field (given : Bytes) (known : Nat)
    (inside : given.length % 8 ≠ 0) :
    ¬ accepts given known := by
  -- the first half of `accepts` is exactly the length check that fails
  intro ok
  exact inside ok.1

-- A caller that leaves its trailing fields unset is read exactly as a caller
-- whose struct stops before them.
theorem unset_new_fields_read_as_absent (given : Bytes) (known extra : Nat) (i : Nat) :
    readAt (given ++ List.replicate extra 0) known i = readAt given known i := by
  -- the appended zeros read the same as reading past the end
  unfold readAt byteAt
  split
  · rcases Nat.lt_or_ge i given.length with h | h
    · simp [List.getD_eq_getElem?_getD, List.getElem?_append_left h]
    · simp [List.getD_eq_getElem?_getD, List.getElem?_append_right h,
        List.getElem?_eq_none h, List.getElem?_replicate]
      split <;> rfl
  · rfl

-- A newer caller's unset new fields are always accepted: both the older
-- struct and the fields appended to it are whole 8-byte words.
theorem unset_new_fields_are_accepted (given : Bytes) (known extra : Nat)
    (fits : given.length ≤ known) (words : given.length % 8 = 0)
    (newWords : extra % 8 = 0) :
    accepts (given ++ List.replicate extra 0) known := by
  -- prove the two halves of `accepts` one at a time
  constructor
  -- whole words plus whole words is a whole number of words
  · simp [endsBetweenFields, List.length_append, List.length_replicate, Nat.add_mod,
      words, newWords]
  -- every byte past the library's struct is one of the appended zeros
  · intro i past h
    have hi : given.length ≤ i := Nat.le_trans fits past
    simp [byteAt, List.getD_eq_getElem?_getD, List.getElem?_append_right hi,
      List.getElem?_replicate]
    split <;> rfl

end FfiOptions
