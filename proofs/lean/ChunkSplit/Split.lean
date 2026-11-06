/-!
# ChunkSplit.Split: how a text field is cut into chunks

The story, in plain words. A document has a long text field marked `@chunk`.
defradb cuts that text into small pieces called chunks, so each piece can be
searched and embedded on its own. The cutter must be predictable: the same text
always gives the same chunks, every chunk is small enough, the chunks together
are the whole text, and a small edit changes only the chunks near it.

How the cutter works, in four steps:
1. The text is a list of paragraphs. Each paragraph knows its characters, whether
   it is a heading, its weight (how many characters it has without the blank
   space around it) and its roll (a number below 2^64 taken from its hash).
2. A fence goes before a paragraph when its roll is small enough for its weight:
   every character has the same small chance, so a segment holds about four
   chunks of text whatever the paragraph sizes. No fence ever goes right after a
   heading. Fences cut the paragraphs into segments. A fence decision looks only
   at a paragraph and whether the one before it is a heading; that is what keeps
   edits local.
3. Inside a segment, each paragraph becomes atoms: a short paragraph is one atom,
   a too-long one is cut into sentences, then words, then single characters,
   until every piece fits. Atoms are then packed greedily into chunks that fit. A
   heading starts a new chunk once the current one is half full, or when the
   heading and the text under it would not fit next to it.
4. Optionally, a chunk starts with a few sentences copied from the end of the
   chunk before it in the same segment (the overlap), but only if they still fit.

This file holds the definitions. The promises are in `Covers.lean`,
`Bounded.lean` and `Local.lean`.

What this model trusts. Two things happen in Rust before this model starts: the
text is read as paragraphs, and each paragraph's heading flag, weight and roll
are computed. The sentence and word cutters are modeled as any functions that
cut a string into pieces which join back to it (`LevelsJoin` in `Covers.lean`); Rust unit tests
check that law for the real cutters.

Rust anchors: `split` in `crates/chunk-split/src/split/mod.rs`; `fence_before` in
`crates/chunk-split/src/split/segments.rs`; `roll` in
`crates/chunk-split/src/split/paragraphs.rs`; `atoms` in
`crates/chunk-split/src/split/atoms.rs`; `pack` in
`crates/chunk-split/src/split/pack.rs`; `with_overlap` in
`crates/chunk-split/src/split/overlap.rs`.
-/

namespace ChunkSplit

-- One paragraph of the text, as Rust reads it.
structure Para where
  -- Its characters, including the blank lines that follow it.
  text : List Char
  -- Whether it is a heading line (`#` or an underlined title).
  heading : Bool
  -- How many characters it has without the blank space around it.
  weight : Nat
  -- The first 8 bytes of its hash read as a number, so it is below `rollRange`.
  roll : Nat

-- How long a segment is on average, counted in chunks of `max` characters.
def segmentChunks : Nat := 4

-- Every roll is below this number, since a roll is 8 bytes.
def rollRange : Nat := 2 ^ 64

-- The cutter's settings.
structure Config where
  -- The largest number of characters a chunk may have (`maxChars`).
  max : Nat
  -- The cutters for a too-long paragraph, finest last: sentences, then words.
  levels : List (List Char → List (List Char))
  -- For the overlap: given the previous chunk, the prefixes it may lend, longest first.
  cands : List Char → List (List Char)

-- One chunk: an optional copied prefix (the overlap) and its own part of the text.
structure Chunk where
  -- The sentences copied from the end of the previous chunk; empty without overlap.
  pre : List Char
  -- The chunk's own characters; the cores of all chunks, in order, are the text.
  core : List Char

-- What a chunk stores: the copied prefix followed by its own part.
def Chunk.text (c : Chunk) : List Char :=
  -- The prefix comes first, then the core.
  c.pre ++ c.core

-- Should a new segment start before paragraph `cur`, whose predecessor is `prev`?
def fence (cfg : Config) (prev cur : Para) : Bool :=
  -- Never right after a heading, so a heading keeps its text;
  !prev.heading
  -- otherwise when the roll is below `weight` out of `segmentChunks * max`
  -- parts of the roll range. Both sides are multiplied out, so no division rounds.
  && decide (cur.roll * (segmentChunks * cfg.max) < cur.weight * rollRange)

