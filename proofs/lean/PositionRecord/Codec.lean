/-!
# PositionRecord.Codec: a position record reads back as the keys it names

The story, in plain words. A deferred index does not move its entries when a
counter changes, because the write never learns the new value. A drain moves
them later, and to find the old entries it keeps one position record per
document: the exact keys of the entries the index holds for it. A document
with an array field can have several entries, so a record is a list of keys.
Each key is written as four bytes giving its length, then the key itself, one
after another. An empty record names no entries.

What must never happen: a record reads back as a different list of keys, in a
different order, or with keys split or joined differently
(`decode_encode`). If it did, the drain would delete the wrong entries and
leave the real ones behind.

Bytes are modelled as natural numbers below 256.

Rust anchors: `encode_record`, `decode_record` in
`crates/db/src/index/deferred/record.rs`.
-/

namespace PositionRecord

-- A length as four big-endian bytes, as `u32::to_be_bytes` writes it.
def be32 (n : Nat) : List Nat :=
  [n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256]

-- Read four big-endian bytes as a length; nothing if fewer than four remain.
def readBe32 : List Nat → Option (Nat × List Nat)
  -- Four bytes or more: combine the first four, keep the rest;
  | a :: b :: c :: d :: rest => some (a * 16777216 + b * 65536 + c * 256 + d, rest)
  -- fewer: a broken record.
  | _ => none

-- Promise: any length a `u32` can hold reads back as itself, and the bytes
-- after it are left untouched.
theorem readBe32_be32 (n : Nat) (rest : List Nat) (h : n < 4294967296) :
    readBe32 (be32 n ++ rest) = some (n, rest) := by
  -- Unfold both sides; what is left is arithmetic on the four digits.
  simp only [be32, readBe32, List.cons_append, List.nil_append, Option.some.injEq,
    Prod.mk.injEq, and_true]
  omega

-- A record: each key's length, then the key, one key after another.
def encode : List (List Nat) → List Nat
  -- No keys: no bytes.
  | [] => []
  -- A key, then the rest of the record.
  | k :: ks => be32 k.length ++ k ++ encode ks

-- Read a record until its bytes run out. `fuel` bounds the keys read, so the
-- reading always ends; a broken length gives nothing.
def decode : Nat → List Nat → Option (List (List Nat))
  -- No bytes left: every key has been read.
  | _, [] => some []
  -- Bytes left but no fuel: more keys than the bound allows.
  | 0, _ :: _ => none
  -- Bytes left: read a length, take that many bytes as a key, read on.
  | fuel + 1, b :: bs =>
    match readBe32 (b :: bs) with
    | none => none
    | some (len, rest) =>
      if len ≤ rest.length then (decode fuel (rest.drop len)).map (rest.take len :: ·) else none

-- A record of at least one key is not empty: its first length takes four bytes.
theorem encode_cons (k : List Nat) (ks : List (List Nat)) :
    ∃ b bs, encode (k :: ks) = b :: bs := by
  -- The first byte is the top byte of the key's length.
  exact ⟨_, _, rfl⟩

-- Promise: with fuel for every key, a record reads back as exactly its keys,
-- in order.
theorem decode_encode_fuel (ks : List (List Nat))
    -- Given every key's length fits a `u32`,
    (hfit : ∀ k ∈ ks, k.length < 4294967296) :
    -- any fuel at least the number of keys reads them all back.
    ∀ fuel, ks.length ≤ fuel → decode fuel (encode ks) = some ks := by
  -- Induct on the keys.
  induction ks with
  -- No keys: no bytes, read as no keys.
  | nil => intro fuel _; cases fuel <;> rfl
  -- A key then the rest: the key reads back, then the rest do.
  | cons k ks ih =>
    intro fuel hfuel
    -- The fuel covers this key, so it is one more than some smaller fuel.
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp at hfuel; omega⟩
    -- The record starts with a byte, so the reading takes the third case.
    obtain ⟨b, bs, hbytes⟩ := encode_cons k ks
    rw [hbytes]
    simp only [decode]
    -- Those bytes are the key's length, the key, then the rest of the record.
    rw [← hbytes]
    have hk := hfit k (by simp)
    have hlen : readBe32 (encode (k :: ks)) = some (k.length, k ++ encode ks) := by
      simp only [encode, List.append_assoc]
      exact readBe32_be32 k.length (k ++ encode ks) hk
    rw [hlen]
    -- The key is all there, so it is taken and the rest read on.
    have hrest : ∀ q ∈ ks, q.length < 4294967296 := fun q hq => hfit q (by simp [hq])
    simp only [List.length_append, Nat.le_add_right, if_true, List.take_left',
      List.drop_left']
    rw [ih hrest f (by simp at hfuel; omega)]
    rfl

-- Promise: a record reads back as exactly its keys, in order, when read with
-- its own length as fuel, as the code does.
theorem decode_encode (ks : List (List Nat))
    -- Given every key's length fits a `u32`,
    (hfit : ∀ k ∈ ks, k.length < 4294967296) :
    -- the record reads back exactly.
    decode (encode ks).length (encode ks) = some ks := by
  -- Every key takes at least four bytes, so the length is enough fuel.
  apply decode_encode_fuel ks hfit
  -- Count: each key adds four bytes or more to the record.
  induction ks with
  | nil => simp [encode]
  | cons k ks ih =>
    have hrest : ∀ q ∈ ks, q.length < 4294967296 := fun q hq => hfit q (by simp [hq])
    have := ih hrest
    simp only [encode, List.length_append, List.length_cons, be32]
    simp
    omega

end PositionRecord
