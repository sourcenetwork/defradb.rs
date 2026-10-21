/-!
# JsonCrdt.Path: a path's storage key gives the path back, and keys nest like paths

The story, in plain words. Each path in a JSON field, such as `a.b`, is
stored under its own key so that edits to different paths never touch. A
path is a list of segments (`a`, then `b`), each a string of bytes. Its key
is each segment written out in turn, and each segment ends with the two
bytes `0 1`. A `0` byte inside a segment is written as `0 255`, so a segment
can hold any bytes and still never look like an ending.

What must never happen:
* a key that does not read back as its own path (`decode_encode`), so two
  different paths share a key (`encode_injective`);
* one path's key starting with another's unless the other path is above it
  (`encode_prefix`). Wiping "everything under `a`" is a scan of the keys that
  start with `a`'s key, so it must catch `a.b` and never `ab`.
And going the other way, a path above another has a key the other's key
starts with (`prefix_encode`), so the scan misses nothing under it.

Bytes are modeled as numbers; only `0`, `1` and `255` are special.

Rust anchors: the JSON CRDT is not written yet. This file is its contract
(issue #2129), for the path keys in `crates/defra-core/src/json_crdt.rs`.
-/

namespace JsonCrdt

-- A segment's bytes with every 0 written as 0 255.
def escape : List Nat → List Nat
  -- Nothing to write.
  | [] => []
  -- A 0 byte becomes 0 255; any other byte is written as is.
  | c :: s => if c = 0 then 0 :: 255 :: escape s else c :: escape s

-- A segment as written in a key: its escaped bytes, then the ending 0 1.
def encSeg (s : List Nat) : List Nat :=
  -- The bytes, then the two-byte ending.
  escape s ++ [0, 1]

-- A path's key: every segment written in turn.
def encode : List (List Nat) → List Nat
  -- The root has the empty key.
  | [] => []
  -- The first segment, then the rest of the path.
  | s :: p => encSeg s ++ encode p

-- Reading one segment off the front of a key: its bytes and what follows, or
-- nothing if the bytes do not end the way a segment does.
def decodeSeg : List Nat → Option (List Nat × List Nat)
  -- 0 1 ends the segment.
  | 0 :: 1 :: rest => some ([], rest)
  -- 0 255 is an escaped 0 byte.
  | 0 :: 255 :: rest => (decodeSeg rest).map fun (s, r) => (0 :: s, r)
  -- A 0 followed by anything else is not a key.
  | 0 :: _ => none
  -- Any other byte is part of the segment.
  | (c + 1) :: rest => (decodeSeg rest).map fun (s, r) => ((c + 1) :: s, r)
  -- Bytes ran out before the ending.
  | [] => none

/-- Reading a segment always leaves fewer bytes than it started with. -/
theorem decodeSeg_shorter : ∀ {bs s r}, decodeSeg bs = some (s, r) → r.length < bs.length
  -- The ending: two bytes gone.
  | 0 :: 1 :: rest, s, r, h => by simp [decodeSeg] at h; obtain ⟨-, rfl⟩ := h; simp; omega
  -- An escaped 0: what the rest leaves, two bytes further on.
  | 0 :: 255 :: rest, s, r, h => by
      -- The result is the rest's result with a 0 in front.
      simp only [decodeSeg, Option.map_eq_some'] at h
      -- Name what reading the rest gave.
      obtain ⟨⟨s', r'⟩, h', he⟩ := h
      -- The leftover bytes are the rest's leftovers.
      simp at he; obtain ⟨-, rfl⟩ := he
      -- The rest is shorter, by induction; two more bytes were read here.
      have := decodeSeg_shorter h'; simp; omega
  -- A 0 then a byte that is neither 1 nor 255: no segment.
  | 0 :: (b + 2) :: rest, s, r, h => by
      by_cases hb : b = 253
      · -- It was 255 after all: an escaped 0, as above.
        subst hb
        -- Same steps as the escaped 0 above.
        simp only [decodeSeg, Option.map_eq_some'] at h
        -- Name what reading the rest gave.
        obtain ⟨⟨s', r'⟩, h', he⟩ := h
        -- The leftover bytes are the rest's leftovers.
        simp at he; obtain ⟨-, rfl⟩ := he
        -- Shorter by induction, two bytes further on.
        have := decodeSeg_shorter h'; simp; omega
      · -- Otherwise the bytes are not a key.
        have : b + 2 ≠ 255 := by omega
        simp [decodeSeg, this] at h
  -- A lone 0: no segment.
  | [0], s, r, h => by simp [decodeSeg] at h
  -- A plain byte: what the rest leaves, one byte further on.
  | (c + 1) :: rest, s, r, h => by
      -- The result is the rest's result with this byte in front.
      simp only [decodeSeg, Option.map_eq_some'] at h
      -- Name what reading the rest gave.
      obtain ⟨⟨s', r'⟩, h', he⟩ := h
      -- The leftover bytes are the rest's leftovers.
      simp at he; obtain ⟨-, rfl⟩ := he
      -- Shorter by induction, one byte further on.
      have := decodeSeg_shorter h'; simp; omega
  -- No bytes: no segment.
  | [], s, r, h => by simp [decodeSeg] at h

-- Reading a whole key back into its path, one segment at a time. (The linter
-- can't see that the termination proof below uses `h`.)
set_option linter.unusedVariables false in
def decode (bs : List Nat) : Option (List (List Nat)) :=
  -- The empty key is the root.
  if bs = [] then some [] else
  -- Otherwise read a segment, then the rest of the key.
  match h : decodeSeg bs with
  | some (s, r) => (decode r).map (s :: ·)
  | none => none
termination_by bs.length
decreasing_by exact decodeSeg_shorter h

/-- A written segment reads back as itself, leaving what followed it. -/
theorem decodeSeg_encSeg (s rest : List Nat) :
    decodeSeg (encSeg s ++ rest) = some (s, rest) := by
  -- Walk the segment byte by byte.
  induction s with
  -- Only the ending is left: it reads as an empty segment.
  | nil => simp [encSeg, escape, decodeSeg]
  -- One more byte in front of a segment that reads back by induction.
  | cons c s ih =>
    -- A 0 was escaped; any other byte was written as is.
    cases c with
    -- 0 was written 0 255, which reads back as 0.
    | zero => simp_all [encSeg, escape, decodeSeg]
    -- Any other byte was written as itself.
    | succ c => simp_all [encSeg, escape, decodeSeg]

/-- A path's key reads back as the path. -/
theorem decode_encode (p : List (List Nat)) : decode (encode p) = some p := by
  -- Walk the path segment by segment.
  induction p with
  -- The root's key is empty.
  | nil => simp [encode, decode]
  -- One segment in front of a path that reads back by induction.
  | cons s p ih =>
    -- The key starts with a segment, which ends with 0 1, so it isn't empty.
    have hne : encSeg s ++ encode p ≠ [] := by simp [encSeg]
    -- Read the first segment, then the rest by induction.
    rw [encode, decode, if_neg hne]
    split
    · -- The segment read is s, and what follows is the rest's key.
      rename_i s' r h
      rw [decodeSeg_encSeg] at h; simp at h; obtain ⟨rfl, rfl⟩ := h
      -- The rest reads back by induction.
      simp [ih]
    · -- Reading the segment can't fail: it was written by encSeg.
      rename_i h; rw [decodeSeg_encSeg] at h; simp at h

/-- Two paths with the same key are the same path. -/
theorem encode_injective {p q : List (List Nat)} (h : encode p = encode q) : p = q := by
  -- Both keys read back as their paths.
  have hp := decode_encode p
  -- q's key is p's key, so it reads back as both.
  rw [h, decode_encode] at hp
  -- So the paths are equal.
  exact (Option.some.inj hp).symm

/-- Two written segments followed by anything are equal only if the segments
are equal and so are the followers: a segment's end can always be found. -/
theorem encSeg_append_inj {s t x y : List Nat}
    (h : encSeg s ++ x = encSeg t ++ y) : s = t ∧ x = y := by
  -- Read the first segment off both sides.
  have := congrArg decodeSeg h
  -- Each side reads as its own segment and follower.
  rw [decodeSeg_encSeg, decodeSeg_encSeg] at this
  -- They read the same, so segments and followers agree.
  simpa using this

/-- If one path's key starts another's, the first path is above (or equal to)
the second. -/
theorem encode_prefix : ∀ {p q : List (List Nat)}, encode p <+: encode q → p <+: q
  -- The root is above every path.
  | [], q, _ => List.nil_prefix
  -- A non-empty key can't start the empty key.
  | s :: p, [], ⟨t, h⟩ => by simp [encode, encSeg] at h
  -- Both start with a segment: the segments match, and so do the rests.
  | s :: p, u :: q, ⟨t, h⟩ => by
      -- Split both keys into their first segment and the rest.
      simp only [encode, List.append_assoc] at h
      -- The first segments are equal, and the rest of one key starts the other.
      obtain ⟨rfl, h'⟩ := encSeg_append_inj h
      -- By induction the rest of the path is above the rest of the other.
      obtain ⟨t', ht'⟩ := encode_prefix ⟨t, h'⟩
      -- Put the shared first segment back on.
      exact ⟨t', by simp [ht']⟩

/-- A key of a path after another path is the first key followed by the second. -/
theorem encode_append (p q : List (List Nat)) : encode (p ++ q) = encode p ++ encode q := by
  -- One segment at a time.
  induction p with
  -- The root adds nothing.
  | nil => simp [encode]
  -- One segment, then the rest by induction.
  | cons s p ih => simp [encode, ih, List.append_assoc]

/-- A path above another has a key the other's key starts with. -/
theorem prefix_encode {p q : List (List Nat)} (h : p <+: q) : encode p <+: encode q := by
  -- q is p followed by more segments; its key is p's key followed by theirs.
  obtain ⟨t, rfl⟩ := h
  -- The extra segments' key is what follows.
  exact ⟨encode t, (encode_append p t).symm⟩

end JsonCrdt
