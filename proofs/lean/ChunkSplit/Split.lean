/-!
# ChunkSplit.Split: how a text field is cut into chunks

The story, in plain words. A document has a long text field marked `@chunk`.
defradb cuts that text into small pieces called chunks, so each piece can be
searched and embedded on its own. The cutter must be predictable: the same text
always gives the same chunks, every chunk is small enough, the chunks together
are the whole text, and a small edit changes only the chunks near it.

How the cutter works, in four steps:
1. The text is a list of paragraphs. Each paragraph knows its characters, whether
   it is a heading, and its weight (how many characters it has without the blank
   space around it).
2. Paragraphs are grouped into units: a heading followed by text joins that
   text, and every other paragraph is a unit of its own. A unit has a weight (its
   paragraphs' weights added up) and a roll (a number below 2^64 taken from the
   hash of its words). A fence goes before a unit when its roll is small enough
   for its weight, so every character of text, body or heading, has the same
   small chance to start a segment. Fences cut the units into segments. A fence
   decision looks only at its own unit; that is what keeps edits local.
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
text is read as paragraphs with their heading flags and weights, and the hash
of a unit's words is computed (`rollOf`, any function here). The sentence and word cutters are modeled as any functions that
cut a string into pieces which join back to it (`LevelsJoin` in `Covers.lean`); Rust unit tests
check that law for the real cutters.

Rust anchors: `split` in `crates/chunk-split/src/split/mod.rs`; `units`,
`fence_before` and `segments` in `crates/chunk-split/src/split/segments.rs`; `atoms` in
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
  -- The roll of a unit: the first 8 bytes of the hash of its words, as a number.
  rollOf : List Para → Nat

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

-- Walk a list, growing the current group `cur` whose last element is `last`, and
-- starting a new group before every element `brk` puts a break in front of.
def groupFrom {α : Type} (brk : α → α → Bool) (cur : List α) (last : α) : List α → List (List α)
  -- Nothing left: the current group is the last one.
  | [] => [cur]
  -- The next element `q`:
  | q :: qs =>
    -- with a break before it, close the current group and start a new one at `q`;
    if brk last q then cur :: groupFrom brk [q] q qs
    -- without one, `q` joins the current group.
    else groupFrom brk (cur ++ [q]) q qs

-- Cut a list into groups with the break rule `brk`.
def group {α : Type} (brk : α → α → Bool) : List α → List (List α)
  -- Nothing, no groups.
  | [] => []
  -- Otherwise the first element opens the first group.
  | p :: ps => groupFrom brk [p] p ps

-- Does a new unit start before `cur`? Yes, unless `prev` is a heading and `cur` is text.
def unitBreak (prev cur : Para) : Bool :=
  -- Only a heading followed by text stays together.
  !(prev.heading && !cur.heading)

-- Cut the paragraphs into units.
def units (ps : List Para) : List (List Para) :=
  -- Group them with the unit rule.
  group unitBreak ps

-- A unit's weight: its paragraphs' weights added up.
def unitWeight (u : List Para) : Nat :=
  -- Add the weights one by one.
  (u.map Para.weight).sum

-- Should a new segment start before unit `u`?
def fence (cfg : Config) (u : List Para) : Bool :=
  -- When the roll is below `weight` out of `segmentChunks * max` parts of the roll
  -- range. Both sides are multiplied out, so no division rounds.
  decide (cfg.rollOf u * (segmentChunks * cfg.max) < unitWeight u * rollRange)

-- Cut the paragraphs into segments, each a list of units.
def segments (cfg : Config) (ps : List Para) : List (List (List Para)) :=
  -- Group the units, with a fence decided by the unit after it alone.
  group (fun _ u => fence cfg u) (units ps)

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
  -- Every segment's paragraphs are chunked on their own, one segment after another.
  (segments cfg ps).flatMap (fun seg => segChunks cfg seg.flatten)

-- The text the paragraphs make when put back together.
def joined (ps : List Para) : List Char :=
  -- Each paragraph's characters, one after another.
  (ps.map Para.text).flatten

-- Promise: a heading and the text right under it are always in one unit, so no
-- fence can come between them.
theorem heading_keeps_text (prev cur : Para)
    -- The previous paragraph is a heading and the next one is text.
    (hh : prev.heading = true) (hc : cur.heading = false) :
    -- Then no unit starts between them.
    unitBreak prev cur = false := by
  -- The rule keeps exactly this pair together.
  simp [unitBreak, hh, hc]

-- Promise: where units start depends only on which paragraphs are headings.
theorem unit_break_reads_only_headings (prev prev' cur cur' : Para)
    -- Two pairs of paragraphs that agree on being headings or not
    (hp : prev.heading = prev'.heading) (hc : cur.heading = cur'.heading) :
    -- decide the unit break the same way.
    unitBreak prev cur = unitBreak prev' cur' := by
  -- The rule reads only the two heading flags.
  simp [unitBreak, hp, hc]

-- Promise: a unit as heavy as a whole segment always starts one.
theorem long_unit_fences (cfg : Config) (u : List Para)
    -- The limit is at least one character,
    (hmax : 1 ≤ cfg.max)
    -- the roll is a real 8-byte roll,
    (hroll : cfg.rollOf u < rollRange)
    -- and the unit weighs at least a whole segment.
    (hw : segmentChunks * cfg.max ≤ unitWeight u) :
    -- Then a fence goes before it.
    fence cfg u = true := by
  -- A segment's worth of characters is more than zero.
  have hpos : 0 < segmentChunks * cfg.max := by
    simp only [segmentChunks]; omega
  -- The roll times the segment size is below the roll range times the segment size,
  have h1 : cfg.rollOf u * (segmentChunks * cfg.max) < rollRange * (segmentChunks * cfg.max) :=
    Nat.mul_lt_mul_of_pos_right hroll hpos
  -- which is at most the roll range times the weight.
  have h2 : rollRange * (segmentChunks * cfg.max) ≤ unitWeight u * rollRange := by
    rw [Nat.mul_comm (unitWeight u)]
    exact Nat.mul_le_mul_left _ hw
  -- So the roll is small enough.
  simp only [fence, decide_eq_true_eq]
  omega

end ChunkSplit
