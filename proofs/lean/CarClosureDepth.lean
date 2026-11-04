/-!
# CarClosureDepth: a depth-cut CAR walk serves exactly the blocks within the cut

The story, in plain words. A receiver asks a provider for the blocks below a
missing frontier. The provider cannot see what the receiver holds, so a walk
of the whole closure sends back history the receiver already has. The
receiver therefore names a cut: the provider walks at most that many links
below each wanted block. When blocks are still missing, the receiver asks
again with twice the cut, up to a cap.

What is promised:
- a cut of zero serves the wanted blocks alone (`within_zero`);
- a wanted block is always served (`roots_served`);
- the walk serves a block exactly when some path of at most the cut leads to
  it from a wanted block (`within_iff_level`), so a held block deeper than
  the cut on every path is never sent again (`deep_block_not_served`);
- a wider cut serves everything a narrower one did, so every cut is part of
  the whole closure the provider already authorized (`within_mono`,
  `within_le`);
- the doubling schedule never shrinks, never passes the cap
  (`cut_le_cap`, `cut_mono`), and reaches any depth up to the cap
  (`cut_reaches`).

A DAG is a function naming each block's links. Blocks are numbers.

Assumed, not modelled: `within` has no visited set and no response cap. The
Rust walk matches `within_iff_level` only because it is breadth-first, with the
visited check on pop, so a block is first taken at its shortest distance from a
wanted block; `depth_cut_keeps_blocks_within_their_shortest_distance_of_a_root`
(`crates/p2p/tests/unit/car_depth.rs`) guards that order. The `CAR_MAX_BLOCKS`
and byte caps can serve fewer blocks than the cut allows. `cut_ge_pow` carries
the logarithmic round bound.

Rust anchors: `collect_dag_blocks_within` (`crates/p2p/src/sync/car.rs`),
`ClosureDepth::initial` and `ClosureDepth::widen`
(`crates/p2p/src/sync/coordinator/closure_depth.rs`).
-/
namespace CarClosureDepth

-- Each block names the blocks it links to.
abbrev Dag := Nat → List Nat

-- The blocks exactly `k` links below the wanted blocks, along some path.
def level (g : Dag) : Nat → List Nat → List Nat
  -- Zero links below: the wanted blocks themselves.
  | 0, roots => roots
  -- One more link: the same distance below the wanted blocks' links.
  | k + 1, roots => level g k (roots.flatMap g)

-- What a walk cut at `d` serves: the wanted blocks, then the walk cut at
-- `d - 1` from their links.
def within (g : Dag) : Nat → List Nat → List Nat
  -- A cut of zero stops at the wanted blocks.
  | 0, roots => roots
  -- A wider cut serves the wanted blocks and walks on from their links.
  | d + 1, roots => roots ++ within g d (roots.flatMap g)

-- A cut of zero is the exact request: the wanted blocks, nothing else.
theorem within_zero (g : Dag) (roots : List Nat) : within g 0 roots = roots := by
  -- The first equation of `within`.
  rfl

-- A wanted block is served whatever the cut.
theorem roots_served (g : Dag) (d : Nat) (roots : List Nat) (x : Nat)
    (h : x ∈ roots) : x ∈ within g d roots := by
  -- Both equations of `within` start with the wanted blocks.
  cases d with
  | zero => exact h
  | succ d => exact List.mem_append_left _ h

-- A block is served exactly when some path of at most `d` links leads to it.
theorem within_iff_level (g : Dag) (d : Nat) :
    ∀ (roots : List Nat) (x : Nat),
      x ∈ within g d roots ↔ ∃ k, k ≤ d ∧ x ∈ level g k roots := by
  -- By the cut, for every set of wanted blocks at once.
  induction d with
  | zero =>
    intro roots x
    constructor
    · -- Served by a zero cut means wanted, which is level zero.
      intro h
      exact ⟨0, Nat.le_refl 0, h⟩
    · -- A level at most zero is level zero, the wanted blocks.
      intro ⟨k, hk, hx⟩
      have hk0 : k = 0 := by omega
      subst hk0
      exact hx
  | succ d ih =>
    intro roots x
    -- Served is: wanted, or served by the narrower cut from their links.
    simp only [within, List.mem_append]
    constructor
    · intro h
      cases h with
      | inl hr =>
        -- A wanted block is at level zero.
        exact ⟨0, Nat.zero_le _, hr⟩
      | inr hw =>
        -- One level below the links is one level more below the wanted.
        obtain ⟨k, hk, hx⟩ := (ih _ x).mp hw
        exact ⟨k + 1, by omega, hx⟩
    · intro ⟨k, hk, hx⟩
      cases k with
      | zero =>
        -- Level zero is the wanted blocks.
        exact Or.inl hx
      | succ k =>
        -- Level k + 1 is level k below the links, inside the narrower cut.
        exact Or.inr ((ih _ x).mpr ⟨k, by omega, hx⟩)

