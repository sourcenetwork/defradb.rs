/-!
# KvSnapshot.Frames: a KV snapshot reads back as the entries written

The story, in plain words. One node exports some documents as a stream of
key-value entries, and another node imports that stream. Each entry is a frame:
four bytes giving the key's length, the key, four bytes giving the value's
length, the value. Every key starts with a byte naming its store, so no key is
empty, and a key length of zero marks the end of the stream. Go Shinzo writes
this format and reads it, and so does defradb.rs.

What must never happen:
* an entry comes back different, out of order, or a later entry is misread
  (`decode_encode`);
* the reader runs past the end mark into bytes that follow it
  (`decode_encode`, with any `rest`);
* a stream cut part-way through a frame is read as complete
  (`decode_cut_length`, `decode_cut_key`, `decode_cut_value_length`,
  `decode_cut_value`).

A stream that stops cleanly between two frames, with no end mark, is read as
complete (`decode_nil`): Go's reader stops at a clean end of file the same way.

Bytes are modelled as natural numbers below 256; what the keys and values
mean (Go's datastore and headstore keys, block bytes) is not covered here.

Rust anchors: `write_entry`, `write_end`, `read_entries` in
`crates/db/src/kv_snapshot/frame.rs`.
-/

namespace KvSnapshot

-- A length as four big-endian bytes, as `u32::to_be_bytes` writes it.
def be32 (n : Nat) : List Nat :=
  [n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256]

-- Read four big-endian bytes as a length; nothing if fewer than four remain.
def readBe32 : List Nat → Option (Nat × List Nat)
  -- Four bytes or more: combine the first four, keep the rest;
  | a :: b :: c :: d :: rest => some (a * 16777216 + b * 65536 + c * 256 + d, rest)
  -- fewer: the length was cut off.
  | _ => none

-- Promise: any length a `u32` can hold reads back as itself, and the bytes
-- after it are left untouched.
theorem readBe32_be32 (n : Nat) (rest : List Nat) (h : n < 4294967296) :
    readBe32 (be32 n ++ rest) = some (n, rest) := by
  -- Unfold both sides; what is left is arithmetic on the four digits.
  simp only [be32, readBe32, List.cons_append, List.nil_append, Option.some.injEq,
    Prod.mk.injEq, and_true]
  omega

-- Promise: fewer than four bytes never read as a length.
theorem readBe32_short (bytes : List Nat) (h : bytes.length < 4) :
    readBe32 bytes = none := by
  -- Each list shorter than four is one of the short patterns.
  match bytes, h with
  | [], _ => rfl
  | [_], _ => rfl
  | [_, _], _ => rfl
  | [_, _, _], _ => rfl

-- One entry: a key and its value.
abbrev Entry := List Nat × List Nat

-- An entry is writable when its key is not empty (an empty key would read as
-- the end mark) and both lengths fit a `u32`.
def Entry.ok (e : Entry) : Prop :=
  0 < e.1.length ∧ e.1.length < 4294967296 ∧ e.2.length < 4294967296

-- One frame: key length, key, value length, value.
def encodeEntry (e : Entry) : List Nat :=
  be32 e.1.length ++ e.1 ++ be32 e.2.length ++ e.2

-- A whole stream: every frame in order, then the end mark, a zero key length.
def encode (es : List Entry) : List Nat :=
  (es.map encodeEntry).flatten ++ be32 0

-- Read frames until the end mark or a clean end of the bytes. `fuel` bounds
-- the number of frames, so the reader always stops; nothing (an error) when a
-- frame is cut part-way.
def decode : Nat → List Nat → Option (List Entry)
  -- Out of fuel: refuse rather than guess.
  | 0, _ => none
  | fuel + 1, bytes =>
    match bytes with
    -- Nothing left between frames: the stream is complete.
    | [] => some []
    | _ =>
      match readBe32 bytes with
      -- A key length cut off: an error.
      | none => none
      | some (klen, rest) =>
        -- A zero key length is the end mark: stop, ignoring what follows.
        if klen = 0 then some []
        -- The key must be all here,
        else if klen ≤ rest.length then
          match readBe32 (rest.drop klen) with
          -- and the value length after it,
          | none => none
          | some (vlen, rest') =>
            -- and the value after that; then read the next frame.
            if vlen ≤ rest'.length then
              (decode fuel (rest'.drop vlen)).map ((rest.take klen, rest'.take vlen) :: ·)
            else none
        else none

-- Promise: a stream that is already at its clean end reads as no entries.
theorem decode_nil (fuel : Nat) : decode (fuel + 1) [] = some [] := by
  -- The first match arm.
  rfl

-- The end mark followed by anything reads as no entries.
theorem decode_end (fuel : Nat) (rest : List Nat) :
    decode (fuel + 1) (be32 0 ++ rest) = some [] := by
  -- The four zero bytes read as length 0, which is the end mark.
  simp [decode, be32, readBe32]

-- Promise: writing entries and reading them back gives the same entries in
-- the same order, and the reader stops at the end mark whatever follows it,
-- given one unit of fuel per frame plus one for the end mark.
theorem decode_encode (es : List Entry) (rest : List Nat)
    -- Given every entry is writable,
    (hok : ∀ e ∈ es, e.ok)
    -- and the reader has fuel for every frame and the end mark,
    (fuel : Nat) (hfuel : es.length < fuel) :
    -- the stream reads back as exactly those entries.
    decode fuel (encode es ++ rest) = some es := by
  -- One frame at a time, from the front.
  induction es generalizing fuel with
  | nil =>
    -- No frames: only the end mark is left.
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp at hfuel; omega⟩
    simp only [encode, List.map_nil, List.flatten_nil, List.nil_append]
    exact decode_end f rest
  | cons e es ih =>
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp at hfuel; omega⟩
    obtain ⟨hk0, hk, hv⟩ := hok e (by simp)
    -- The frames after this one read back, by the induction.
    have htail := ih (fun x hx => hok x (by simp [hx])) f (by simp at hfuel; omega)
    -- This frame, then the rest of the stream.
    have hstream : encode (e :: es) ++ rest =
        be32 e.1.length ++ (e.1 ++ (be32 e.2.length ++ (e.2 ++ (encode es ++ rest)))) := by
      simp [encode, encodeEntry, List.append_assoc]
    -- The key length reads back,
    have hk1 := readBe32_be32 e.1.length
      (e.1 ++ (be32 e.2.length ++ (e.2 ++ (encode es ++ rest)))) hk
    -- and, after the key, the value length.
    have hv1 := readBe32_be32 e.2.length (e.2 ++ (encode es ++ rest)) hv
    rw [hstream]
    -- Walk the reader through this frame.
    unfold decode
    split
    · -- The stream is not empty: it starts with four length bytes.
      next heq => simp [be32] at heq
    · rw [hk1]
      simp [Nat.pos_iff_ne_zero.mp hk0, hv1, htail]

-- Promise: a stream cut inside a key length is an error, not a short stream.
theorem decode_cut_length (fuel : Nat) (bytes : List Nat)
    -- Given one to three bytes remain where a key length should be,
    (h0 : 0 < bytes.length) (h : bytes.length < 4) :
    decode (fuel + 1) bytes = none := by
  -- They are not empty, and too short to be a length.
  unfold decode
  split
  · simp at h0
  · rw [readBe32_short bytes h]

-- Promise: a stream cut inside a key is an error.
theorem decode_cut_key (fuel : Nat) (key : List Nat) (n : Nat)
    -- Given a non-empty key whose length fits a `u32`, cut after `n` of its
    -- bytes with `n` short of the whole key,
    (hk0 : 0 < key.length) (hk : key.length < 4294967296) (hn : n < key.length) :
    decode (fuel + 1) (be32 key.length ++ key.take n) = none := by
  have h1 := readBe32_be32 key.length (key.take n) hk
  unfold decode
  split
  · next heq => simp [be32] at heq
  · rw [h1]
    -- The announced key length is more than the bytes that remain.
    simp [Nat.pos_iff_ne_zero.mp hk0]
    intro h
    exfalso
    omega

-- Promise: a stream cut inside a value length is an error.
theorem decode_cut_value_length (fuel : Nat) (key part : List Nat)
    -- Given a whole key, then fewer than four bytes of the value length,
    (hk0 : 0 < key.length) (hk : key.length < 4294967296) (hp : part.length < 4) :
    decode (fuel + 1) (be32 key.length ++ (key ++ part)) = none := by
  have h1 := readBe32_be32 key.length (key ++ part) hk
  unfold decode
  split
  · next heq => simp [be32] at heq
  · rw [h1]
    simp [Nat.pos_iff_ne_zero.mp hk0, readBe32_short part hp]

-- Promise: a stream cut inside a value is an error.
theorem decode_cut_value (fuel : Nat) (key value : List Nat) (n : Nat)
    -- Given a whole key and value length, then `n` bytes short of the value,
    (hk0 : 0 < key.length) (hk : key.length < 4294967296)
    (hv : value.length < 4294967296) (hn : n < value.length) :
    decode (fuel + 1) (be32 key.length ++ (key ++ (be32 value.length ++ value.take n))) = none := by
  have h1 := readBe32_be32 key.length (key ++ (be32 value.length ++ value.take n)) hk
  have h2 := readBe32_be32 value.length (value.take n) hv
  unfold decode
  split
  · next heq => simp [be32] at heq
  · rw [h1]
    -- The announced value length is more than the bytes that remain.
    simp [Nat.pos_iff_ne_zero.mp hk0, h2]
    intro h
    exfalso
    omega

end KvSnapshot