-- Walk the paragraphs, growing the current segment `cur` whose last paragraph is
-- `last`, and closing it whenever a fence comes.
def segsFrom (cfg : Config) (cur : List Para) (last : Para) : List Para → List (List Para)
  -- No paragraphs left: the current segment is the last one.
  | [] => [cur]
  -- The next paragraph `q`:
  | q :: qs =>
    -- with a fence before it, close the current segment and start a new one at `q`;
    if fence cfg last q then cur :: segsFrom cfg [q] q qs
    -- without one, `q` joins the current segment.
    else segsFrom cfg (cur ++ [q]) q qs

-- Cut the paragraphs into segments.
def segments (cfg : Config) : List Para → List (List Para)
  -- No paragraphs, no segments.
  | [] => []
  -- Otherwise the first paragraph opens the first segment.
  | p :: ps => segsFrom cfg [p] p ps

-- Cut a too-long string with the given cutters, keeping only pieces that fit.
def refine (max : Nat) : List (List Char → List (List Char)) → List Char → List (List Char)
  -- No cutters left: every character is its own piece.
  | [], s => s.map (fun c => [c])
  -- Cut with the coarsest cutter, drop empty pieces, and cut again the ones that
  -- still don't fit with the finer cutters.
  | l :: ls, s =>
    ((l s).filter (fun piece => !piece.isEmpty)).flatMap
      (fun piece => if piece.length ≤ max then [piece] else refine max ls piece)

-- The atoms of one paragraph: pieces that each fit in a chunk.
def atoms (cfg : Config) (p : Para) : List (List Char) :=
  -- A paragraph that fits is one atom (none if it is empty);
  if p.text.length ≤ cfg.max then (if p.text.isEmpty then [] else [p.text])
  -- a too-long one is cut down with the cutters.
  else refine cfg.max cfg.levels p.text

-- One atom of a segment, and whether it is the first atom of a heading.
structure Atom where
  -- The atom's characters.
  text : List Char
  -- True only for the first atom of a heading paragraph.
  heading : Bool

-- Mark the atoms of one paragraph: the first carries the heading flag, the rest don't.
def tag (h : Bool) : List (List Char) → List Atom
  -- No atoms, nothing to mark.
  | [] => []
  -- The first atom gets the flag, the others are plain.
  | a :: as => ⟨a, h⟩ :: as.map (fun t => ⟨t, false⟩)

-- The atoms of a whole segment, marked, in order.
def segAtoms (cfg : Config) (seg : List Para) : List Atom :=
  -- Each paragraph's atoms, marked with its heading flag.
  seg.flatMap (fun p => tag p.heading (atoms cfg p))

-- The length of the heading run at the front, plus the first text atom after it.
def unitLen : List Atom → Nat
  -- Nothing left: nothing to count.
  | [] => 0
  -- A heading atom counts and the run goes on; a text atom counts and ends it.
  | a :: as => if a.heading then a.text.length + unitLen as else a.text.length

-- Should the core close before the atoms `rest`, though the next one would fit?
-- Only before a heading: when the core is half full, or the heading and its text
-- would not fit next to it.
def breaksBefore (max curLen : Nat) : List Atom → Bool
  -- Nothing comes next: no reason to close.
  | [] => false
  -- The next atom `a` is a heading and one of the two conditions holds.
  | a :: as => a.heading && (decide (max ≤ 2 * curLen) || decide (max < curLen + unitLen (a :: as)))

-- Pack atoms greedily into chunk cores. `cur` is the core being filled, and
-- `afterHeading` says whether its last atom opened a heading.
def packFrom (max : Nat) (cur : List Char) (afterHeading : Bool) : List Atom → List (List Char)
  -- No atoms left: emit the core being filled, if it holds anything.
  | [] => if cur.isEmpty then [] else [cur]
  -- The next atom `a`:
  | a :: as =>
    -- an empty core just takes it;
    if cur.isEmpty then packFrom max a.text a.heading as
    -- if it fits next to what is there,
    else if cur.length + a.text.length ≤ max then
      -- the core still closes before a heading that asks for it, unless the core
      -- just opened a heading itself;
      if !afterHeading && breaksBefore max cur.length (a :: as) then
        cur :: packFrom max a.text a.heading as
      -- otherwise `a` joins it;
      else packFrom max (cur ++ a.text) a.heading as
    -- when it does not fit, the core is emitted and `a` starts the next one.
    else cur :: packFrom max a.text a.heading as

