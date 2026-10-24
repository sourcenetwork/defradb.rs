/-!
# ServiceRecord.Providers: who a fetch may ask, from the book of records

The story, in plain words. When our node is missing part of a collection's
history, it looks in its book of service records for the nodes that say they
hold the collection. It walks the matches in the order the book gives them,
skips itself and any node it already took, sorts what is left so replicas
come before archives and archives before readers, and keeps at most a few.

What must never happen:
* our own node is in the list (`providers_exclude_self`);
* a node is in the list twice (`providers_nodup`);
* a node is in the list that the book did not name (`providers_from_book`);
* the list is longer than its cap (`providers_within_cap`);
* a node comes before another with a better role (`providers_sorted`).

A role is modelled as its rank: 0 for a replica, 1 for an archive, 2 for a
reader. Nodes are numbers.

Rust anchors: `Records::providers` and `rank` in
`crates/p2p/src/iroh/records/task.rs`.
-/

namespace ServiceRecord

-- A node the book named, with the rank of the role it holds the collection in.
abbrev Cand := Nat × Nat

-- Take candidate `c` unless it is us or a node already taken.
def keep (me : Nat) (acc : List Cand) (c : Cand) : List Cand :=
  -- A new node that is not us goes on the end; anything else is skipped.
  if c.1 ≠ me ∧ c.1 ∉ acc.map Prod.fst then acc ++ [c] else acc

-- Walk the book's matches in order, keeping each new node that is not us.
def collect (me : Nat) (cands : List Cand) : List Cand :=
  -- Start from nothing and keep candidates one at a time.
  cands.foldl (keep me) []

-- The order the list is sorted in: the lower rank, the better role, first.
def byRank (a b : Cand) : Bool :=
  -- `a` may come before `b` when its rank is no worse.
  decide (a.2 ≤ b.2)

-- The providers a lookup returns: collected, sorted by role, at most `cap`.
def providers (me cap : Nat) (cands : List Cand) : List Cand :=
  -- Sort what was collected, then keep the first `cap`.
  ((collect me cands).mergeSort byRank).take cap

-- What every kept list satisfies: no node twice, and never us.
def Good (me : Nat) (acc : List Cand) : Prop :=
  -- The nodes are distinct, and none of them is us.
  (acc.map Prod.fst).Nodup ∧ ∀ c ∈ acc, c.1 ≠ me

-- Promise: keeping one more candidate keeps a good list good.
theorem keep_good (me : Nat) (acc : List Cand) (c : Cand)
    -- Given a good list,
    (h : Good me acc) :
    -- keeping `c` leaves it good.
    Good me (keep me acc c) := by
  -- Unfold keeping, and split on whether `c` is taken.
  unfold keep
  -- Both cases of "is `c` new and not us?".
  split
  · -- Taken: `c` is new and not us.
    rename_i hc
    -- Split the list's good parts and the new candidate's facts.
    obtain ⟨hnd, hme⟩ := h
    obtain ⟨hne, hnew⟩ := hc
    -- The new list is good on both counts.
    refine ⟨?_, ?_⟩
    · -- Its nodes are the old distinct ones plus one that is not among them.
      simp only [List.map_append, List.map_cons, List.map_nil]
      -- Distinct old nodes, a new node, and the new one is not an old one.
      exact List.pairwise_append.mpr ⟨hnd, List.pairwise_singleton _ _, by
        -- Any old node differs from the new one, since the new one is not old.
        intro a ha b hb
        -- `b` is the new node itself.
        simp at hb
        -- So `a = b` would make the new node an old one.
        subst hb
        -- Which the check ruled out.
        exact fun hab => hnew (hab ▸ ha)⟩
    · -- No node is us: the old ones were not, and neither is `c`.
      intro x hx
      -- `x` is old or it is `c`.
      simp only [List.mem_append, List.mem_singleton] at hx
      -- Each case was covered.
      rcases hx with hx | hx
      · exact hme x hx
      · exact hx ▸ hne
  · -- Skipped: the list is unchanged, so still good.
    exact h

-- Promise: walking any candidates from a good list ends in a good list.
theorem foldl_good (me : Nat) (cands : List Cand) :
    -- For every good starting list,
    ∀ acc, Good me acc →
    -- the walk ends good.
    Good me (cands.foldl (keep me) acc) := by
  -- By induction on the candidates still to walk.
  induction cands with
  -- Nothing to walk: the list is the starting one.
  | nil => intro acc h; exact h
  -- One candidate, then the rest: keeping it stays good, then the rest do too.
  | cons c cs ih => intro acc h; exact ih _ (keep_good me acc c h)

