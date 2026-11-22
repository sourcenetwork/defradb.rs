/-!
# CommitsDepthCast: a negative `_commits` depth, and the walk below a commit

The story, in plain words. `_commits(cid: C, depth: N)` lists commit `C`
and the commits below it, following each commit's heads (its parents in the
document's history), down to `N - 1` heads below `C`.

Who acts: the parser, which reads `depth` (an Int, so possibly negative)
into an unsigned 64-bit number, and the walk (`CommitsFetcher::traverse_depth`),
which visits the commits below `C` with a stack.

What can happen:
- `depth` is negative. Go casts it to an unsigned number, so it becomes a
  huge depth: no limit. The parser does the same cast (`as u64`).
- the history is merged: one commit is reached through several heads, along
  paths of different lengths.

What the walk does: it pushes a commit's heads in order and pops the last one
first. When it pops a commit it looks at the most remaining depth that commit
was already walked with: never walked, it lists the commit and walks its
heads; walked with less depth, it walks its heads again without listing it;
otherwise it skips it. One guard: a commit already walked with at least
`UNLIMITED_DEPTH` (2^62) left reached the bottom of its history, so it is
always skipped; a negative depth leaves at least that everywhere
(`negative_clears_cap`).

What must never happen:
- a negative depth limiting the walk (`negative_is_huge`, `walk_unlimited`);
- a commit listed twice (`walk_nodup`);
- a commit that some path reaches within the depth left out (`walk_mem`),
  which a plain "seen" set would do when the long path is walked first
  (`seen_only_loses_a_commit`);
- an order other than Go's: the commit, then everything reached through its
  last head, then through each earlier head, skipping what is already listed
  (`walk_eq_fresh`, `go_scan_merged`, `go_scan_diamond`). Deciding at push
  time instead of pop time lists the heads first (`push_time_differs`);
- an unlimited walk loading a block more than once, which it does without
  the guard each time a short branch reaches a merge the long branch already
  walked (`ladder_loads_once`; RED `no_guard_loads_again`).

Rust anchors: `parse_field_to_select` in
`crates/engine/query/src/query_parse/parser.rs`, and
`CommitsFetcher::fetch_commit_by_cid`, `CommitsFetcher::traverse_depth` and
`CommitsFetcher::push_heads` in `crates/database/db/src/read/commits/mod.rs`.
-/

namespace CommitsDepthCast

/-! ## 1. The cast of a negative depth -/

-- 2^64, the number of values of a u64.
abbrev two64 : Int := 18446744073709551616

-- Rust's `v as u64` for an i64 `v`: the value modulo 2^64, as a natural number.
def asU64 (v : Int) : Nat := (v % two64).toNat

-- Promise: every negative i64 casts to at least 2^63, a depth far beyond any history.
theorem negative_is_huge (v : Int) (lo : -9223372036854775808 ≤ v) (neg : v < 0) :
    9223372036854775808 ≤ asU64 v := by
  unfold asU64 two64   -- open the cast
  omega                -- v % 2^64 = v + 2^64 here, so at least 2^63

-- Promise: a non-negative i64 casts to itself, so positive depths keep their meaning.
theorem nonnegative_is_itself (v : Int) (nonneg : 0 ≤ v) (hi : v < 9223372036854775808) :
    asU64 v = v.toNat := by
  unfold asU64 two64   -- open the cast
  omega                -- below 2^64 the modulo changes nothing

-- Promise: `depth: -1` is `u64::MAX`, the value the parser test checks.
theorem minus_one_is_max : asU64 (-1) = 18446744073709551615 := by
  unfold asU64 two64   -- open the cast
  omega                -- -1 % 2^64 = 2^64 - 1

-- Promise: below a negative depth's start (`asU64 d - 1` left), every commit
-- fewer than 2^62 heads down still has at least `UNLIMITED_DEPTH` (2^62)
-- left, so the walk's guard holds for all of them.
theorem negative_clears_cap (v : Int) (k : Nat) (lo : -9223372036854775808 ≤ v) (neg : v < 0)
    (near : k < 4611686018427387904) :
    4611686018427387904 ≤ asU64 v - 1 - k := by
  have := negative_is_huge v lo neg   -- the start is at least 2^63 - 1
  omega                               -- 2^63 - 1 - k is at least 2^62

/-! ## 2. The walk below a commit -/

section Walk

-- A commit's heads, in the order its block stores them.
variable (heads : Nat → List Nat)

-- The path walk the old code made, listing a commit once per path, with the
-- last head first: `r` is the depth left below the commit.
def pathLast : Nat → Nat → List Nat
  | v, 0 => [v]                                                  -- no depth left: only v
  | v, r + 1 => v :: (heads v).reverse.flatMap (pathLast · r)    -- v, then each head, last first

-- The same path walk with the first head first, the order the old code used.
def pathFwd : Nat → Nat → List Nat
  | v, 0 => [v]                                                  -- no depth left: only v
  | v, r + 1 => v :: (heads v).flatMap (pathFwd · r)             -- v, then each head, first first

-- `fresh E L`: the items of `L` not in `E`, each at its first occurrence.
-- This is what listing "each commit once, where it is first met" gives.
def fresh : (Nat → Bool) → List Nat → List Nat
  | _, [] => []                                                  -- nothing left
  | E, x :: xs =>                                                -- the next item x:
      if E x then fresh E xs                                     -- already listed: skip
      else x :: fresh (fun y => E y || y == x) xs                -- list it, remember it

-- The most remaining depth each commit was walked with (none: not yet).
abbrev Best := Nat → Option Nat

