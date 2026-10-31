import ChunkSplit.Split

/-!
# ChunkSplit.Covers: the chunks, put back together, are exactly the text

The story, in plain words. After the cutter runs, take every chunk's own part
(its core, leaving out any copied overlap) and lay them end to end. You must get
back the text you started with: no character lost, none repeated, none moved.

The promise is `covers`. Everything above it is a small step it is built from:
each stage of the cutter (segments, atoms, packing, overlap) keeps the
characters exactly as they were.

What it trusts: the sentence and word cutters only cut; joining their pieces
gives back their input (`LevelsJoin`). Rust unit tests check that for the real
cutters in `crates/chunk-split/src/split/atoms.rs`.
-/

namespace ChunkSplit

-- The law every cutter must keep: joining the pieces it makes gives back its input.
def LevelsJoin (cfg : Config) : Prop :=
  -- For every cutter in the settings and every string, the pieces join to the string.
  ∀ l ∈ cfg.levels, ∀ s, (l s).flatten = s

-- Promise: joining a flat-mapped list is joining each element's own result.
theorem flatten_flatMap {α β : Type} (g : α → List (List β)) :
    -- For any list, joining what `flatMap g` makes equals joining each `g x` joined.
    ∀ xs : List α, (xs.flatMap g).flatten = (xs.map (fun x => (g x).flatten)).flatten := by
  -- Take the list.
  intro xs
  -- Go through it one element at a time.
  induction xs with
  -- The empty list joins to nothing on both sides.
  | nil => simp
  -- One more element: both sides add that element's joined result in front.
  | cons x xs ih => simp [List.flatMap_cons, List.flatten_append, ih]

-- Promise: dropping empty pieces does not change what they join to.
theorem flatten_filter_nonempty {α : Type} :
    -- For any list of pieces, joining the non-empty ones equals joining all of them.
    ∀ xs : List (List α), (xs.filter (fun p => !p.isEmpty)).flatten = xs.flatten := by
  -- Take the list of pieces.
  intro xs
  -- Go through it one piece at a time.
  induction xs with
  -- No pieces: both join to nothing.
  | nil => simp
  -- One more piece:
  | cons x xs ih =>
    -- look at whether it is empty;
    cases x with
    -- empty: dropping it changes nothing, since it adds no characters;
    | nil => simp [ih]
    -- not empty: it is kept on both sides.
    | cons c cs => simp [ih]

