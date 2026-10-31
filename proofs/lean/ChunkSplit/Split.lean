/-!
# ChunkSplit.Split: how a text field is cut into chunks

The story, in plain words. A document has a long text field marked `@chunk`.
defradb cuts that text into small pieces called chunks, so each piece can be
searched and embedded on its own. The cutter must be predictable: the same text
always gives the same chunks, every chunk is small enough, the chunks together
are the whole text, and a small edit changes only the chunks near it.

How the cutter works, in four steps:
1. The text is a list of paragraphs. Each paragraph knows its characters, whether
   it is a heading, and whether its own hash picked it as a fence.
2. A fence goes before a paragraph that is a heading, that is too long, that
   comes right after a too-long paragraph, or that its hash picked unless a
   heading comes right before it. Fences cut the
   paragraphs into segments. A fence decision looks only at a paragraph and the
   one before it; that is what keeps edits local.
3. Inside a segment, each paragraph becomes atoms: a short paragraph is one atom,
   a too-long one is cut into sentences, then words, then single characters,
   until every piece fits. Atoms are then packed greedily into chunks that fit.
4. Optionally, a chunk starts with a few sentences copied from the end of the
   chunk before it in the same segment (the overlap), but only if they still fit.

This file holds the definitions. The promises are in `Covers.lean`,
`Bounded.lean` and `Local.lean`.

What this model trusts. Two things happen in Rust before this model starts: the
text is read as paragraphs, and each paragraph's heading flag and hash fence are
computed. The sentence and word cutters are modeled as any functions that cut a
string into pieces which join back to it (`LevelsJoin` in `Covers.lean`); Rust unit tests
check that law for the real cutters.

Rust anchors: `split` in `crates/chunk-split/src/split/mod.rs`; `fence_before` in
`crates/chunk-split/src/split/segments.rs`; `atoms` in
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
  -- Whether its own hash picked it to start a new segment.
  pick : Bool

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
  -- Yes when `cur` is a heading,
  cur.heading
  -- or `cur` is too long to fit in one chunk,
  || decide (cfg.max < cur.text.length)
  -- or the paragraph before it was too long,
  || decide (cfg.max < prev.text.length)
  -- or `cur`'s hash picked it, unless `prev` is a heading, so a heading keeps its text.
  || (cur.pick && !prev.heading)

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

-- Pack atoms greedily into chunk cores. `cur` is the core being filled.
def packFrom (max : Nat) (cur : List Char) : List (List Char) → List (List Char)
  -- No atoms left: emit the core being filled, if it holds anything.
  | [] => if cur.isEmpty then [] else [cur]
  -- The next atom `a`:
  | a :: as =>
    -- an empty core just takes it;
    if cur.isEmpty then packFrom max a as
    -- if it fits next to what is there, it joins;
    else if cur.length + a.length ≤ max then packFrom max (cur ++ a) as
    -- otherwise the core is emitted and `a` starts the next one.
    else cur :: packFrom max a as

-- Pack a list of atoms, starting from an empty core.
def pack (max : Nat) (as : List (List Char)) : List (List Char) :=
  -- Start with nothing in the core.
  packFrom max [] as

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
  overlapFrom cfg none (pack cfg.max (seg.flatMap (atoms cfg)))

-- The whole cutter: segments, then each segment's chunks, in order.
def split (cfg : Config) (ps : List Para) : List Chunk :=
  -- Every segment is chunked on its own, and the results are put one after another.
  (segments cfg ps).flatMap (segChunks cfg)

-- The text the paragraphs make when put back together.
def joined (ps : List Para) : List Char :=
  -- Each paragraph's characters, one after another.
  (ps.map Para.text).flatten

-- Promise: the hash never cuts a heading off from the short paragraph under it.
theorem heading_keeps_text (cfg : Config) (prev cur : Para)
    -- The previous paragraph is a heading that fits,
    (hh : prev.heading = true) (hp : prev.text.length ≤ cfg.max)
    -- and the next one is a plain paragraph that fits.
    (hc : cur.heading = false) (hl : cur.text.length ≤ cfg.max) :
    -- Then no fence goes between them.
    fence cfg prev cur = false := by
  -- Every part of the rule is false here, the hash part because of the heading.
  simp [fence, hh, hc, Nat.not_lt.mpr hp, Nat.not_lt.mpr hl]

end ChunkSplit