-- Records that `v` was walked with `r` left.
def note (m : Best) (v r : Nat) : Best := fun x => if x = v then some r else m x

-- Whether `x` was already listed.
def seen (m : Best) (x : Nat) : Bool := (m x).isSome

-- The stack entries a popped commit pushes (`push_heads`): its heads with one
-- less depth, the last head on top, and nothing at depth 0.
def push (v : Nat) : Nat → List (Nat × Nat)
  | 0 => []                                                      -- no depth left
  | r + 1 => (heads v).reverse.map (fun h => (h, r))             -- last head on top

-- `UNLIMITED_DEPTH` (2^62): a commit walked with at least this much left was
-- walked to the bottom, so it is never walked again.
variable (cap : Nat)

-- The stack loop, as `traverse_depth` runs it, with `fuel` bounding its steps
-- (none: the fuel ran out). The stack's top is the list's front. The Rust
-- code starts it after the first pop: `start` noted, its heads pushed.
def run : Nat → List (Nat × Nat) → Best → Option (List Nat × Best)
  | _, [], m => some ([], m)                                     -- empty stack: done
  | 0, _ :: _, _ => none                                         -- out of fuel
  | f + 1, (v, r) :: rest, m =>                                  -- pop v with r left
      match m v with                                             -- decide at pop time
      | none =>                                                  -- never walked:
          (run f (push heads v r ++ rest) (note m v r)).map      --   walk its heads
            (fun p => (v :: p.1, p.2))                           --   and list it
      | some b =>                                                -- walked with b left:
          if b < r ∧ b < cap then                                -- more now, and b not unlimited:
            run f (push heads v r ++ rest) (note m v r)          --   walk again
          else run f rest m                                      -- otherwise skip it

-- The same walk written recursively: `proc v r m` handles one popped entry
-- and everything it pushes, `walkHeads` a list of heads in turn.
def walkHeads (p : Nat → Best → List Nat × Best) : List Nat → Best → List Nat × Best
  | [], m => ([], m)                                             -- no heads left
  | h :: hs, m =>                                                -- the next head h:
      let a := p h m                                             -- the first head's walk,
      let b := walkHeads p hs a.2                                -- then the others'
      (a.1 ++ b.1, b.2)                                          -- both lists, the last map

def proc : Nat → Nat → Best → List Nat × Best
  | v, 0, m =>                                                   -- v with no depth left:
      match m v with                                             -- decide on v
      | none => ([v], note m v 0)                                -- list it, no heads
      | some _ => ([], m)                                        -- already walked: skip
  | v, r + 1, m =>                                               -- v with depth left:
      let w := walkHeads (proc · r) (heads v).reverse (note m v (r + 1))  -- walk the heads
      match m v with                                             -- decide on v
      | none => (v :: w.1, w.2)                                  -- new: list it, then them
      | some b => if b < r + 1 ∧ b < cap then w else ([], m)     -- more depth, or skip

/-! ### The stack loop is the recursive walk -/

