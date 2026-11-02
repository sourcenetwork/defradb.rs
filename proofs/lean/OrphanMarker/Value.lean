/-!
# OrphanMarker.Value: a to-merge mark reads back as the time it holds

The story, in plain words. When a node stores a block a peer sent it, it
writes a to-merge mark beside the block. The mark's value is the time the
block was fetched: eight bytes, the unix seconds, most significant first.
Go DefraDB writes a single byte, 0xff, with no time in it, and so did older
defradb.rs builds; Go only checks that a mark is there. The orphan sweep
reads the time to tell a fetch still in flight from an abandoned one.

What must never happen:
* a time comes back different from the one written (`time_value`);
* a mark that is not exactly eight bytes, such as the older 0xff byte, is
  read as having a time (`time_short`, `time_long`, `time_legacy`), since
  the sweep must leave such a block alone.

Bytes are modelled as natural numbers below 256 and times as naturals below
2^64.

Rust anchors: `marker_value`, `marker_time` in
`crates/storage/src/stores/blockstore.rs`.
-/

namespace OrphanMarker

-- The base of one byte.
def B : Nat := 256

-- A time as eight big-endian bytes, as `u64::to_be_bytes` writes it: the
-- byte for 256^7 first, the byte for 256^0 last.
def value (t : Nat) : List Nat :=
  [t / B ^ 7 % B, t / B ^ 6 % B, t / B ^ 5 % B, t / B ^ 4 % B,
   t / B ^ 3 % B, t / B ^ 2 % B, t / B % B, t % B]

-- Read a mark: exactly eight bytes give the time they hold; any other
-- length gives no time.
def time : List Nat → Option Nat
  -- Eight bytes: combine them, most significant first;
  | [a, b, c, d, e, f, g, h] =>
      some (((((((a * B + b) * B + c) * B + d) * B + e) * B + f) * B + g) * B + h)
  -- anything else carries no time.
  | _ => none

-- A time written as a mark reads back unchanged.
theorem time_value (t : Nat) (h : t < B ^ 8) : time (value t) = some t := by
  simp only [value, time, B, Option.some.injEq] at *
  omega

-- A mark shorter than eight bytes has no time.
theorem time_short (bytes : List Nat) (h : bytes.length < 8) : time bytes = none := by
  -- Every list of length below eight is one of the patterns the second
  -- case of `time` catches.
  match bytes, h with
  | [], _ => rfl
  | [_], _ => rfl
  | [_, _], _ => rfl
  | [_, _, _], _ => rfl
  | [_, _, _, _], _ => rfl
  | [_, _, _, _, _], _ => rfl
  | [_, _, _, _, _, _], _ => rfl
  | [_, _, _, _, _, _, _], _ => rfl

-- A mark longer than eight bytes has no time.
theorem time_long (bytes : List Nat) (h : 8 < bytes.length) : time bytes = none := by
  -- A list longer than eight starts with nine elements, which no case of
  -- `time` but the last matches.
  match bytes, h with
  | _ :: _ :: _ :: _ :: _ :: _ :: _ :: _ :: _ :: _, _ => rfl

-- The older single 0xff byte has no time, so the sweep leaves its block.
theorem time_legacy : time [255] = none := rfl

end OrphanMarker
