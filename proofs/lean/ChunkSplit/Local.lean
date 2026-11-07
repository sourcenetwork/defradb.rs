import ChunkSplit.Split

/-!
# ChunkSplit.Local: the same text gives the same chunks, and an edit stays local

The story, in plain words. Two nodes that cut the same text must get the same
chunks, so they make the same chunk documents. And when someone fixes a typo
in one paragraph of a long article, only the chunks around that paragraph may
change; every other chunk must stay exactly the same, so it is not written,
signed, replicated or embedded again.

When does an edit stay local? The edited paragraph `p` becomes `p'`, and stays a
heading or stays text. Units start only where the heading flags say, so the
units stay the same except the one holding `p`. A fence is decided by its own
unit alone, so if the edited unit keeps its fence, every segment except the one
holding it is unchanged, and so are all their chunks.

The walk that cuts units and the walk that cuts segments are the same generic
walk (`group`), so the edit lemmas are proved once for any list and any break
rule that reads only the two neighbours.

What must never happen: an edit in one place shifting chunk boundaries far away.
Plain greedy packing over the whole text does exactly that (one longer
paragraph pushes every later boundary), which is why segments exist.

The promises are `deterministic`, `units_edit` and `local_edit`.
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

-- Two lists of groups differ by one edit when they are equal except for a single
-- group, which holds `p` in one list and `p'` at the same place in the other.
def OneEdit {α : Type} (A B : List (List α)) (p p' : α) : Prop :=
  -- Some groups before (`L`), some after (`R`), and the one group in between,
  -- the same around the edited element (`SL` before it, `SR` after it).
  ∃ L R SL SR, A = L ++ (SL ++ p :: SR) :: R ∧ B = L ++ (SL ++ p' :: SR) :: R

-- Putting the same group in front of both lists keeps them one edit apart.
theorem OneEdit.cons {α : Type} {A B : List (List α)} {p p' : α} (s : List α)
    -- Given the two lists are one edit apart,
    (h : OneEdit A B p p') :
    -- they still are with `s` in front of both.
    OneEdit (s :: A) (s :: B) p p' := by
  -- Unpack where the edit is,
  obtain ⟨L, R, SL, SR, hA, hB⟩ := h
  -- and put `s` at the front of the unchanged part before it.
  exact ⟨s :: L, R, SL, SR, by simp [hA], by simp [hB]⟩

-- Promise: elements added at the start of the current group only show up in the
-- first group; every later group is the same.
theorem groupFrom_prefix {α : Type} (brk : α → α → Bool) :
    -- For any remaining elements, an extra start `pre`, a current group and its last element,
    ∀ (zs : List α) (pre cur : List α) (last : α),
      -- the walk gives some first group `s` and later groups `rest`, and with
      -- `pre` in front it gives `pre ++ s` and the very same `rest`.
      ∃ s rest, groupFrom brk cur last zs = s :: rest ∧
        groupFrom brk (pre ++ cur) last zs = (pre ++ s) :: rest := by
  -- Take the remaining elements.
  intro zs
  -- Go through them one at a time.
  induction zs with
  -- None left: the current group is the only one, with or without `pre`.
  | nil => intro pre cur last; exact ⟨cur, [], by simp [groupFrom], by simp [groupFrom]⟩
  -- One more element `q`:
  | cons q qs ih =>
    -- take the extra start, the current group and its last element,
    intro pre cur last
    -- open the definition,
    simp only [groupFrom]
    -- and split on whether a break comes before `q` (it depends on `last` and `q` only).
    split
    -- A break: the current group closes; only it carries `pre`.
    · exact ⟨cur, _, rfl, rfl⟩
    -- No break: `q` joins the current group, and the step before applies.
    · obtain ⟨s, rest, h1, h2⟩ := ih pre (cur ++ [q]) q
      refine ⟨s, rest, h1, ?_⟩
      simpa using h2

-- Promise: editing the last element of the current group, keeping the break after it,
-- changes only the group that holds it.
theorem groupFrom_tail_edit {α : Type} (brk : α → α → Bool) (p p' : α) :
    -- For any later elements and any start of the current group,
    ∀ (ys : List α) (cur : List α),
      -- if the break before the next element is decided the same for `p` and `p'`,
      (∀ b, ys.head? = some b → brk p b = brk p' b) →
      -- the two walks are one edit apart.
      OneEdit (groupFrom brk (cur ++ [p]) p ys) (groupFrom brk (cur ++ [p']) p' ys) p p' := by
  -- Take the later elements, the start, and the break law.
  intro ys cur hnext
  -- Look at whether anything comes after.
  cases ys with
  -- Nothing: the current group is the only one, and it holds the edit.
  | nil => exact ⟨[], [], cur, [], by simp [groupFrom], by simp [groupFrom]⟩
  -- A next element `q`:
  | cons q qs =>
    -- its break is decided the same way after `p` and after `p'`;
    have hq : brk p q = brk p' q := hnext q rfl
    -- open the definition, using that.
    simp only [groupFrom, hq]
    split
    -- A break: the edited group closes, and the rest is the same walk from `q`.
    · exact ⟨[], groupFrom brk [q] q qs, cur, [], by simp, by simp⟩
    -- No break: `q` joins the edited group; the walk from `q` gives `s :: rest`,
    -- and the edited group is `cur ++ [p] ++ s` (or with `p'`), followed by `rest`.
    · obtain ⟨s, rest, h0, h⟩ := groupFrom_prefix brk qs (cur ++ [p]) [q] q
      obtain ⟨s', rest', h0', h'⟩ := groupFrom_prefix brk qs (cur ++ [p']) [q] q
      -- Both describe the same walk from `q`, so they name the same `s` and `rest`.
      rw [h0] at h0'
      injection h0' with hs hr
      subst hs hr
      exact ⟨[], rest, cur, s, by simpa using h, by simpa using h'⟩

-- Promise: an edit anywhere after the start of the walk, keeping both its breaks,
-- changes only the group that holds it.
theorem groupFrom_edit {α : Type} (brk : α → α → Bool) (p p' : α) (ys : List α)
    -- The break after the edited element is decided the same for `p` and `p'`.
    (hnext : ∀ b, ys.head? = some b → brk p b = brk p' b) :
    -- For any elements `xs` before the edit, current group, and last element,
    ∀ (xs : List α) (cur : List α) (last : α),
      -- if the break before the edited element is decided the same for `p` and `p'`,
      (∀ a, (last :: xs).getLast? = some a → brk a p = brk a p') →
      -- the two walks are one edit apart.
      OneEdit (groupFrom brk cur last (xs ++ p :: ys))
        (groupFrom brk cur last (xs ++ p' :: ys)) p p' := by
  -- Take the elements before the edit.
  intro xs
  -- Go through them one at a time.
  induction xs with
  -- The edited element comes next:
  | nil =>
    -- take the current group, its last element, and the break law;
    intro cur last hprev
    -- the break before it is decided the same way for `p` and `p'`;
    have hl : brk last p = brk last p' := hprev last (by simp)
    -- open the definition, using that.
    simp only [List.nil_append, groupFrom, hl]
    split
    -- A break: the current group closes (the same in both), and the edited
    -- element opens a new group; that case is the step before.
    · apply OneEdit.cons
      simpa using groupFrom_tail_edit brk p p' ys [] hnext
    -- No break: the edited element joins the current group; the step before again.
    · exact groupFrom_tail_edit brk p p' ys cur hnext
  -- An unchanged element `x` comes next:
  | cons x xs ih =>
    -- take the current group, its last element, and the break law;
    intro cur last hprev
    -- from `x` on, the element before the edit is still the same one.
    have hprev' : ∀ a, (x :: xs).getLast? = some a → brk a p = brk a p' := by
      intro a ha
      apply hprev a
      rw [List.getLast?_cons_cons]
      exact ha
    -- Open the definition: the break before `x` is the same in both walks.
    simp only [List.cons_append, groupFrom]
    split
    -- A break: the same group closes in both, and the rest is one edit apart.
    · exact OneEdit.cons cur (ih [x] x hprev')
    -- No break: `x` joins the current group in both, and the rest is one edit apart.
    · exact ih (cur ++ [x]) x hprev'

-- Promise: an edit that keeps both of its breaks changes exactly one group.
theorem group_edit {α : Type} (brk : α → α → Bool) (pre post : List α) (p p' : α)
    -- The break before the edited element is decided the same for `p` and `p'`,
    (hprev : ∀ a, pre.getLast? = some a → brk a p = brk a p')
    -- and so is the break after it.
    (hnext : ∀ b, post.head? = some b → brk p b = brk p' b) :
    -- The groups before and after the edit are one edit apart.
    OneEdit (group brk (pre ++ p :: post)) (group brk (pre ++ p' :: post)) p p' := by
  -- Look at whether anything comes before the edit.
  cases pre with
  -- Nothing: the edited element opens the first group.
  | nil => simpa [group] using groupFrom_tail_edit brk p p' post [] hnext
  -- Something: the walk starts at the first element and reaches the edit later.
  | cons x xs =>
    simp only [List.cons_append, group]
    exact groupFrom_edit brk p p' post hnext xs [x] x hprev

-- Promise: an edit that keeps the paragraph a heading or text changes only its own unit.
theorem units_edit (pre post : List Para) (p p' : Para)
    -- The edit keeps the heading flag.
    (hh : p.heading = p'.heading) :
    -- The units before and after the edit are one edit apart.
    OneEdit (units (pre ++ p :: post)) (units (pre ++ p' :: post)) p p' := by
  -- Unit breaks read only heading flags, and `p` and `p'` share theirs.
  exact group_edit unitBreak pre post p p'
    (fun a _ => unit_break_reads_only_headings a a p p' rfl hh)
    (fun b _ => unit_break_reads_only_headings p p' b b hh rfl)

-- Promise (local_edit): an edit inside one paragraph that keeps it a heading or text,
-- and keeps the fence before its unit, leaves every chunk outside that unit's
-- segment exactly the same.
theorem local_edit (cfg : Config) (pre post : List Para) (p p' : Para)
    -- The edit keeps the heading flag.
    (hh : p.heading = p'.heading) :
    -- There is a unit around the edit (`UL` before `p`, `UR` after it), and when its
    -- fence is the same before and after the edit,
    ∃ UL UR, fence cfg (UL ++ p :: UR) = fence cfg (UL ++ p' :: UR) →
      -- the chunks are: the same chunks from the segments `L` before, the edited
      -- segment's chunks, then the same chunks from the segments `R` after.
      ∃ (L R : List (List (List Para))) (SL SR : List (List Para)),
        split cfg (pre ++ p :: post) =
          L.flatMap (fun seg => segChunks cfg seg.flatten) ++
            segChunks cfg (SL ++ (UL ++ p :: UR) :: SR).flatten ++
            R.flatMap (fun seg => segChunks cfg seg.flatten) ∧
        split cfg (pre ++ p' :: post) =
          L.flatMap (fun seg => segChunks cfg seg.flatten) ++
            segChunks cfg (SL ++ (UL ++ p' :: UR) :: SR).flatten ++
            R.flatMap (fun seg => segChunks cfg seg.flatten) := by
  -- The units are one edit apart, around the unit `UL ++ p :: UR`.
  obtain ⟨Lu, Ru, UL, UR, hA, hB⟩ := units_edit pre post p p' hh
  -- Take that unit, and suppose it keeps its fence.
  refine ⟨UL, UR, fun hf => ?_⟩
  -- A fence reads only the unit after it, so the segments are one edit apart too.
  obtain ⟨L, R, SL, SR, hsA, hsB⟩ :=
    group_edit (fun _ u => fence cfg u) Lu Ru (UL ++ p :: UR) (UL ++ p' :: UR)
      (fun _ _ => hf) (fun _ _ => rfl)
  -- Every segment is chunked on its own, so the chunks are too.
  refine ⟨L, R, SL, SR, ?_, ?_⟩
  · simp only [split, segments, hA, hsA, List.flatMap_append, List.flatMap_cons,
      List.append_assoc]
  · simp only [split, segments, hB, hsB, List.flatMap_append, List.flatMap_cons,
      List.append_assoc]

end ChunkSplit