-- Promise: cutting a too-long string down to pieces keeps every character in order.
theorem refine_flatten (max : Nat) :
    -- For any list of cutters
    ∀ (levels : List (List Char → List (List Char))),
      -- that each keep the characters,
      (∀ l ∈ levels, ∀ s, (l s).flatten = s) →
      -- the pieces `refine` makes join back to the string.
      ∀ s, (refine max levels s).flatten = s := by
  -- Take the cutters.
  intro levels
  -- Go through the cutters from coarsest to finest.
  induction levels with
  -- No cutters left: every character becomes its own piece.
  | nil =>
    -- Take the law (unused here) and the string.
    intro _ s
    -- Go through the string one character at a time.
    induction s with
    -- An empty string makes no pieces.
    | nil => simp [refine]
    -- One more character: its one-character piece goes in front.
    | cons c cs ih => simp_all [refine]
  -- A coarsest cutter `l`, then finer cutters `ls`:
  | cons l ls ih =>
    -- Take the law and the string.
    intro h s
    -- `l` keeps the characters of `s`;
    have hl : (l s).flatten = s := h l (by simp) s
    -- the finer cutters keep the law too.
    have hls : ∀ l' ∈ ls, ∀ s, (l' s).flatten = s := fun l' m => h l' (by simp [m])
    -- Each piece `l` made comes back unchanged, whether it fit or was cut again.
    have piece_ok : ∀ piece : List Char,
        (if piece.length ≤ max then [piece] else refine max ls piece).flatten = piece := by
      -- Take a piece.
      intro piece
      -- Split on whether it fits.
      split
      -- It fits: it is kept as one piece.
      · simp
      -- It doesn't: the finer cutters keep its characters (the step before).
      · exact ih hls piece
    -- Open the definition of `refine` for this case.
    simp only [refine]
    -- Joining the flat-map is joining each piece's result,
    rw [flatten_flatMap]
    -- and each piece's result joins back to the piece.
    simp only [piece_ok, List.map_id']
    -- Dropping empty pieces changes nothing, and `l`'s pieces join to `s`.
    rw [flatten_filter_nonempty, hl]

-- Promise: a paragraph's atoms join back to the paragraph.
theorem atoms_flatten (cfg : Config) (h : LevelsJoin cfg) (p : Para) :
    -- The atoms of `p`, joined, are `p`'s characters.
    (atoms cfg p).flatten = p.text := by
  -- Open the definition of atoms.
  unfold atoms
  -- Split on whether the paragraph fits.
  split
  -- It fits:
  · split
    -- it is empty, so it makes no atoms and has no characters;
    · rename_i _ he
      simp [List.isEmpty_iff.mp he]
    -- it is one atom, itself.
    · simp
  -- It is too long: the cutters keep its characters.
  · exact refine_flatten cfg.max cfg.levels h p.text

-- Promise: packing keeps the characters: the cores join to what was being filled plus the atoms.
theorem packFrom_flatten (max : Nat) :
    -- For any atoms and any core being filled,
    ∀ (as : List (List Char)) (cur : List Char),
      -- the packed cores join to the core followed by all the atoms.
      (packFrom max cur as).flatten = cur ++ as.flatten := by
  -- Take the atoms.
  intro as
  -- Go through them one at a time.
  induction as with
  -- No atoms left:
  | nil =>
    -- take the core,
    intro cur
    -- open the definition,
    simp only [packFrom]
    -- and split on whether the core is empty.
    split
    -- Empty: nothing is emitted, and the core had no characters.
    · rename_i he
      simp [List.isEmpty_iff.mp he]
    -- Not empty: the core itself is emitted.
    · simp
  -- One more atom `a`:
  | cons a as ih =>
    -- take the core,
    intro cur
    -- open the definition,
    simp only [packFrom]
    -- and split on what happens to `a`.
    split
    -- The core was empty: `a` becomes the core.
    · rename_i he
      simp [ih, List.isEmpty_iff.mp he]
    -- The core was not empty:
    · split
      -- `a` fits next to it and joins it;
      · simp [ih]
      -- or the core is emitted and `a` starts the next one.
      · simp [ih]

-- Promise: adding overlap prefixes leaves every core as it was, in order.
theorem overlapFrom_cores (cfg : Config) :
    -- For any cores and any previous core,
    ∀ (cores : List (List Char)) (prev : Option (List Char)),
      -- the chunks' cores are the cores we started with.
      (overlapFrom cfg prev cores).map Chunk.core = cores := by
  -- Take the cores.
  intro cores
  -- Go through them one at a time.
  induction cores with
  -- No cores: no chunks.
  | nil => intro prev; simp [overlapFrom]
  -- One more core: its chunk keeps it as the core, and the rest follow.
  | cons c cs ih => intro prev; simp [overlapFrom, ih]

-- Promise: cutting into segments keeps every paragraph, in order.
theorem segsFrom_flatten (cfg : Config) :
    -- For any remaining paragraphs, current segment, and last paragraph,
    ∀ (ps : List Para) (cur : List Para) (last : Para),
      -- the segments join to the current segment followed by the remaining paragraphs.
      (segsFrom cfg cur last ps).flatten = cur ++ ps := by
  -- Take the paragraphs.
  intro ps
  -- Go through them one at a time.
  induction ps with
  -- None left: the current segment is the only one.
  | nil => intro cur last; simp [segsFrom]
  -- One more paragraph `q`:
  | cons q qs ih =>
    -- take the current segment and last paragraph,
    intro cur last
    -- open the definition,
    simp only [segsFrom]
    -- and split on whether a fence comes before `q`.
    split
    -- A fence: the current segment closes and `q` opens the next.
    · simp [ih]
    -- No fence: `q` joins the current segment.
    · simp [ih]

-- Promise: the segments of a text, joined, are its paragraphs.
theorem segments_flatten (cfg : Config) (ps : List Para) :
    -- Joining the segments gives the paragraphs back.
    (segments cfg ps).flatten = ps := by
  -- Look at whether there are any paragraphs.
  cases ps with
  -- None: no segments.
  | nil => simp [segments]
  -- Some: the walk starts at the first paragraph and keeps all of them.
  | cons p ps => simp [segments, segsFrom_flatten]

-- Promise: one segment's chunk cores join to that segment's text.
theorem segChunks_cores (cfg : Config) (h : LevelsJoin cfg) (seg : List Para) :
    -- The cores of the segment's chunks, joined, are the segment's characters.
    ((segChunks cfg seg).map Chunk.core).flatten = joined seg := by
  -- Overlap keeps the cores, and packing keeps the atoms' characters.
  simp only [segChunks, overlapFrom_cores, pack, packFrom_flatten, List.nil_append]
  -- Joining the paragraphs' atoms is joining each paragraph's atoms,
  rw [flatten_flatMap]
  -- and each paragraph's atoms join to the paragraph.
  simp only [atoms_flatten cfg h]
  -- What is left is the definition of `joined`.
  rfl

-- Promise (covers): the chunks' own parts, laid end to end, are exactly the whole text.
theorem covers (cfg : Config) (h : LevelsJoin cfg) (ps : List Para) :
    -- The cores of all chunks, joined, equal the paragraphs joined.
    ((split cfg ps).map Chunk.core).flatten = joined ps := by
  -- For any list of segments, the chunks' cores join to the segments' text.
  have step : ∀ segs : List (List Para),
      ((segs.flatMap (segChunks cfg)).map Chunk.core).flatten = joined segs.flatten := by
    -- Take the segments.
    intro segs
    -- Go through them one at a time.
    induction segs with
    -- No segments: no chunks and no text.
    | nil => simp [joined]
    -- One more segment: its chunks give its text, followed by the rest.
    | cons s ss ih =>
      simp only [List.flatMap_cons, List.map_append, List.flatten_append, ih,
        segChunks_cores cfg h, joined, List.flatten_cons, List.map_append]
  -- `split` is the chunks of every segment, in order;
  simp only [split]
  -- so its cores give the segments' text, and the segments are the paragraphs.
  rw [step, segments_flatten]

end ChunkSplit