-- Promise: running the heads' entries, then the rest, gives the heads' walk,
-- then the rest (given that one entry does).
theorem run_walkHeads (r : Nat)
    (ih : ∀ v m rest o m', (∃ f, run heads cap f rest (proc heads cap v r m).2 = some (o, m')) →
      ∃ f, run heads cap f ((v, r) :: rest) m = some ((proc heads cap v r m).1 ++ o, m')) :
    ∀ (hs : List Nat) m rest o m',
      (∃ f, run heads cap f rest (walkHeads (proc heads cap · r) hs m).2 = some (o, m')) →
      ∃ f, run heads cap f (hs.map (fun h => (h, r)) ++ rest) m =
        some ((walkHeads (proc heads cap · r) hs m).1 ++ o, m') := by
  intro hs                                                       -- the heads
  induction hs with                                              -- one head at a time
  | nil => intro m rest o m' h; simpa [walkHeads] using h        -- no heads: just the rest
  | cons h hs ihs =>                                             -- head h, then hs
    intro m rest o m' hrun                                       -- the rest runs to o
    -- the other heads, then the rest, from the first head's result
    obtain ⟨f, hf⟩ := ihs (proc heads cap h r m).2 rest o m' (by simpa [walkHeads] using hrun)
    -- then the first head in front of them
    obtain ⟨g, hg⟩ := ih h m _ _ _ ⟨f, hf⟩
    exact ⟨g, by simpa [walkHeads, List.append_assoc] using hg⟩  -- the lists line up

-- Promise: one popped entry, then the rest, runs as the recursive walk of
-- that entry followed by the rest.
theorem run_proc : ∀ r v m rest o m',
    (∃ f, run heads cap f rest (proc heads cap v r m).2 = some (o, m')) →
    ∃ f, run heads cap f ((v, r) :: rest) m = some ((proc heads cap v r m).1 ++ o, m') := by
  intro r                                                        -- the depth left
  induction r with                                               -- on the depth left
  | zero =>                                                      -- no depth: no heads pushed
    intro v m rest o m' ⟨f, hf⟩                                  -- the rest runs in f steps
    refine ⟨f + 1, ?_⟩                                           -- one more for v
    cases hv : m v <;> simp_all [run, proc, push]                -- list it, or skip it
  | succ r ih =>                                                 -- depth left: heads pushed
    intro v m rest o m' ⟨f, hf⟩                                  -- the rest runs in f steps
    have hw := run_walkHeads heads cap r ih (heads v).reverse (note m v (r + 1)) rest o m'
    cases hv : m v with                                          -- what v's mark says
    | none =>                                                    -- never walked
      simp only [proc, hv] at hf                                 -- the rest, after the heads
      obtain ⟨g, hg⟩ := hw ⟨f, hf⟩                               -- the heads, then the rest
      refine ⟨g + 1, ?_⟩                                         -- one more step for v
      simp only [run, proc, hv, push, hg]; rfl                   -- v listed in front
    | some b =>                                                  -- walked with b left
      by_cases hb : b < r + 1 ∧ b < cap                          -- walk again, or not
      · simp only [proc, hv, hb, and_self, if_true] at hf        -- walked with less depth
        obtain ⟨g, hg⟩ := hw ⟨f, hf⟩                             -- the heads, then the rest
        refine ⟨g + 1, ?_⟩                                       -- one more step for v
        simp only [run, proc, hv, hb, and_self, if_true, push, hg]  -- walked again, not listed
      · simp only [proc, hv, hb, if_false] at hf                 -- deep enough, or unlimited: skip
        exact ⟨f + 1, by simp [run, proc, hv, hb, hf]⟩           -- just the rest

/-! ### What the recursive walk lists -/

-- Promise: `fresh E L` holds exactly the items of `L` not in `E`.
theorem mem_fresh : ∀ (L : List Nat) (E : Nat → Bool) x,
    x ∈ fresh E L ↔ x ∈ L ∧ E x = false := by
  intro L                                                        -- the list
  induction L with                                               -- one item at a time
  | nil => simp [fresh]                                          -- nothing in either
  | cons y ys ih =>                                              -- item y, then ys
    intro E x                                                    -- the set and the item
    by_cases hy : E y = true                                     -- y listed already, or not
    · simp only [fresh, hy, if_true, ih]                         -- y skipped
      constructor                                                -- both directions:
      · intro ⟨h1, h2⟩; exact ⟨List.mem_cons_of_mem _ h1, h2⟩    -- in ys, so in y :: ys
      · intro ⟨h1, h2⟩                                           -- in y :: ys, not in E:
        rcases List.mem_cons.mp h1 with rfl | h1                 -- it is y, or in ys
        · simp_all                                               -- y is in E: impossible
        · exact ⟨h1, h2⟩                                         -- in ys
    · simp only [fresh, hy, ih]                                  -- y listed
      by_cases hxy : x = y <;> simp_all                          -- x is y, or another

-- Promise: `fresh` lists no item twice.
theorem fresh_nodup : ∀ (L : List Nat) (E : Nat → Bool), (fresh E L).Nodup := by
  intro L                                                        -- the list
  induction L with                                               -- one item at a time
  | nil => simp [fresh]                                          -- empty: no repeats
  | cons y ys ih =>                                              -- item y, then ys
    intro E                                                      -- the listed set
    by_cases hy : E y = true                                     -- y listed already, or not
    · simp [fresh, hy, ih]                                       -- y skipped
    · simp only [fresh, hy]                                      -- y listed once:
      refine List.nodup_cons.mpr ⟨?_, ih _⟩                      -- it is remembered
      intro h; simp [mem_fresh] at h                             -- so not listed again

-- Promise: `fresh` depends only on which items its set holds.
theorem fresh_congr (L : List Nat) (E F : Nat → Bool) (h : ∀ y, E y = F y) :
    fresh E L = fresh F L := by
  have : E = F := funext h                                       -- the same set
  rw [this]                                                      -- so the same list

-- Promise: `fresh` of two lists is `fresh` of the first, then of the second
-- with the first's items remembered.
theorem fresh_append : ∀ (A B : List Nat) (E : Nat → Bool),
    fresh E (A ++ B) = fresh E A ++ fresh (fun y => E y || decide (y ∈ A)) B := by
  intro A                                                        -- the first list
  induction A with                                               -- one item at a time
  | nil => intro B E; simp [fresh]                               -- empty first list
  | cons a as ih =>                                              -- item a, then as
    intro B E                                                    -- the second list, the set
    by_cases ha : E a = true                                     -- a listed already, or not
    · simp only [List.cons_append, fresh, ha, if_true, ih]       -- a skipped
      congr 1; apply fresh_congr; intro y                        -- the remembered sets agree:
      by_cases hya : y = a <;> simp_all                          -- a was in E already
    · simp only [List.cons_append, fresh, ha, ih]                -- a listed
      simp only [Bool.false_eq_true, if_false, List.cons_append] -- a in front of both
      congr 2; apply fresh_congr; intro y                        -- the remembered sets agree:
      by_cases hya : y = a                                       -- y is a, or another
      · simp_all [Bool.or_assoc]                                 -- a is remembered either way
      · have hb : (y == a) = false := by simp [hya]              -- y is not a
        simp [Bool.or_assoc, hb, hya]                            -- so only E and as count

-- Promise: when every item is already listed, `fresh` lists nothing.
theorem fresh_seen : ∀ (L : List Nat) (E : Nat → Bool), (∀ x ∈ L, E x = true) →
    fresh E L = [] := by
  intro L                                                        -- the list
  induction L with                                               -- one item at a time
  | nil => intro _ _; rfl                                        -- empty: nothing
  | cons y ys ih =>                                              -- item y, then ys
    intro E h                                                    -- every item is in E
    simp [fresh, h y (List.mem_cons_self _ _), ih E (fun x hx => h x (List.mem_cons_of_mem _ hx))]

-- Promise: a path walk with less depth reaches nothing a deeper one misses.
theorem pathLast_mono : ∀ r v b x, r ≤ b → x ∈ pathLast heads v r → x ∈ pathLast heads v b := by
  intro r                                                        -- the smaller depth
  induction r with                                               -- on the smaller depth
  | zero =>                                                      -- depth 0 reaches only v
    intro v b x _ hx                                             -- x is v
    cases b <;> simp_all [pathLast]                              -- v starts every path walk
  | succ r ih =>                                                 -- depth r + 1
    intro v b x hrb hx                                           -- b is at least r + 1
    obtain ⟨b, rfl⟩ : ∃ b', b = b' + 1 := ⟨b - 1, by omega⟩      -- so b is b' + 1
    simp only [pathLast, List.mem_cons, List.mem_flatMap, List.mem_reverse] at hx ⊢
    rcases hx with rfl | ⟨h, hh, hx⟩                             -- x is v, or below a head h
    · exact Or.inl rfl                                           -- v: reached
    · exact Or.inr ⟨h, hh, ih h b x (by omega) hx⟩               -- below h with less depth

-- The walk's invariant: every commit walked with `b` left, other than the
-- open ones `X` (whose heads are still being walked), has everything its
-- path walk reaches already listed.
def Inv (X : Nat → Prop) (m : Best) : Prop :=
  ∀ w b, m w = some b → ¬ X w → ∀ x ∈ pathLast heads w b, seen m x = true

-- What one entry's walk guarantees, given the invariant and that no open
-- commit lies at or below it (ranks fall along heads, so this is a DAG):
-- it lists the new commits of its path walk in order, marks all of them,
-- keeps the invariant, and touches no commit above it.
def Spec (rank : Nat → Nat) (p : Nat → Best → List Nat × Best) (v r : Nat) : Prop :=
  ∀ (X : Nat → Prop) m, (∀ x, X x → rank v < rank x) → Inv heads X m →
    (p v m).1 = fresh (seen m) (pathLast heads v r) ∧                         -- what it lists
    (∀ x, seen (p v m).2 x = (seen m x || decide (x ∈ pathLast heads v r))) ∧ -- what it marks
    Inv heads X (p v m).2 ∧                                                   -- the invariant
    (∀ w, rank v < rank w → (p v m).2 w = m w)                                -- nothing above

-- Each commit's height: every head sits strictly lower than its commit.
variable (rank : Nat → Nat)

-- Promise: walking each item the same way gives the same concatenation.
theorem flatMap_congr {L : List Nat} {f g : Nat → List Nat} (h : ∀ x ∈ L, f x = g x) :
    L.flatMap f = L.flatMap g := by
  induction L with                               -- one item at a time
  | nil => rfl                                   -- no items: both empty
  | cons y ys ih =>                              -- the first item, then the rest
    simp only [List.flatMap_cons]                -- split off the first item
    rw [h y (List.mem_cons_self _ _), ih (fun x hx => h x (List.mem_cons_of_mem _ hx))]

-- Promise: with at least as much depth as the commit's height, more depth
-- changes nothing, because the walk already reaches the first commit.
theorem pathLast_stable (dag : ∀ v h, h ∈ heads v → rank h < rank v) :
    ∀ r r' v, rank v ≤ r → rank v ≤ r' → pathLast heads v r = pathLast heads v r' := by
  intro r                                        -- one depth
  induction r with                               -- on that depth
  | zero =>                                      -- depth 0: v has height 0
    intro r' v hr _                              -- the other depth
    have : heads v = [] := by                    -- so v has no heads
      cases hh : heads v with                    -- look at its heads
      | nil => rfl                               -- none: done
      | cons h _ => have := dag v h (by simp [hh]); omega  -- a head would sit below 0
    cases r' <;> simp [pathLast, this]           -- both walks are [v]
  | succ r ih =>                                 -- depth r + 1
    intro r' v hr hr'                            -- the other depth
    cases r' with                                -- 0, or more
    | zero =>                                    -- 0: v has height 0
      have : heads v = [] := by                  -- so v has no heads
        cases hh : heads v with                  -- look at its heads
        | nil => rfl                             -- none: done
        | cons h _ => have := dag v h (by simp [hh]); omega  -- a head would sit below 0
      simp [pathLast, this]                      -- both walks are [v]
    | succ r' =>                                 -- both have depth left
      simp only [pathLast, List.cons.injEq, true_and]  -- v, then the heads
      apply flatMap_congr                        -- each head's walk is unchanged:
      intro h hh                                 -- a head h
      have := dag v h (List.mem_reverse.mp hh)   -- sits lower than v,
      exact ih r' h (by omega) (by omega)        -- so it has enough depth either way

-- Promise: walking a list of heads, each meeting its spec, lists what their
-- path walks list, in order, each once.
theorem walkHeads_spec (r : Nat) (u : Nat) :
    ∀ (hs : List Nat), (∀ h ∈ hs, rank h < rank u) →
      (∀ h ∈ hs, Spec heads rank (proc heads cap · r) h r) →
    ∀ (X : Nat → Prop) m, (∀ x, X x → rank u ≤ rank x) → Inv heads X m →
      (walkHeads (proc heads cap · r) hs m).1 = fresh (seen m) (hs.flatMap (pathLast heads · r)) ∧
      (∀ x, seen (walkHeads (proc heads cap · r) hs m).2 x =
        (seen m x || decide (x ∈ hs.flatMap (pathLast heads · r)))) ∧
      Inv heads X (walkHeads (proc heads cap · r) hs m).2 ∧
      (∀ w, rank u ≤ rank w → (walkHeads (proc heads cap · r) hs m).2 w = m w) := by
  intro hs                                                       -- the heads
  induction hs with                                              -- one head at a time
  | nil =>                                                       -- no heads:
    intro _ _ X m _ hinv                                         -- nothing changes
    simp [walkHeads, fresh, hinv]                                -- nothing listed or marked
  | cons h hs ih =>                                              -- head h, then hs
    intro hrank hspec X m hX hinv                                -- the hypotheses
    have hh := hspec h (List.mem_cons_self _ _) X m              -- h's own spec
      (fun x hx => Nat.lt_of_lt_of_le (hrank h (List.mem_cons_self _ _)) (hX x hx)) hinv
    obtain ⟨o1, d1, i1, f1⟩ := hh                                -- what h's walk gives
    obtain ⟨o2, d2, i2, f2⟩ := ih (fun h' hm => hrank h' (List.mem_cons_of_mem _ hm))
      (fun h' hm => hspec h' (List.mem_cons_of_mem _ hm)) X _ hX i1  -- then the others'
    refine ⟨?_, ?_, ?_, ?_⟩                                      -- the four promises:
    · simp only [walkHeads, List.flatMap_cons, fresh_append, o1, o2]  -- the lists join
      congr 1                                                    -- the second part:
      exact fresh_congr _ _ _ (fun y => by rw [d1])              -- h's marks are remembered
    · intro x                                                    -- the marks:
      simp only [walkHeads, d2, d1, List.flatMap_cons, List.mem_append]
      cases seen m x <;> simp [Bool.or_assoc]                    -- h's, then the others'
    · simpa [walkHeads] using i2                                 -- the invariant holds
    · intro w hw                                                 -- above u nothing changed:
      simp only [walkHeads]                                      -- through h and the others
      rw [f2 w hw, f1 w (Nat.lt_of_lt_of_le (hrank h (List.mem_cons_self _ _)) hw)]

-- Promise: on a DAG whose heights all lie below `cap`, every entry's walk
-- meets its spec.
theorem proc_spec (dag : ∀ v h, h ∈ heads v → rank h < rank v) (hcap : ∀ v, rank v < cap) :
    ∀ r v, Spec heads rank (proc heads cap · r) v r := by
  intro r                                                        -- the depth left
  induction r with                                               -- on the depth left
  | zero =>                                                      -- no depth: only v
    intro v X m hX hinv                                          -- the hypotheses
    cases hv : m v with                                          -- what v's mark says
    | none =>                                                    -- never walked: listed
      refine ⟨?_, ?_, ?_, ?_⟩                                    -- the four promises:
      · simp [proc, hv, pathLast, fresh, seen]                   -- it lists v
      · intro x; by_cases hx : x = v <;> simp [proc, hv, pathLast, seen, note, hx]  -- marks v
      · intro w b hw hnx y hy                                    -- the invariant:
        simp only [proc, hv] at hw ⊢                             -- in the new map
        by_cases hwv : w = v                                     -- for v, or another
        · subst hwv; simp [note] at hw; subst hw                 -- v, walked with 0 left
          simp [pathLast] at hy; subst hy; simp [seen, note]     -- reaches only itself
        · simp [note, hwv] at hw                                 -- another commit:
          have := hinv w b hw hnx y hy                           -- it held before
          by_cases hyv : y = v <;> simp_all [seen, note]         -- and marks only grow
      · intro w hw                                               -- above v:
        have : w ≠ v := fun h => by subst h; omega               -- not v itself
        simp [proc, hv, note, this]                              -- so unchanged
    | some b =>                                                  -- walked already: skipped
      refine ⟨?_, ?_, ?_, ?_⟩                                    -- the four promises:
      · simp [proc, hv, pathLast, fresh, seen]                   -- nothing new listed
      · intro x; by_cases hx : x = v <;> simp [proc, hv, pathLast, seen, hx]  -- v was marked
      · simpa [proc, hv] using hinv                              -- the map is unchanged
      · intro w _; simp [proc, hv]                               -- nothing changed
  | succ r ih =>                                                 -- depth left: heads walked
    intro v X m hX hinv                                          -- the hypotheses
    -- the heads are walked with v open
    let X' : Nat → Prop := fun x => X x ∨ x = v
    have hX' : ∀ x, X' x → rank v ≤ rank x := by                 -- open commits sit at v or above
      intro x hx; rcases hx with hx | rfl                        -- open before, or v
      · exact Nat.le_of_lt (hX x hx)                             -- above v
      · exact Nat.le_refl _                                      -- v itself
    have hinv' : Inv heads X' (note m v (r + 1)) := by           -- the invariant, v open
      intro w b hw hnx y hy                                      -- a closed commit w
      have hwv : w ≠ v := fun h => hnx (Or.inr h)                -- w is not v
      simp [note, hwv] at hw                                     -- so its mark is unchanged
      have := hinv w b hw (fun h => hnx (Or.inl h)) y hy         -- it held before
      by_cases hyv : y = v <;> simp_all [seen, note]             -- and marks only grow
    have hw := walkHeads_spec heads cap rank r v (heads v).reverse   -- v's heads, last first
      (fun h hh => dag v h (List.mem_reverse.mp hh)) (fun h _ => ih h) X' _ hX' hinv'
    obtain ⟨o, d, i, f⟩ := hw                                    -- what the heads' walk gives
    -- v keeps its new mark while its heads are walked
    have fv : (walkHeads (proc heads cap · r) (heads v).reverse (note m v (r + 1))).2 v =
        some (r + 1) := by
      rw [f v (Nat.le_refl _)]; simp [note]                      -- v sits above its heads
    -- after its heads, v is no longer open
    have closed : Inv heads X (walkHeads (proc heads cap · r) (heads v).reverse (note m v (r + 1))).2 := by
      intro w b hwb hnx y hy                                     -- a closed commit w
      by_cases hwv : w = v                                       -- v, or another
      · subst hwv                                                -- v, walked with r + 1:
        rw [fv] at hwb; cases hwb                                -- its mark is r + 1
        simp only [pathLast, List.mem_cons] at hy                -- y is v, or below a head
        rcases hy with rfl | hy                                  -- which one
        · simp [seen, fv]                                        -- v is marked
        · rw [d]; simp [hy]                                      -- the heads' walk marked y
      · exact i w b hwb (fun h => by rcases h with h | h; exact hnx h; exact hwv h) y hy
    have frame : ∀ w, rank v < rank w →                          -- above v nothing changed
        (walkHeads (proc heads cap · r) (heads v).reverse (note m v (r + 1))).2 w = m w := by
      intro w hw                                                 -- a commit above v
      have : w ≠ v := fun h => by subst h; omega                 -- not v itself
      rw [f w (Nat.le_of_lt hw)]; simp [note, this]              -- untouched by the heads
    cases hv : m v with                                          -- what v's mark was
    | none =>                                                    -- never walked: listed
      refine ⟨?_, ?_, ?_, ?_⟩                                    -- the four promises:
      · simp only [proc, hv, pathLast, fresh, seen, Option.isSome_none, Bool.false_eq_true,
          if_false, List.cons.injEq, true_and]                   -- v, then the heads' list
        rw [o]; exact fresh_congr _ _ _ (fun y => by by_cases hy : y = v <;> simp [seen, note, hy])
      · intro x                                                  -- the marks:
        simp only [proc, hv, d, pathLast, List.mem_cons]         -- v and the heads' walk
        by_cases hx : x = v <;> simp [seen, note, hx]            -- x is v, or another
      · simpa [proc, hv] using closed                            -- the invariant holds
      · intro w hw; simpa [proc, hv] using frame w hw            -- nothing above changed
    | some b =>                                                  -- walked with b left
      by_cases hb : b < r + 1 ∧ b < cap                          -- walk again, or not
      · refine ⟨?_, ?_, ?_, ?_⟩                                  -- walked again, not listed:
        · simp only [proc, hv, hb, and_self, if_true, pathLast, fresh, seen, Option.isSome_some]
          rw [o]; exact fresh_congr _ _ _ (fun y => by by_cases hy : y = v <;> simp [seen, note, hy, hv])
        · intro x                                                -- the marks:
          simp only [proc, hv, hb, and_self, if_true, d, pathLast, List.mem_cons]
          by_cases hx : x = v <;> simp [seen, note, hx, hv]      -- v was marked already
        · simpa [proc, hv, hb] using closed                      -- the invariant holds
        · intro w hw; simpa [proc, hv, hb] using frame w hw      -- nothing above changed
      · -- walked with at least this depth, or with at least `cap`, which is
        -- more than v's height: either way everything below is listed
        have hvx : ¬ X v := fun h => Nat.lt_irrefl _ (hX v h)    -- v is not open
        have reach : ∀ x ∈ pathLast heads v (r + 1), x ∈ pathLast heads v b := by
          intro x hx                                             -- reached with r + 1 left
          by_cases hrb : r + 1 ≤ b                               -- b is at least r + 1:
          · exact pathLast_mono heads (r + 1) v b x hrb hx       --   b reaches more
          · have hvb : rank v ≤ b := by                          -- otherwise b ≥ cap > v's height
              have := hcap v; omega
            rw [← pathLast_stable heads rank dag (r + 1) b v (by omega) hvb]; exact hx  -- both reach all
        have all : ∀ x ∈ pathLast heads v (r + 1), seen m x = true :=  -- all already marked
          fun x hx => hinv v b hv hvx x (reach x hx)
        refine ⟨?_, ?_, ?_, ?_⟩                                  -- the four promises:
        · simp only [proc, hv, hb, if_false]; exact (fresh_seen _ _ all).symm  -- nothing new
        · intro x                                                -- the marks:
          simp only [proc, hv, hb, if_false]                     -- unchanged
          by_cases hx : x ∈ pathLast heads v (r + 1)             -- reached by v, or not
          · simp [all x hx]                                      -- reached: already marked
          · simp [hx]                                            -- not reached: as before
        · simpa [proc, hv, hb] using hinv                        -- the map is unchanged
        · intro w _; simp [proc, hv, hb]                         -- nothing changed

/-! ### The theorems -/

-- Promise: the stack loop, from commit `v` with `r` left, lists exactly the
-- path walk with the last head first, each commit at its first occurrence,
-- on any history whose heights lie below `cap` (2^62 in the Rust code).
theorem walk_eq_fresh (dag : ∀ v h, h ∈ heads v → rank h < rank v) (hcap : ∀ v, rank v < cap)
    (v r : Nat) :
    ∃ f m', run heads cap f [(v, r)] (fun _ => none) =
      some (fresh (fun _ => false) (pathLast heads v r), m') := by
  have hs := proc_spec heads cap rank dag hcap r v (fun _ => False) (fun _ => none)  -- nothing open
    (fun _ h => False.elim h) (by intro w b hw; simp at hw)      -- an empty map is fine
  obtain ⟨o, _, _, _⟩ := hs                                      -- what it lists
  obtain ⟨f, hf⟩ := run_proc heads cap r v (fun _ => none) [] [] (proc heads cap v r (fun _ => none)).2
    ⟨0, by simp [run]⟩                                           -- the stack loop runs it
  refine ⟨f, (proc heads cap v r (fun _ => none)).2, ?_⟩             -- with that final map
  rw [hf, List.append_nil, o, fresh_congr _ (seen _) (fun _ => false) (fun y => by simp [seen])]

-- Promise: the forward and last-first path walks reach the same commits.
theorem pathLast_mem_iff_pathFwd : ∀ r v x, x ∈ pathLast heads v r ↔ x ∈ pathFwd heads v r := by
  intro r                                                        -- the depth left
  induction r with                                               -- on the depth left
  | zero => intro v x; simp [pathLast, pathFwd]                  -- only v either way
  | succ r ih =>                                                 -- v, then its heads:
    intro v x                                                    -- reversing the heads
    simp [pathLast, pathFwd, List.mem_flatMap, ih]               -- keeps who is reached

-- Promise: a commit is listed exactly when the old forward walk listed it
-- (the same set).
theorem walk_mem (v r : Nat) (x : Nat) :
    x ∈ fresh (fun _ => false) (pathLast heads v r) ↔ x ∈ pathFwd heads v r := by
  rw [mem_fresh, ← pathLast_mem_iff_pathFwd]; simp               -- fresh keeps every item

-- Promise: the walk lists each commit once.
theorem walk_nodup (v r : Nat) : (fresh (fun _ => false) (pathLast heads v r)).Nodup :=
  fresh_nodup _ _                                                -- fresh never repeats

/-! ## 3. A negative depth is unlimited -/


-- Promise: `_commits(cid: v, depth: d)` with a negative `d` walks with
-- `asU64 d - 1` left below `v`; on any history below 2^63 commits high, that
-- is the walk with no limit (the one with exactly `v`'s height left).
theorem walk_unlimited (dag : ∀ v h, h ∈ heads v → rank h < rank v) (v : Nat) (d : Int)
    (lo : -9223372036854775808 ≤ d) (neg : d < 0) (height : rank v < 9223372036854775808) :
    pathLast heads v (asU64 d - 1) = pathLast heads v (rank v) := by
  have := negative_is_huge d lo neg              -- the depth is at least 2^63
  exact pathLast_stable heads rank dag _ _ v (by omega) (Nat.le_refl _)  -- beyond v's height

end Walk

/-! ## 4. Go's order, and the guards it needs -/

-- `UNLIMITED_DEPTH` in the Rust code: 2^62, more than any history's height.
abbrev unlimited : Nat := 4611686018427387904

-- Go's DAG scan at no depth limit: take the queue's front; skip it if visited;
-- otherwise list it, mark it, and put its heads in front, last head first.
def goScan (heads : Nat → List Nat) : Nat → List Nat → (Nat → Bool) → List Nat
  | _, [], _ => []                                               -- queue empty
  | 0, _ :: _, _ => []                                           -- out of fuel
  | f + 1, c :: q, vis =>                                        -- the front, c
      if vis c then goScan heads f q vis                         -- visited: skip
      else c :: goScan heads f ((heads c).reverse ++ q) (fun y => vis y || y == c)  -- list, mark

-- RED: marking at push time instead, a commit is listed when first pushed.
def pushScan (heads : Nat → List Nat) : Nat → List Nat → (Nat → Bool) → List Nat
  | _, [], _ => []                                               -- stack empty
  | 0, _ :: _, _ => []                                           -- out of fuel
  | f + 1, c :: q, vis =>                                        -- the top, c
      let new := ((heads c).reverse.filter (fun h => !vis h)).eraseDups  -- heads not yet listed
      new ++ pushScan heads f (new ++ q) (fun y => vis y || decide (y ∈ new))  -- listed now

-- RED: a plain "seen" set, which skips every commit already listed however
-- much depth it is reached with now.
def runSeen (heads : Nat → List Nat) : Nat → List (Nat × Nat) → (Nat → Bool) → List Nat
  | _, [], _ => []                                               -- stack empty
  | 0, _ :: _, _ => []                                           -- out of fuel
  | f + 1, (v, r) :: rest, vis =>                                -- pop v with r left
      if vis v then runSeen heads f rest vis                     -- seen: skip, whatever r is
      else v :: runSeen heads f (push heads v r ++ rest) (fun y => vis y || y == v)  -- list it

-- The merged history of the testo scenario: merge 6 with heads [4, 5];
-- 4 -> 2 -> 0 (one branch), 5 -> 3 -> 0 (the other), 0 the create.
def merged : Nat → List Nat
  | 6 => [4, 5]                                                  -- the merge commit
  | 4 => [2]                                                     -- first branch, top
  | 5 => [3]                                                     -- second branch, top
  | 2 => [0]                                                     -- first branch, bottom
  | 3 => [0]                                                     -- second branch, bottom
  | _ => []                                                      -- the create

-- Promise: Go lists the merge, its last head's branch down to the create,
-- then the first head's branch: [6, 5, 3, 0, 4, 2].
theorem go_scan_merged : goScan merged 20 [6] (fun _ => false) = [6, 5, 3, 0, 4, 2] := by
  decide                                                         -- run it

-- Promise: the stack loop with pop-time marking lists the same at the depth
-- a negative `depth` gives.
theorem walk_merged :
    (run merged unlimited 20 [(6, 9223372036854775807)] (fun _ => none)).map Prod.fst =
      some [6, 5, 3, 0, 4, 2] := by
  decide                                                         -- run it

-- Promise: and so does the per-path walk, deduplicated at first occurrence.
theorem fresh_merged : fresh (fun _ => false) (pathLast merged 6 3) = [6, 5, 3, 0, 4, 2] := by
  decide                                                         -- run it

-- A diamond: 3 with heads [1, 2], both with head 0.
def diamond : Nat → List Nat
  | 3 => [1, 2]                                                  -- the top
  | 1 => [0]                                                     -- one side
  | 2 => [0]                                                     -- the other side
  | _ => []                                                      -- the bottom

-- Promise: on a diamond Go lists the top, the last side, the bottom, then the
-- first side, and the stack loop does too.
theorem go_scan_diamond :
    goScan diamond 10 [3] (fun _ => false) = [3, 2, 0, 1] ∧
      (run diamond unlimited 10 [(3, 5)] (fun _ => none)).map Prod.fst = some [3, 2, 0, 1] := by
  decide                                                         -- run both

-- Promise (RED): marking at push time lists both sides before the bottom:
-- [3, 2, 1, 0], not Go's [3, 2, 0, 1].
theorem push_time_differs :
    [3] ++ pushScan diamond 10 [3] (fun y => y == 3) = [3, 2, 1, 0] := by
  decide                                                         -- run it

-- The shape of the shorter-path test: top 6 with heads [4, 5]; 4 (short)
-- -> 2; 5 (long) -> 3 -> 2; 2 -> 1 -> 0.
def shortLong : Nat → List Nat
  | 6 => [4, 5]                                                  -- top: short, then long
  | 4 => [2]                                                     -- short -> shared
  | 5 => [3]                                                     -- long -> mid
  | 3 => [2]                                                     -- mid -> shared
  | 2 => [1]                                                     -- shared -> deep
  | 1 => [0]                                                     -- deep -> deeper
  | _ => []                                                      -- deeper

-- Promise: at `depth: 5` (4 left below the top) the walk reaches 0 through
-- the short head, though the long head is walked first.
theorem walk_short_long :
    (run shortLong unlimited 30 [(6, 4)] (fun _ => none)).map Prod.fst = some [6, 5, 3, 2, 1, 4, 0] := by
  decide                                                         -- run it

-- Promise (RED): a plain seen set loses 0, which only the short head reaches
-- within the depth.
theorem seen_only_loses_a_commit :
    runSeen shortLong 30 [(6, 4)] (fun _ => false) = [6, 5, 3, 2, 1, 4] := by
  decide                                                         -- run it

/-! ## 5. An unlimited walk loads each block once -/

-- How many blocks the stack loop loads: one per pop it does not skip, the
-- same decisions as `run`.
def loads (heads : Nat → List Nat) (cap : Nat) : Nat → List (Nat × Nat) → Best → Nat
  | _, [], _ => 0                                                -- empty stack: done
  | 0, _ :: _, _ => 0                                            -- out of fuel
  | f + 1, (v, r) :: rest, m =>                                  -- pop v with r left
      match m v with                                             -- decide at pop time
      | none => 1 + loads heads cap f (push heads v r ++ rest) (note m v r)  -- new: load it
      | some b =>                                                -- walked with b left:
          if b < r ∧ b < cap then                                -- walk again: load it again
            1 + loads heads cap f (push heads v r ++ rest) (note m v r)
          else loads heads cap f rest m                          -- skipped: no load

-- Three merges stacked, the shape of the Rust ladder test: each merge's heads
-- are [short, long], the short branch one commit, the long one three, both
-- on the merge below. The long head is last, so it is walked first, and the
-- short branch then reaches the merge below with more depth left.
def ladder : Nat → List Nat
  | 15 => [11, 14]                                               -- third merge
  | 11 => [10]                                                   -- its short branch
  | 14 => [13] | 13 => [12] | 12 => [10]                         -- its long branch
  | 10 => [6, 9]                                                 -- second merge
  | 6 => [5]                                                     -- its short branch
  | 9 => [8] | 8 => [7] | 7 => [5]                               -- its long branch
  | 5 => [1, 4]                                                  -- first merge
  | 1 => [0]                                                     -- its short branch
  | 4 => [3] | 3 => [2] | 2 => [0]                               -- its long branch
  | _ => []                                                      -- the create, 0

-- Promise: at a negative depth the ladder's 16 commits are listed, each once,
-- and each block is loaded once. The walk starts with 2^63 - 1 left, the
-- least a negative depth gives (`depth: -2^63`).
theorem ladder_loads_once :
    (run ladder unlimited 100 [(15, 9223372036854775807)] (fun _ => none)).map (·.1.length) = some 16 ∧
      loads ladder unlimited 100 [(15, 9223372036854775807)] (fun _ => none) = 16 := by
  decide                                                         -- run both

-- Promise (RED): without the guard (a cap above every depth), the walk lists
-- the same commits but loads blocks again each time a short branch reaches a
-- merge with more depth left: 34 loads for 16 commits, growing with the
-- square of the merges.
theorem no_guard_loads_again :
    (run ladder 18446744073709551616 100 [(15, 9223372036854775807)] (fun _ => none)).map Prod.fst =
        (run ladder unlimited 100 [(15, 9223372036854775807)] (fun _ => none)).map Prod.fst ∧
      loads ladder 18446744073709551616 100 [(15, 9223372036854775807)] (fun _ => none) = 34 := by
  decide                                                         -- run both

end CommitsDepthCast
