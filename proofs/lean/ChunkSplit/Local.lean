import ChunkSplit.Split

/-!
# ChunkSplit.Local: the same text gives the same chunks, and an edit stays local

The story, in plain words. Two nodes that cut the same text must get the same
chunks, so they make the same chunk documents. And when someone fixes a typo
in one paragraph of a long article, only the chunks around that paragraph may
change; every other chunk must stay exactly the same, so it is not written,
signed, replicated or embedded again.

When does an edit stay local? The edited paragraph `p` becomes `p'`. If the
fence before it and the fence after it are decided the same way for `p'` as for
`p` (it did not become a heading, did not grow past the limit, and its hash
picked it or not the same way), then every segment except the one holding the
paragraph is unchanged, and so are all their chunks.

What must never happen: an edit in one place shifting chunk boundaries far away.
Plain greedy packing over the whole text does exactly that (one longer
paragraph pushes every later boundary), which is why segments exist.

The promises are `deterministic` and `local_edit`.
-/

namespace ChunkSplit

-- Promise (deterministic): the same paragraphs and settings always give the same chunks.
-- In Lean every function is a pure function of its inputs, so this holds by
-- construction; the Rust code keeps it by reading nothing but the text and the
-- settings (no clock, no randomness, no node identity).
theorem deterministic (cfg : Config) (ps qs : List Para) (h : ps = qs) :
    -- Equal inputs, equal chunks.
    split cfg ps = split cfg qs := by
  -- Replace one input by the other.
  rw [h]

