import ChunkSplit.Split

/-!
# ChunkSplit.Bounded: every chunk has between 1 and `maxChars` characters

The story, in plain words. A chunk is what gets embedded and searched, so it
must never be empty and never be longer than the limit the schema set
(`maxChars`). That has to hold for the copied overlap too: a chunk with its
borrowed sentences still fits.

How it holds: every atom fits (a too-long paragraph is cut down until each
piece fits, all the way to single characters), packing only adds an atom to a
core when the sum still fits, and the overlap only borrows a prefix that keeps
the chunk within the limit.

The promise is `bounded`. It needs the limit to be at least 1, since a single
character must fit; the schema validator requires `maxChars >= 64`.
-/

namespace ChunkSplit

-- A piece fits when it holds at least one character and at most `max`.
def Fits (max : Nat) (a : List Char) : Prop :=
  -- Not empty, and not longer than the limit.
  a ≠ [] ∧ a.length ≤ max

-- Promise: every piece `refine` makes fits.
theorem refine_fits (max : Nat) (hmax : 1 ≤ max) :
    -- For any cutters and any string,
    ∀ (levels : List (List Char → List (List Char))) (s : List Char),
      -- each piece made from it fits.
      ∀ a ∈ refine max levels s, Fits max a := by
  -- Take the cutters.
  intro levels
  -- Go through them from coarsest to finest.
  induction levels with
  -- No cutters left: every piece is one character.
  | nil =>
    -- Take the string and a piece of it.
    intro s a ha
    -- The piece is `[c]` for some character `c` of the string,
    simp only [refine, List.mem_map] at ha
    obtain ⟨c, _, rfl⟩ := ha
    -- which is not empty and has length 1, within the limit.
    exact ⟨by simp, by simpa using hmax⟩
  -- A coarsest cutter `l` and finer cutters `ls`:
  | cons l ls ih =>
    -- Take the string and a piece of the result.
    intro s a ha
    -- The piece came from some non-empty piece that `l` cut.
    simp only [refine, List.mem_flatMap, List.mem_filter] at ha
    obtain ⟨piece, ⟨_, hne⟩, ha⟩ := ha
    -- Split on whether that piece fit.
    split at ha
    -- It fit: the result is that piece, which is not empty and fits.
    · rename_i hle
      simp only [List.mem_singleton] at ha
      subst ha
      refine ⟨?_, hle⟩
      intro h
      simp [h] at hne
    -- It didn't: the finer cutters made the result, and their pieces fit.
    · exact ih piece a ha

-- Promise: every atom of a paragraph fits.
theorem atoms_fit (cfg : Config) (hmax : 1 ≤ cfg.max) (p : Para) :
    -- Each atom of `p` has at least one and at most `max` characters.
    ∀ a ∈ atoms cfg p, Fits cfg.max a := by
  -- Take an atom.
  intro a ha
  -- Open the definition of atoms.
  unfold atoms at ha
  -- Split on whether the paragraph fits.
  split at ha
  -- It fits:
  · rename_i hle
    split at ha
    -- an empty paragraph has no atoms, so there is nothing to check;
    · simp at ha
    -- otherwise the atom is the paragraph, not empty and within the limit.
    · rename_i hne
      simp only [List.mem_singleton] at ha
      subst ha
      refine ⟨?_, hle⟩
      intro h
      simp [h] at hne
  -- It is too long: the cutters made the atom, and their pieces fit.
  · exact refine_fits cfg.max hmax cfg.levels p.text a ha

-- Promise: packing atoms that fit makes cores that fit.
theorem packFrom_fits (max : Nat) :
    -- For any atoms and any core being filled,
    ∀ (as : List (List Char)) (cur : List Char),
      -- if the core is within the limit and every atom fits,
      cur.length ≤ max → (∀ a ∈ as, Fits max a) →
      -- every emitted core fits.
      ∀ c ∈ packFrom max cur as, Fits max c := by
  -- Take the atoms.
  intro as
  -- Go through them one at a time.
  induction as with
  -- No atoms left:
  | nil =>
    -- take the core, its bound, and an emitted core;
    intro cur hcur _ c hc
    -- open the definition,
    simp only [packFrom] at hc
    split at hc
    -- an empty core emits nothing, so there is nothing to check;
    · simp at hc
    -- a non-empty core is emitted as it is, and it is within the limit.
    · rename_i hne
      simp only [List.mem_singleton] at hc
      subst hc
      refine ⟨?_, hcur⟩
      intro h
      simp [h] at hne
  -- One more atom `a`:
  | cons a as ih =>
    -- take the core, its bound, the atoms' bounds, and an emitted core;
    intro cur hcur has c hc
    -- `a` fits,
    have ha : Fits max a := has a (by simp)
    -- and so do the atoms after it.
    have hrest : ∀ b ∈ as, Fits max b := fun b m => has b (by simp [m])
    -- Open the definition and follow what happens to `a`.
    simp only [packFrom] at hc
    split at hc
    -- The core was empty: `a` becomes the core, and it is within the limit.
    · exact ih a ha.2 hrest c hc
    -- The core was not empty:
    · rename_i hne
      split at hc
      -- `a` joins it because the sum fits, so the bigger core is within the limit;
      · rename_i hle
        exact ih (cur ++ a) (by simpa using hle) hrest c hc
      -- or the core is emitted and `a` starts the next one:
      · simp only [List.mem_cons] at hc
        rcases hc with rfl | hc
        -- the emitted core is not empty and is within the limit;
        · refine ⟨?_, hcur⟩
          intro h
          simp [h] at hne
        -- the later cores come from `a` onwards, and they fit.
        · exact ih a ha.2 hrest c hc