-- Pack a list of atoms, starting from an empty core.
def pack (max : Nat) (as : List Atom) : List (List Char) :=
  -- Start with nothing in the core, and no heading before it.
  packFrom max [] false as

-- The overlap prefix for `core`: the longest offered prefix of `prev` that still fits.
def pickPre (cfg : Config) (prev core : List Char) : List Char :=
  -- Take the first candidate that keeps the chunk within `max`, or nothing.
  ((cfg.cands prev).find? (fun c => decide (c.length + core.length ≤ cfg.max))).getD []

-- Give each core of one segment its overlap prefix; `prev` is the core before it.
def overlapFrom (cfg : Config) (prev : Option (List Char)) : List (List Char) → List Chunk
  -- No cores left.
  | [] => []
  -- The next core:
  | core :: rest =>
    -- the first core of a segment borrows nothing; later ones borrow from `prev`;
    let pre := match prev with | none => [] | some p => pickPre cfg p core
    -- then the rest follow, each borrowing from the core just before it.
    ⟨pre, core⟩ :: overlapFrom cfg (some core) rest

-- The chunks of one segment: its atoms, packed, with overlap inside the segment only.
def segChunks (cfg : Config) (seg : List Para) : List Chunk :=
  -- Atoms of every paragraph, packed into cores, then given their prefixes.
  overlapFrom cfg none (pack cfg.max (segAtoms cfg seg))

-- The whole cutter: segments, then each segment's chunks, in order.
def split (cfg : Config) (ps : List Para) : List Chunk :=
  -- Every segment is chunked on its own, and the results are put one after another.
  (segments cfg ps).flatMap (segChunks cfg)

-- The text the paragraphs make when put back together.
def joined (ps : List Para) : List Char :=
  -- Each paragraph's characters, one after another.
  (ps.map Para.text).flatten

-- Promise: the hash never cuts a heading off from the paragraph under it.
theorem heading_keeps_text (cfg : Config) (prev cur : Para)
    -- The previous paragraph is a heading.
    (hh : prev.heading = true) :
    -- Then no fence goes between them.
    fence cfg prev cur = false := by
  -- The rule starts with "not after a heading", which is false here.
  simp [fence, hh]

-- Promise: the fence after a paragraph reads only its heading flag, not its text.
theorem fence_after_reads_only_the_heading (cfg : Config) (prev prev' cur : Para)
    -- Two paragraphs that agree on being a heading or not,
    (h : prev.heading = prev'.heading) :
    -- decide the fence before `cur` the same way.
    fence cfg prev cur = fence cfg prev' cur := by
  -- The rule reads `prev` only through its heading flag.
  simp [fence, h]

-- Promise: a paragraph as long as a whole segment always starts one, unless a
-- heading comes right before it.
theorem long_paragraph_fences (cfg : Config) (prev cur : Para)
    -- The limit is at least one character,
    (hmax : 1 ≤ cfg.max)
    -- the roll is a real 8-byte roll,
    (hroll : cur.roll < rollRange)
    -- the paragraph weighs at least a whole segment,
    (hw : segmentChunks * cfg.max ≤ cur.weight)
    -- and the one before it is not a heading.
    (hh : prev.heading = false) :
    -- Then a fence goes before it.
    fence cfg prev cur = true := by
  -- A segment's worth of characters is more than zero.
  have hpos : 0 < segmentChunks * cfg.max := by
    simp only [segmentChunks]; omega
  -- The roll times that is below the roll range times that,
  have h1 : cur.roll * (segmentChunks * cfg.max) < rollRange * (segmentChunks * cfg.max) :=
    Nat.mul_lt_mul_of_pos_right hroll hpos
  -- which is at most the roll range times the weight.
  have h2 : rollRange * (segmentChunks * cfg.max) ≤ cur.weight * rollRange := by
    rw [Nat.mul_comm cur.weight]
    exact Nat.mul_le_mul_left _ hw
  -- So the roll is small enough, and nothing else stops the fence.
  simp only [fence, hh, Bool.not_false, Bool.true_and, decide_eq_true_eq]
  omega

end ChunkSplit
