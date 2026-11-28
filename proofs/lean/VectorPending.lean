/-!
# VectorPending: a deferred vector index's pending entry reads back as written

The story, in plain words. A vector index declared `maintenance: DEFERRED`
does not touch its graph when a document is written. It stores one pending
entry for the document's node instead, and a drain applies it to the graph
later. The entry is one byte saying what to do (1: set the vector, 2: remove
the node), then the vector, each element as the four little-endian bytes of
its `f32` bits. A search and the drain both read entries back.

What must never happen: an entry reads back as another operation or another
vector (`decode_encode`); an empty value, a value cut inside an element, or an
unknown operation reads as an entry at all (`decode_empty`,
`decode_partial_element`, `decode_unknown_op`). Either would make the drain
apply a vector nobody wrote.

An element is modelled by its 32 bits, a number below 2^32: `f32::to_le_bytes`
and `f32::from_le_bytes` move exactly those bits. Bytes are numbers below 256.

Rust anchors: `encode_value`, `decode_value` in
`crates/database/db/src/index/vector/pending/entry.rs`.
-/

namespace VectorPending

-- Four little-endian bytes of a 32-bit word, as `f32::to_le_bytes` writes its bits.
def le32 (w : Nat) : List Nat :=
  [w % 256, w / 256 % 256, w / 65536 % 256, w / 16777216 % 256]

-- Read four little-endian bytes as a word; nothing if fewer than four remain.
def readLe32 : List Nat → Option (Nat × List Nat)
  -- Four bytes or more: combine the first four, keep the rest;
  | a :: b :: c :: d :: rest => some (a + b * 256 + c * 65536 + d * 16777216, rest)
  -- fewer: a cut element.
  | _ => none

-- Promise: any 32-bit word reads back as itself, and the bytes after it are
-- left untouched.
theorem readLe32_le32 (w : Nat) (rest : List Nat) (h : w < 4294967296) :
    readLe32 (le32 w ++ rest) = some (w, rest) := by
  -- Unfold both sides; what is left is arithmetic on the four digits.
  simp only [le32, readLe32, List.cons_append, List.nil_append, Option.some.injEq,
    Prod.mk.injEq, and_true]
  omega

-- What an entry does to its node.
inductive Op where
  -- Set the node's vector.
  | upsert
  -- Remove the node from the results.
  | delete
  deriving DecidableEq

-- The byte each operation is stored as (Rust: `PendingOp as u8`).
def opByte : Op → Nat
  | .upsert => 1
  | .delete => 2

-- The operation a stored byte names, if any.
def opOf : Nat → Option Op
  | 1 => some .upsert
  | 2 => some .delete
  | _ => none

-- Promise: each operation's byte names it back.
theorem opOf_opByte (op : Op) : opOf (opByte op) = some op := by
  -- Two operations, each checked.
  cases op <;> rfl

-- The elements, each as its four bytes, one after another.
def encodeWords : List Nat → List Nat
  -- No elements: no bytes.
  | [] => []
  -- An element, then the rest.
  | w :: ws => le32 w ++ encodeWords ws

-- The stored value: the operation's byte, then the elements.
def encode (op : Op) (ws : List Nat) : List Nat :=
  opByte op :: encodeWords ws

-- The elements take four bytes each.
theorem encodeWords_length (ws : List Nat) : (encodeWords ws).length = 4 * ws.length := by
  -- Induct on the elements; each adds four bytes.
  induction ws with
  | nil => rfl
  | cons w ws ih => simp [encodeWords, le32, ih]; omega

-- Read elements until the bytes run out. `fuel` bounds the elements read, so
-- the reading always ends; a cut element gives nothing.
def decodeWords : Nat → List Nat → Option (List Nat)
  -- No bytes left: every element has been read.
  | _, [] => some []
  -- Bytes left but no fuel: more elements than the bound allows.
  | 0, _ :: _ => none
  -- Bytes left: read one element, then the rest.
  | fuel + 1, b :: bs =>
    match readLe32 (b :: bs) with
    | none => none
    | some (w, rest) => (decodeWords fuel rest).map (w :: ·)

-- Read a stored value, in the order the Rust checks: an empty value, then a
-- cut element, then an unknown operation are refused.
def decode : List Nat → Option (Op × List Nat)
  -- No operation byte: refused (Rust: `MalformedVectorPending`).
  | [] => none
  | b :: rest =>
    -- Bytes that are not whole elements: refused (Rust: `MalformedVectorPending`).
    if rest.length % 4 ≠ 0 then none
    else
      match opOf b with
      -- An unknown operation: refused (Rust: `UnknownVectorPendingOp`).
      | none => none
      -- Otherwise the elements, read with their byte count as fuel.
      | some op => (decodeWords rest.length rest).map (fun ws => (op, ws))

-- Promise: with fuel for every element, the elements read back exactly.
theorem decodeWords_encodeWords (ws : List Nat)
    -- Given every element is a 32-bit word,
    (hfit : ∀ w ∈ ws, w < 4294967296) :
    -- any fuel at least the number of elements reads them all back.
    ∀ fuel, ws.length ≤ fuel → decodeWords fuel (encodeWords ws) = some ws := by
  -- Induct on the elements.
  induction ws with
  -- No elements: no bytes, read as no elements.
  | nil => intro fuel _; cases fuel <;> rfl
  -- An element then the rest: it reads back, then the rest do.
  | cons w ws ih =>
    intro fuel hfuel
    -- The fuel covers this element, so it is one more than some smaller fuel.
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp at hfuel; omega⟩
    -- This element is a word, and so is every later one.
    have hw := hfit w (by simp)
    have hrest : ∀ q ∈ ws, q < 4294967296 := fun q hq => hfit q (by simp [hq])
    -- Its four bytes read back as it, leaving the rest's bytes.
    have hr := readLe32_le32 w (encodeWords ws) hw
    -- The rest reads back with the fuel that is left.
    have hws := ih hrest f (by simp at hfuel; omega)
    -- Unfold one step of the reading and use both facts.
    simp only [encodeWords, le32, List.cons_append, List.nil_append] at hr ⊢
    simp only [decodeWords, hr, hws, Option.map_some']

-- Promise: an entry reads back as exactly its operation and its vector.
theorem decode_encode (op : Op) (ws : List Nat)
    -- Given every element is a 32-bit word,
    (hfit : ∀ w ∈ ws, w < 4294967296) :
    -- the stored value reads back exactly.
    decode (encode op ws) = some (op, ws) := by
  -- The elements are whole, the operation byte names the operation, and the
  -- byte count is enough fuel since each element takes four.
  have hlen := encodeWords_length ws
  have hwords := decodeWords_encodeWords ws hfit (4 * ws.length) (by omega)
  simp only [encode, decode, hlen, Nat.mul_mod_right, ne_eq, not_true_eq_false, if_false,
    opOf_opByte, hwords, Option.map_some']

-- Promise: an empty value is no entry.
theorem decode_empty : decode [] = none := rfl

-- Promise: a value cut inside an element is no entry.
theorem decode_partial_element (b : Nat) (rest : List Nat) (h : rest.length % 4 ≠ 0) :
    decode (b :: rest) = none := by
  -- The whole-element check fails first.
  simp [decode, h]

-- Promise: a value naming an unknown operation is no entry.
theorem decode_unknown_op (b : Nat) (rest : List Nat) (h : opOf b = none) :
    decode (b :: rest) = none := by
  -- Whatever the element check says, the operation check fails.
  unfold decode
  split <;> simp_all

end VectorPending