-- Promise: the overlap prefix never pushes a chunk over the limit.
theorem pickPre_fits (cfg : Config) (prev core : List Char) (hcore : core.length ≤ cfg.max) :
    -- The prefix plus the core is within the limit.
    (pickPre cfg prev core).length + core.length ≤ cfg.max := by
  -- Open the definition of the prefix.
  unfold pickPre
  -- Look at whether some candidate fit.
  cases hf : (cfg.cands prev).find? (fun c => decide (c.length + core.length ≤ cfg.max)) with
  -- None did: the prefix is empty, and the core alone is within the limit.
  | none => simpa using hcore
  -- One did: it was picked exactly because it fits next to the core.
  | some c =>
    have := List.find?_some hf
    simpa using this

-- Promise: every chunk made from cores that fit, overlap included, fits.
theorem overlapFrom_bounded (cfg : Config) :
    -- For any cores and any previous core,
    ∀ (cores : List (List Char)) (prev : Option (List Char)),
      -- if every core fits,
      (∀ c ∈ cores, Fits cfg.max c) →
      -- every chunk's stored text has between 1 and `max` characters.
      ∀ ch ∈ overlapFrom cfg prev cores, 1 ≤ ch.text.length ∧ ch.text.length ≤ cfg.max := by
  -- Take the cores.
  intro cores
  -- Go through them one at a time.
  induction cores with
  -- No cores: no chunks, nothing to check.
  | nil => intro prev _ ch hch; simp [overlapFrom] at hch
  -- One more core:
  | cons core rest ih =>
    -- take the previous core, the bounds, and a chunk;
    intro prev hall ch hch
    -- this core fits,
    have hc : Fits cfg.max core := hall core (by simp)
    -- and so do the ones after it.
    have hrest : ∀ c ∈ rest, Fits cfg.max c := fun c m => hall c (by simp [m])
    -- The chunk is either this core's chunk or a later one.
    simp only [overlapFrom, List.mem_cons] at hch
    rcases hch with rfl | hch
    -- This core's chunk:
    · -- the core has at least one character;
      have hpos : 1 ≤ core.length := by
        cases core with
        | nil => exact absurd rfl hc.1
        | cons _ _ => simp
      -- look at whether it borrows a prefix.
      cases prev with
      -- First in its segment: no prefix, so the text is the core.
      | none =>
        simp only [Chunk.text, List.nil_append]
        exact ⟨hpos, hc.2⟩
      -- Later: the borrowed prefix keeps it within the limit.
      | some p =>
        simp only [Chunk.text, List.length_append]
        exact ⟨by omega, pickPre_fits cfg p core hc.2⟩
    -- A later chunk: it fits by the same argument.
    · exact ih (some core) hrest ch hch

-- Promise (bounded): every chunk the cutter makes has between 1 and `maxChars` characters.
theorem bounded (cfg : Config) (hmax : 1 ≤ cfg.max) (ps : List Para) :
    -- Each chunk's stored text, overlap included, is not empty and within the limit.
    ∀ ch ∈ split cfg ps, 1 ≤ ch.text.length ∧ ch.text.length ≤ cfg.max := by
  -- Take a chunk.
  intro ch hch
  -- It belongs to some segment's chunks.
  simp only [split, List.mem_flatMap] at hch
  obtain ⟨seg, _, hch⟩ := hch
  -- Those are the segment's packed atoms with overlap.
  simp only [segChunks] at hch
  -- With overlap they fit if every packed core fits,
  refine overlapFrom_bounded cfg _ none ?_ ch hch
  -- and packed cores fit if packing starts empty and every atom fits.
  apply packFrom_fits cfg.max _ [] (by simp)
  -- Take an atom.
  intro a ha
  -- It is an atom of some paragraph of the segment,
  simp only [List.mem_flatMap] at ha
  obtain ⟨p, _, ha⟩ := ha
  -- and every paragraph's atoms fit.
  exact atoms_fit cfg hmax p a ha

end ChunkSplit
