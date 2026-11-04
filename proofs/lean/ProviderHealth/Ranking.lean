/-!
# ProviderHealth.Ranking: ranking only reorders, and cool-downs stay in range

The story, in plain words. Before a node fetches a DAG root it lines up the
peers it may ask. The peer that announced the root comes first. Each peer gets
a standing: healthy, cooling (it just refused fast) or demoted (its fetches
mostly fail). The node asks healthy peers first, then cooling ones, then
demoted ones, keeping the announced order inside each group. When the
announcing peer itself is moved back, the healthy group starts at a different
peer each root, so the load it sheds is spread out.

A cool-down starts at 2 seconds. A repeat while it is fresh doubles it, up to
30 seconds; a repeat after it went stale (ended more than one delay ago) starts
over. When the table of cool-downs is full, stale entries are forgotten.

What must never happen:
* ranking drops, adds or duplicates a peer, which would break the rule that
  every provider stays askable (`rank_perm`);
* an announcing peer that is healthy loses its first place
  (`rank_keeps_healthy_source_first`), or a node where every peer is healthy
  sees any other order than the announced one (`rank_all_healthy`);
* a cool-down falls outside 2..30 seconds (`next_delay_in_range`), keeps
  doubling once stale (`stale_starts_over`), or forgets a fresh entry when
  pruning (`forget_keeps_fresh`, `forget_drops_stale`).

Rust anchors: `ProviderHealth::rank`, `record_fast_failure`, `forget_stale` and
`Cooldown::is_stale` in `crates/p2p/src/sync/coordinator/provider_health.rs`.
Times are whole seconds here; Rust uses `Instant` and `Duration`, which the
rules below only compare and add.
-/

namespace ProviderHealth

-- How a peer stands for the next root.
inductive Standing where
  | healthy
  | cooling
  | demoted
  deriving DecidableEq

variable {α : Type}

-- The peers of one standing, in the order they were given.
def group (stand : α → Standing) (s : Standing) (l : List α) : List α :=
  l.filter (fun p => decide (stand p = s))

-- `rank`: healthy, then cooling, then demoted. The healthy group is rotated by
-- the root's turn only when the announcing peer was moved back.
def rank (stand : α → Standing) (moved : Bool) (turn : Nat) (l : List α) : List α :=
  -- The healthy peers, rotated when the announcer was moved back;
  (if moved then (group stand .healthy l).rotateLeft turn else group stand .healthy l)
  -- then the cooling ones, then the demoted ones.
  ++ group stand .cooling l ++ group stand .demoted l

-- Rotating a list only reorders it.
theorem rotateLeft_perm (l : List α) (n : Nat) : (l.rotateLeft n).Perm l := by
  -- Unfold the rotation into a drop followed by a take.
  unfold List.rotateLeft
  simp only []
  split
  -- A short list is not rotated.
  · exact List.Perm.refl _
  -- Otherwise it is `drop ++ take`, which is `take ++ drop` swapped.
  · exact List.perm_append_comm.trans (by rw [List.take_append_drop])

-- Splitting a list into its three standings only reorders it.
theorem groups_perm (stand : α → Standing) (l : List α) :
    (group stand .healthy l ++ group stand .cooling l ++ group stand .demoted l).Perm l := by
  -- Walk the list one peer at a time.
  induction l with
  -- No peers: nothing on either side.
  | nil => simp [group]
  | cons p rest ih =>
    -- Name the three groups of the rest of the list.
    simp only [group, List.filter_cons] at ih ⊢
    generalize List.filter (fun p => decide (stand p = Standing.healthy)) rest = H at ih ⊢
    generalize List.filter (fun p => decide (stand p = Standing.cooling)) rest = C at ih ⊢
    generalize List.filter (fun p => decide (stand p = Standing.demoted)) rest = D at ih ⊢
    -- Look at the new peer's standing: it joins exactly one group.
    cases stand p <;> simp only [reduceCtorEq, decide_true, decide_false, ite_true, ite_false]
    -- Healthy: it already leads.
    · exact ih.cons p
    -- Cooling: it moves past the healthy group.
    · have h := (List.perm_middle (l₁ := H) (a := p) (l₂ := C ++ D)).trans
        ((by simpa [List.append_assoc] using ih : (H ++ (C ++ D)).Perm rest).cons p)
      simpa [List.append_assoc] using h
    -- Demoted: it moves past the healthy and cooling groups.
    · exact (List.perm_middle (l₁ := H ++ C) (a := p) (l₂ := D)).trans (ih.cons p)