-- Two lists of segments differ by one edit when they are equal except for a single
-- segment, which holds `p` in one list and `p'` at the same place in the other.
def OneEdit (A B : List (List Para)) (p p' : Para) : Prop :=
  -- Some segments before (`L`), some after (`R`), and the one segment in between,
  -- the same around the edited paragraph (`SL` before it, `SR` after it).
  ∃ L R SL SR, A = L ++ (SL ++ p :: SR) :: R ∧ B = L ++ (SL ++ p' :: SR) :: R

-- Putting the same segment in front of both lists keeps them one edit apart.
theorem OneEdit.cons {A B : List (List Para)} {p p' : Para} (s : List Para)
    -- Given the two lists are one edit apart,
    (h : OneEdit A B p p') :
    -- they still are with `s` in front of both.
    OneEdit (s :: A) (s :: B) p p' := by
  -- Unpack where the edit is,
  obtain ⟨L, R, SL, SR, hA, hB⟩ := h
  -- and put `s` at the front of the unchanged part before it.
  exact ⟨s :: L, R, SL, SR, by simp [hA], by simp [hB]⟩

-- Promise: paragraphs added at the start of the current segment only show up in the
-- first segment; every later segment is the same.
theorem segsFrom_prefix (cfg : Config) :
    -- For any remaining paragraphs, an extra start `pre`, a current segment and its last paragraph,
    ∀ (zs : List Para) (pre cur : List Para) (last : Para),
      -- the walk gives some first segment `s` and later segments `rest`, and with
      -- `pre` in front it gives `pre ++ s` and the very same `rest`.
      ∃ s rest, segsFrom cfg cur last zs = s :: rest ∧
        segsFrom cfg (pre ++ cur) last zs = (pre ++ s) :: rest := by
  -- Take the remaining paragraphs.
  intro zs
  -- Go through them one at a time.
  induction zs with
  -- None left: the current segment is the only one, with or without `pre`.
  | nil => intro pre cur last; exact ⟨cur, [], by simp [segsFrom], by simp [segsFrom]⟩
  -- One more paragraph `q`:
  | cons q qs ih =>
    -- take the extra start, the current segment and its last paragraph,
    intro pre cur last
    -- open the definition,
    simp only [segsFrom]
    -- and split on whether a fence comes before `q` (it depends on `last` and `q` only).
    split
    -- A fence: the current segment closes; only it carries `pre`.
    · exact ⟨cur, _, rfl, rfl⟩
    -- No fence: `q` joins the current segment, and the step before applies.
    · obtain ⟨s, rest, h1, h2⟩ := ih pre (cur ++ [q]) q
      refine ⟨s, rest, h1, ?_⟩
      simpa using h2

-- Promise: editing the last paragraph of the current segment, keeping the fence after it,
-- changes only the segment that holds it.
theorem segsFrom_tail_edit (cfg : Config) (p p' : Para) :
    -- For any later paragraphs and any start of the current segment,
    ∀ (ys : List Para) (cur : List Para),
      -- if the fence before the next paragraph is decided the same for `p` and `p'`,
      (∀ b, ys.head? = some b → fence cfg p b = fence cfg p' b) →
      -- the two walks are one edit apart.
      OneEdit (segsFrom cfg (cur ++ [p]) p ys) (segsFrom cfg (cur ++ [p']) p' ys) p p' := by
  -- Take the later paragraphs, the start, and the fence law.
  intro ys cur hnext
  -- Look at whether anything comes after.
  cases ys with
  -- Nothing: the current segment is the only one, and it holds the edit.
  | nil => exact ⟨[], [], cur, [], by simp [segsFrom], by simp [segsFrom]⟩
  -- A next paragraph `q`:
  | cons q qs =>
    -- its fence is decided the same way after `p` and after `p'`;
    have hq : fence cfg p q = fence cfg p' q := hnext q rfl
    -- open the definition, using that.
    simp only [segsFrom, hq]
    split
    -- A fence: the edited segment closes, and the rest is the same walk from `q`.
    · exact ⟨[], segsFrom cfg [q] q qs, cur, [], by simp, by simp⟩
    -- No fence: `q` joins the edited segment; the walk from `q` gives `s :: rest`,
    -- and the edited segment is `cur ++ [p] ++ s` (or with `p'`), followed by `rest`.
    · obtain ⟨s, rest, h0, h⟩ := segsFrom_prefix cfg qs (cur ++ [p]) [q] q
      obtain ⟨s', rest', h0', h'⟩ := segsFrom_prefix cfg qs (cur ++ [p']) [q] q
      -- Both describe the same walk from `q`, so they name the same `s` and `rest`.
      rw [h0] at h0'
      injection h0' with hs hr
      subst hs hr
      exact ⟨[], rest, cur, s, by simpa using h, by simpa using h'⟩

-- Promise: an edit anywhere after the start of the walk, keeping both its fences,
-- changes only the segment that holds it.
theorem segsFrom_edit (cfg : Config) (p p' : Para) (ys : List Para)
    -- The fence after the edited paragraph is decided the same for `p` and `p'`.
    (hnext : ∀ b, ys.head? = some b → fence cfg p b = fence cfg p' b) :
    -- For any paragraphs `xs` before the edit, current segment, and last paragraph,
    ∀ (xs : List Para) (cur : List Para) (last : Para),
      -- if the fence before the edited paragraph is decided the same for `p` and `p'`,
      (∀ a, (last :: xs).getLast? = some a → fence cfg a p = fence cfg a p') →
      -- the two walks are one edit apart.
      OneEdit (segsFrom cfg cur last (xs ++ p :: ys))
        (segsFrom cfg cur last (xs ++ p' :: ys)) p p' := by
  -- Take the paragraphs before the edit.
  intro xs
  -- Go through them one at a time.
  induction xs with
  -- The edited paragraph comes next:
  | nil =>
    -- take the current segment, its last paragraph, and the fence law;
    intro cur last hprev
    -- the fence before it is decided the same way for `p` and `p'`;
    have hl : fence cfg last p = fence cfg last p' := hprev last (by simp)
    -- open the definition, using that.
    simp only [List.nil_append, segsFrom, hl]
    split
    -- A fence: the current segment closes (the same in both), and the edited
    -- paragraph opens a new segment; that case is the step before.
    · apply OneEdit.cons
      simpa using segsFrom_tail_edit cfg p p' ys [] hnext
    -- No fence: the edited paragraph joins the current segment; the step before again.
    · exact segsFrom_tail_edit cfg p p' ys cur hnext
  -- An unchanged paragraph `x` comes next:
  | cons x xs ih =>
    -- take the current segment, its last paragraph, and the fence law;
    intro cur last hprev
    -- from `x` on, the paragraph before the edit is still the same one.
    have hprev' : ∀ a, (x :: xs).getLast? = some a → fence cfg a p = fence cfg a p' := by
      intro a ha
      apply hprev a
      rw [List.getLast?_cons_cons]
      exact ha
    -- Open the definition: the fence before `x` is the same in both walks.
    simp only [List.cons_append, segsFrom]
    split
    -- A fence: the same segment closes in both, and the rest is one edit apart.
    · exact OneEdit.cons cur (ih [x] x hprev')
    -- No fence: `x` joins the current segment in both, and the rest is one edit apart.
    · exact ih (cur ++ [x]) x hprev'

-- Promise: an edit that keeps both of its fences changes exactly one segment.
theorem segments_edit (cfg : Config) (pre post : List Para) (p p' : Para)
    -- The fence before the edited paragraph is decided the same for `p` and `p'`,
    (hprev : ∀ a, pre.getLast? = some a → fence cfg a p = fence cfg a p')
    -- and so is the fence after it.
    (hnext : ∀ b, post.head? = some b → fence cfg p b = fence cfg p' b) :
    -- The segments before and after the edit are one edit apart.
    OneEdit (segments cfg (pre ++ p :: post)) (segments cfg (pre ++ p' :: post)) p p' := by
  -- Look at whether anything comes before the edit.
  cases pre with
  -- Nothing: the edited paragraph opens the first segment.
  | nil => simpa [segments] using segsFrom_tail_edit cfg p p' post [] hnext
  -- Something: the walk starts at the first paragraph and reaches the edit later.
  | cons x xs =>
    simp only [List.cons_append, segments]
    exact segsFrom_edit cfg p p' post hnext xs [x] x hprev

-- Promise (local_edit): an edit inside one paragraph that keeps its fences leaves every
-- chunk outside that paragraph's segment exactly the same.
theorem local_edit (cfg : Config) (pre post : List Para) (p p' : Para)
    -- The fence before the edited paragraph is decided the same for `p` and `p'`,
    (hprev : ∀ a, pre.getLast? = some a → fence cfg a p = fence cfg a p')
    -- and so is the fence after it.
    (hnext : ∀ b, post.head? = some b → fence cfg p b = fence cfg p' b) :
    -- The chunks before and after the edit are: the same chunks from the segments `L`
    -- before, then the edited segment's chunks, then the same chunks from the segments `R`.
    ∃ (L R : List (List Para)) (SL SR : List Para),
      split cfg (pre ++ p :: post) =
        L.flatMap (segChunks cfg) ++ segChunks cfg (SL ++ p :: SR) ++ R.flatMap (segChunks cfg) ∧
      split cfg (pre ++ p' :: post) =
        L.flatMap (segChunks cfg) ++ segChunks cfg (SL ++ p' :: SR) ++ R.flatMap (segChunks cfg) := by
  -- The segments are one edit apart;
  obtain ⟨L, R, SL, SR, hA, hB⟩ := segments_edit cfg pre post p p' hprev hnext
  -- every segment is chunked on its own, so the chunks are too.
  refine ⟨L, R, SL, SR, ?_, ?_⟩
  · simp [split, hA, List.flatMap_append]
  · simp [split, hB, List.flatMap_append]

end ChunkSplit