-- A block that no path of at most `d` links reaches is never served: held
-- history deeper than the cut is not sent again.
theorem deep_block_not_served (g : Dag) (d : Nat) (roots : List Nat) (x : Nat)
    (deep : ∀ k, k ≤ d → x ∉ level g k roots) : x ∉ within g d roots := by
  -- Being served would give a short path, which `deep` rules out.
  intro h
  obtain ⟨k, hk, hx⟩ := (within_iff_level g d roots x).mp h
  exact deep k hk hx

-- Widening the cut by one keeps every block the narrower cut served.
theorem within_mono (g : Dag) (d : Nat) (roots : List Nat) (x : Nat)
    (h : x ∈ within g d roots) : x ∈ within g (d + 1) roots := by
  -- The short path that served it is still within the wider cut.
  obtain ⟨k, hk, hx⟩ := (within_iff_level g d roots x).mp h
  exact (within_iff_level g (d + 1) roots x).mpr ⟨k, by omega, hx⟩

-- Any wider cut serves a superset, so each cut stays inside the whole
-- closure the provider would have served and authorized before.
theorem within_le (g : Dag) (d e : Nat) (roots : List Nat) (x : Nat)
    (hde : d ≤ e) (h : x ∈ within g d roots) : x ∈ within g e roots := by
  -- The same short path fits under the wider cut.
  obtain ⟨k, hk, hx⟩ := (within_iff_level g d roots x).mp h
  exact (within_iff_level g e roots x).mpr ⟨k, by omega, hx⟩

-- The cut after `n` widenings: start at `init`, double, never past `cap`.
def cut (init cap : Nat) : Nat → Nat
  -- No widening yet: the first cut.
  | 0 => init
  -- One more widening: twice the cut, held at the cap.
  | n + 1 => min (2 * cut init cap n) cap

-- The cut never passes the cap.
theorem cut_le_cap (init cap n : Nat) (h : init ≤ cap) : cut init cap n ≤ cap := by
  -- The first cut is below the cap, and every widening takes a minimum with it.
  cases n with
  | zero => exact h
  | succ n => exact Nat.min_le_right _ _

-- Widening never narrows the cut.
theorem cut_mono (init cap n : Nat) (h : init ≤ cap) :
    cut init cap n ≤ cut init cap (n + 1) := by
  -- The cut is at most the cap, so doubling it and capping is no smaller.
  have hc := cut_le_cap init cap n h
  show cut init cap n ≤ min (2 * cut init cap n) cap
  omega

-- After `n` widenings the cut is at least `2 ^ n`, or the cap if smaller.
theorem cut_ge_pow (init cap n : Nat) (h1 : 1 ≤ init) (h : init ≤ cap) :
    min (2 ^ n) cap ≤ cut init cap n := by
  -- Each widening doubles both sides until the cap holds them.
  induction n with
  | zero =>
    show min 1 cap ≤ init
    omega
  | succ n ih =>
    show min (2 ^ (n + 1)) cap ≤ min (2 * cut init cap n) cap
    rw [Nat.pow_succ]
    omega

-- Every number is below two to its power.
theorem lt_two_pow (n : Nat) : n < 2 ^ n := by
  -- Adding one to the number at most doubles the power, which is at least one.
  induction n with
  | zero => decide
  | succ n ih =>
    rw [Nat.pow_succ]
    omega

-- Any depth up to the cap is reached after that many widenings at most, so
-- a receiver missing a deep DAG still gets all of it.
theorem cut_reaches (init cap depth : Nat) (h1 : 1 ≤ init) (h : init ≤ cap)
    (hd : depth ≤ cap) : depth ≤ cut init cap depth := by
  -- `depth` is below `2 ^ depth`, and both are within the cut's lower bound.
  have hpow : depth < 2 ^ depth := lt_two_pow depth
  have hge := cut_ge_pow init cap depth h1 h
  omega

end CarClosureDepth