-- Promise: ranking keeps every peer exactly once; it only changes the order.
theorem rank_perm (stand : α → Standing) (moved : Bool) (turn : Nat) (l : List α) :
    (rank stand moved turn l).Perm l := by
  -- With or without the rotation, the healthy group is a reordering of itself.
  unfold rank
  split
  · exact ((List.Perm.append_right _ (rotateLeft_perm _ turn)).append_right _).trans
      (groups_perm stand l)
  · exact groups_perm stand l

-- Promise: a healthy announcing peer is not moved, so it is asked first.
theorem rank_keeps_healthy_source_first (stand : α → Standing) (turn : Nat)
    (source : α) (rest : List α)
    -- Given the announcer is healthy,
    (h : stand source = .healthy) :
    -- with no rotation (the announcer was not moved back), it leads.
    (rank stand false turn (source :: rest)).head? = some source := by
  -- The healthy group starts with the announcer.
  simp [rank, group, List.filter_cons, h]

-- Promise: when every peer is healthy, the order is the announced one.
theorem rank_all_healthy (stand : α → Standing) (turn : Nat) (l : List α)
    -- Given every peer is healthy,
    (h : ∀ p ∈ l, stand p = .healthy) :
    -- ranking without rotation returns the list unchanged.
    rank stand false turn l = l := by
  -- The healthy group is everyone; the other groups are empty.
  have hh : group stand .healthy l = l := List.filter_eq_self.mpr (by simpa using h)
  have hc : group stand .cooling l = [] :=
    List.filter_eq_nil_iff.mpr (by intro p hp; simp [h p hp])
  have hd : group stand .demoted l = [] :=
    List.filter_eq_nil_iff.mpr (by intro p hp; simp [h p hp])
  simp [rank, hh, hc, hd]

-- The first cool-down, in seconds (`COOLDOWN_BASE`).
def base : Nat := 2

-- The longest cool-down, in seconds (`COOLDOWN_MAX`).
def ceiling : Nat := 30

-- A remembered cool-down: when it ends and how long it was.
structure Cooldown where
  ends : Nat
  delay : Nat

-- `Cooldown::is_stale`: it ended more than one delay ago.
def stale (now : Nat) (c : Cooldown) : Bool := decide (c.ends + c.delay ≤ now)

-- `record_fast_failure`: the delay of the next cool-down.
def nextDelay (now : Nat) : Option Cooldown → Nat
  -- First fast failure: the base.
  | none => base
  -- A repeat: double while fresh, up to the ceiling; start over once stale.
  | some c => if stale now c then base else min (2 * c.delay) ceiling

-- Promise: every cool-down lasts between the base and the ceiling.
theorem next_delay_in_range (now : Nat) (prev : Option Cooldown)
    -- Given the remembered cool-down, if any, was in range,
    (h : ∀ c, prev = some c → base ≤ c.delay ∧ c.delay ≤ ceiling) :
    -- the next one is too.
    base ≤ nextDelay now prev ∧ nextDelay now prev ≤ ceiling := by
  -- Split on whether there was one and whether it went stale.
  cases prev with
  | none => simp [nextDelay, base, ceiling]
  | some c =>
    have hc := h c rfl
    simp only [nextDelay]
    split
    · simp [base, ceiling]
    · simp only [base, ceiling] at hc ⊢
      omega

-- Promise: a repeat after the cool-down went stale starts over at the base.
theorem stale_starts_over (now : Nat) (c : Cooldown) (h : stale now c = true) :
    nextDelay now (some c) = base := by
  -- The stale branch is taken.
  simp [nextDelay, h]

-- `forget_stale`: keep only the cool-downs that are still fresh.
def forget (now : Nat) (table : List (α × Cooldown)) : List (α × Cooldown) :=
  table.filter (fun e => !stale now e.2)

-- Promise: pruning never forgets a fresh cool-down.
theorem forget_keeps_fresh (now : Nat) (table : List (α × Cooldown)) (e : α × Cooldown)
    -- Given the entry is in the table and fresh,
    (hm : e ∈ table) (hf : stale now e.2 = false) :
    -- it is still there after pruning.
    e ∈ forget now table := by
  -- Membership in a filter is membership plus the test.
  simp [forget, hm, hf]

-- Promise: pruning leaves no stale cool-down behind.
theorem forget_drops_stale (now : Nat) (table : List (α × Cooldown)) (e : α × Cooldown)
    -- Given the entry survived pruning,
    (hm : e ∈ forget now table) :
    -- it is fresh.
    stale now e.2 = false := by
  -- The filter only lets fresh entries through.
  simp [forget] at hm
  simpa using hm.2

end ProviderHealth