-- Promise: everything a walk ends with was in the starting list or the walk.
theorem foldl_from (me : Nat) (cands : List Cand) :
    -- For every starting list and every candidate in the result,
    ∀ acc x, x ∈ cands.foldl (keep me) acc →
    -- it came from the start or from the candidates walked.
    x ∈ acc ∨ x ∈ cands := by
  -- By induction on the candidates still to walk.
  induction cands with
  -- Nothing walked: the result is the start.
  | nil => intro acc x hx; exact Or.inl hx
  -- One candidate, then the rest.
  | cons c cs ih =>
    intro acc x hx
    -- The rest's walk started from `keep acc c`.
    rcases ih _ x hx with hk | hcs
    · -- From `keep acc c`: either from `acc`, or it is `c`.
      unfold keep at hk
      -- Split on whether `c` was taken.
      split at hk
      · simp only [List.mem_append, List.mem_singleton] at hk
        rcases hk with h | h
        · exact Or.inl h
        · exact Or.inr (h ▸ List.mem_cons_self _ _)
      · exact Or.inl hk
    · -- From the rest of the walk.
      exact Or.inr (List.mem_cons_of_mem _ hcs)

-- Promise: the collected list is good.
theorem collect_good (me : Nat) (cands : List Cand) :
    -- No node twice, and never us.
    Good me (collect me cands) :=
  -- The walk starts from the empty list, which is good.
  foldl_good me cands [] ⟨List.nodup_nil, fun _ h => by simp at h⟩

-- Promise: our own node is never a provider.
theorem providers_exclude_self (me cap : Nat) (cands : List Cand) :
    -- Every provider's node differs from ours.
    ∀ c ∈ providers me cap cands, c.1 ≠ me := by
  -- Take a provider.
  intro c hc
  -- It is among the first `cap` of the sorted list,
  have hs := List.mem_of_mem_take hc
  -- so among the sorted list, which holds the collected list's members.
  have hm := (List.mergeSort_perm (collect me cands) byRank).mem_iff.mp hs
  -- The collected list never holds us.
  exact (collect_good me cands).2 c hm

-- Promise: no node is a provider twice.
theorem providers_nodup (me cap : Nat) (cands : List Cand) :
    -- The providers' nodes are distinct.
    ((providers me cap cands).map Prod.fst).Nodup := by
  -- Sorting only reorders the collected nodes, which are distinct,
  have hsorted : (((collect me cands).mergeSort byRank).map Prod.fst).Nodup :=
    ((List.mergeSort_perm (collect me cands) byRank).map Prod.fst).nodup_iff.mpr
      (collect_good me cands).1
  -- and keeping a prefix keeps them distinct.
  exact ((List.take_sublist _ _).map Prod.fst).nodup hsorted

-- Promise: every provider is a node the book named.
theorem providers_from_book (me cap : Nat) (cands : List Cand) :
    -- Every provider is one of the candidates.
    ∀ c ∈ providers me cap cands, c ∈ cands := by
  -- Take a provider.
  intro c hc
  -- It is in the sorted list, so in the collected list,
  have hm := (List.mergeSort_perm (collect me cands) byRank).mem_iff.mp
    (List.mem_of_mem_take hc)
  -- and the collected list holds only candidates (it started empty).
  rcases foldl_from me cands [] c hm with h | h
  · simp at h
  · exact h

-- Promise: a lookup returns at most `cap` providers.
theorem providers_within_cap (me cap : Nat) (cands : List Cand) :
    -- The list is no longer than the cap.
    (providers me cap cands).length ≤ cap :=
  -- Keeping the first `cap` keeps at most `cap`.
  List.length_take_le _ _

-- Promise: providers come replicas first, then archives, then readers.
theorem providers_sorted (me cap : Nat) (cands : List Cand) :
    -- Every provider's rank is no worse than any provider after it.
    (providers me cap cands).Pairwise (fun a b => byRank a b = true) := by
  -- The sorted list is in rank order: rank order chains and any two ranks compare,
  have hsorted := List.sorted_mergeSort (le := byRank)
    (fun a b c hab hbc => by simp [byRank] at *; omega)
    (fun a b => by simp only [byRank, Bool.or_eq_true, decide_eq_true_eq]; omega)
    (collect me cands)
  -- and a prefix of an ordered list is ordered.
  exact hsorted.sublist (List.take_sublist _ _)

end ServiceRecord
