import DeferredMerges.Core

/-! What stays true of the index whatever happens to it.

The story: the index starts empty, and then any sequence of defers, parks,
releases, sweeps, direct queueing, takes and forgets happens. Whatever the
sequence, five things stay true (`Inv`):

* each composite has at most one entry;
* a key holds a composite exactly when that composite's entry lists the
  key (no dangling entry either way);
* no key holds more than `perKey` composites;
* no more than `capacity` composites are indexed;
* every entry lists at least one key.

The headline theorems are `inv_run` and `bounded_after_any_run`.
Rust owner: crates/database/db/src/merge/governance/deferred.rs DeferredMerges. -/
namespace DeferredMerges

/-- Key `k` holds exactly the composites whose entries list it. -/
def Cons (s : State) (k : Key) : Prop :=
  -- Held under `k` if and only if some entry of that composite lists `k`.
  ∀ c, c ∈ s.waiters k ↔ ∃ e ∈ s.entries, e.cid = c ∧ k ∈ e.keys

/-- What stays true of the index. -/
structure Inv (L : Limits) (s : State) : Prop where
  /-- Each composite has at most one entry. -/
  nodup : (cids s).Nodup
  /-- Every key holds exactly the composites whose entries list it. -/
  cons : ∀ k, Cons s k
  /-- No key holds more than `perKey` composites. -/
  perKey : ∀ k, (s.waiters k).length ≤ L.perKey
  /-- No more than `capacity` composites are indexed. -/
  capacity : s.entries.length ≤ L.capacity
  /-- Every entry lists at least one key. -/
  keyed : ∀ e ∈ s.entries, e.keys ≠ []

/-! ## Small facts about the pieces -/

/-- Reading rows after dropping key `k`'s: nobody under `k`, the rest as
before. -/
theorem rowsGet_filter (rows : List (Key × List Cid)) (k k' : Key) :
    rowsGet (rows.filter fun r => r.1 != k) k' = if k' = k then [] else rowsGet rows k' := by
  -- One row at a time.
  induction rows with
  | nil => simp [rowsGet]
  | cons r rows ih =>
    obtain ⟨k0, v⟩ := r
    -- Whether the row is `k`'s, and whether it answers for `k'`.
    by_cases h0 : k0 = k <;> by_cases h1 : k' = k0 <;>
      simp_all [rowsGet, List.filter_cons]

/-- Reading a key after changing one key. -/
@[simp] theorem update_apply (w : Waiters) (k k' : Key) (v : List Cid) :
    update w k v k' = if k' = k then v else w k' := by
  -- Read the new row, if any, then the other keys' rows.
  show rowsGet (update w k v).rows k' = if k' = k then v else rowsGet w.rows k'
  unfold update
  by_cases hv : v = []
  · -- No new row: the old one is gone.
    subst hv; simp only [if_true, List.nil_append, rowsGet_filter]
  · -- The new row answers for `k`.
    simp only [hv, if_false, List.singleton_append, rowsGet, rowsGet_filter]
    by_cases h : k' = k <;> simp [h]

/-- The empty waiters hold nobody. -/
@[simp] theorem empty_waiters (k : Key) : empty.waiters k = [] := rfl

/-- Unfiling `c` from `ks` takes it out of exactly the keys in `ks`. -/
theorem unindex_apply (c : Cid) (ks : List Key) (w : Waiters) (k : Key) :
    unindex c ks w k = if k ∈ ks then (w k).filter (· ≠ c) else w k := by
  -- One key of `ks` at a time, for every starting `w`.
  induction ks generalizing w with
  -- No key: nothing changes.
  | nil => simp [unindex]
  -- The first key, then the rest.
  | cons k0 ks ih =>
    -- Unfold one step and use the rest's answer.
    simp only [unindex, ih, update_apply, List.mem_cons]
    -- Whether `k` is the first key, and whether the rest holds it.
    by_cases h0 : k = k0 <;> by_cases h1 : k ∈ ks <;> simp [h0, h1, List.filter_filter]

/-- Two entries of a list without repeated CIDs that share a CID are the
same entry. -/
theorem entry_unique {l : List Entry} (h : (l.map (·.cid)).Nodup) {e e' : Entry}
    (he : e ∈ l) (he' : e' ∈ l) (hc : e.cid = e'.cid) : e = e' := by
  -- One entry of the list at a time.
  induction l with
  -- An empty list holds no entry.
  | nil => cases he
  -- The first entry, then the rest.
  | cons a t ih =>
    -- The first CID is not repeated in the rest, and the rest has no repeats.
    simp only [List.map_cons, List.nodup_cons, List.mem_map] at h
    -- Where each of the two entries is.
    rcases List.mem_cons.mp he with h1 | h1 <;> rcases List.mem_cons.mp he' with h2 | h2
    -- Both are the first: the same.
    · rw [h1, h2]
    -- One is the first and one in the rest: the first CID would repeat.
    · exact absurd ⟨e', h2, hc.symm.trans (congrArg Entry.cid h1)⟩ h.1
    -- The same, the other way round.
    · exact absurd ⟨e, h1, hc.trans (congrArg Entry.cid h2)⟩ h.1
    -- Both in the rest: by the rest.
    · exact ih h.2 h1 h2

/-- Without repeated CIDs, `lookup` finds exactly the entry with that CID. -/
theorem lookup_iff {s : State} (h : (cids s).Nodup) (c : Cid) (ks : List Key) :
    lookup s c = some ks ↔ ∃ e ∈ s.entries, e.cid = c ∧ e.keys = ks := by
  -- Unfold to the list of entries.
  unfold lookup; unfold cids at h
  -- Work on any list of entries.
  generalize s.entries = l at h ⊢
  -- One entry at a time.
  induction l with
  -- An empty list finds nothing and holds nothing.
  | nil => simp
  -- The first entry, then the rest.
  | cons a t ih =>
    -- The first CID is not repeated in the rest.
    simp only [List.map_cons, List.nodup_cons, List.mem_map] at h
    -- Whether the first entry is the one.
    by_cases ha : a.cid = c
    · -- It is: `lookup` returns its keys, and no other entry has its CID.
      simp only [List.find?_cons, ha, beq_self_eq_true, Option.map_some', Option.some.injEq,
        List.mem_cons, exists_eq_or_imp, true_and]
      -- Another entry with this CID would repeat it.
      constructor
      · exact Or.inl
      · rintro (h' | ⟨e, he, hc, _⟩)
        · exact h'
        · exact absurd ⟨e, he, hc.trans ha.symm⟩ h.1
    · -- It is not: `lookup` goes on to the rest.
      have hb : (a.cid == c) = false := by simpa using ha
      -- And the first entry is not a match.
      simp only [List.find?_cons, hb, List.mem_cons, exists_eq_or_imp, ha, false_and, false_or]
      -- The rest decides.
      exact ih h.2

/-- `lookup` finds nothing exactly when no entry has that CID. -/
theorem lookup_none_iff (s : State) (c : Cid) :
    lookup s c = none ↔ ∀ e ∈ s.entries, e.cid ≠ c := by
  -- Nothing found means no entry matches.
  simp [lookup, List.find?_eq_none]

/-- A composite with no entry: `lookup` finds nothing. -/
theorem lookup_of_not_mem {s : State} {c : Cid} (h : c ∉ cids s) : lookup s c = none := by
  -- No entry has its CID.
  rw [lookup_none_iff]
  -- Any entry with it would put it among the CIDs.
  intro e he hc; exact h (List.mem_map.mpr ⟨e, he, hc⟩)

/-! ## Taking an entry -/

/-- Taking `c`'s entry drops exactly the entries with CID `c`. -/
theorem takeEntry_entries (c : Cid) (s : State) :
    (takeEntry c s).2.entries = s.entries.filter (·.cid ≠ c) := by
  -- Whether `c` was indexed.
  unfold takeEntry; split
  · -- Not indexed: no entry has CID `c`, so dropping them changes nothing.
    rename_i hn
    -- The filter keeps everything.
    exact (List.filter_eq_self.mpr fun e he => by simpa using (lookup_none_iff s c).mp hn e he).symm
  · -- Indexed: its entries are dropped.
    rfl

/-- Taking an entry leaves the queue alone. -/
theorem takeEntry_ready (c : Cid) (s : State) : (takeEntry c s).2.ready = s.ready := by
  -- Neither branch touches it.
  unfold takeEntry; split <;> rfl

/-- Taking an entry leaves the queued set alone. -/
theorem takeEntry_queued (c : Cid) (s : State) : (takeEntry c s).2.queued = s.queued := by
  -- Neither branch touches it.
  unfold takeEntry; split <;> rfl

/-- A key holding nobody still holds nobody after taking an entry. -/
theorem takeEntry_waiters_nil (c : Cid) (s : State) (k : Key) (h : s.waiters k = []) :
    (takeEntry c s).2.waiters k = [] := by
  -- Whether `c` was indexed.
  unfold takeEntry; split
  -- Not indexed: unchanged.
  · exact h
  -- Indexed: the key is filtered, or unchanged; empty either way.
  · simp [unindex_apply, h]

/-- Where key `k` is consistent, taking `c`'s entry takes `c` out of `k` and
changes nothing else there. -/
theorem takeEntry_waiters {s : State} (hn : (cids s).Nodup) {k : Key} (hk : Cons s k)
    (c : Cid) : (takeEntry c s).2.waiters k = (s.waiters k).filter (· ≠ c) := by
  -- Whether `c` was indexed.
  unfold takeEntry; split
  · -- Not indexed: `k` cannot hold it, so filtering it out changes nothing.
    rename_i hl
    -- Any `c` under `k` would have an entry.
    refine (List.filter_eq_self.mpr fun d hd => ?_).symm
    -- Suppose `d` is `c`.
    simp only [ne_eq, decide_eq_true_eq]; rintro rfl
    -- Then it has an entry listing `k`, which `lookup` would have found.
    obtain ⟨e, he, hc, _⟩ := (hk d).mp hd
    exact (lookup_none_iff s d).mp hl e he hc
  · -- Indexed under keys `ks`.
    rename_i ks hl
    -- Unfiling filters exactly the keys in `ks`.
    simp only [unindex_apply]
    split
    -- `k` is one of them: filtered.
    · rfl
    · -- `k` is not: then `k` does not hold `c` either.
      rename_i hks
      refine (List.filter_eq_self.mpr fun d hd => ?_).symm
      -- Suppose `d` is `c`.
      simp only [ne_eq, decide_eq_true_eq]; rintro rfl
      -- Its entry lists `k`, and it is the entry `lookup` found, which does not.
      obtain ⟨e, he, hc, hke⟩ := (hk d).mp hd
      have := (lookup_iff hn d e.keys).mpr ⟨e, he, hc, rfl⟩
      rw [hl] at this; cases this; exact hks hke

/-- Taking an entry keeps everything that stays true. -/
theorem inv_takeEntry {L : Limits} {s : State} (h : Inv L s) (c : Cid) :
    Inv L (takeEntry c s).2 where
  -- The CIDs left are a part of the CIDs before.
  nodup := by
    rw [cids, takeEntry_entries]
    exact h.nodup.sublist ((List.filter_sublist _).map _)
  -- Each key lost `c` and every entry with CID `c` went.
  cons := fun k d => by
    rw [takeEntry_waiters h.nodup (h.cons k), takeEntry_entries, List.mem_filter, h.cons k d]
    simp only [List.mem_filter, ne_eq, decide_eq_true_eq]
    constructor
    · rintro ⟨⟨e, he, rfl, hk⟩, hne⟩; exact ⟨e, ⟨he, hne⟩, rfl, hk⟩
    · rintro ⟨e, ⟨he, hne⟩, rfl, hk⟩; exact ⟨⟨e, he, rfl, hk⟩, hne⟩
  -- Filtering never makes a key hold more.
  perKey := fun k => by
    rw [takeEntry_waiters h.nodup (h.cons k)]
    exact Nat.le_trans (List.length_filter_le _ _) (h.perKey k)
  -- Dropping entries never makes more.
  capacity := by
    rw [takeEntry_entries]; exact Nat.le_trans (List.length_filter_le _ _) h.capacity
  -- The entries left are entries from before.
  keyed := fun e he => by
    rw [takeEntry_entries, List.mem_filter] at he; exact h.keyed e he.1

/-! ## Filing -/

/-- What filing does: every key it files `c` under gains `c` at the end and
had room and did not hold `c`; every other key is unchanged; and it files
only under keys it was given. -/
theorem file_spec (L : Limits) (c : Cid) (ks : List Key) (w : Waiters) :
    (∀ k, (file L c ks w).2 k = if k ∈ (file L c ks w).1 then w k ++ [c] else w k) ∧
    (∀ k ∈ (file L c ks w).1, c ∉ w k ∧ (w k).length < L.perKey) ∧
    (∀ k ∈ (file L c ks w).1, k ∈ ks) := by
  -- One key at a time, for every starting `w`.
  induction ks generalizing w with
  -- No key: nothing filed, nothing changed.
  | nil => simp [file]
  -- The first key, then the rest.
  | cons k0 ks ih =>
    -- Whether the first key has room and does not hold `c`.
    by_cases hc : (w k0).length < L.perKey ∧ c ∉ w k0
    · -- It does: filed under it, then under the rest, from the new waiters.
      have hf : file L c (k0 :: ks) w = (k0 :: (file L c ks (update w k0 (w k0 ++ [c]))).1,
          (file L c ks (update w k0 (w k0 ++ [c]))).2) := by
        rw [file, if_pos hc]
      rw [hf]
      -- What the rest does, from the new waiters.
      obtain ⟨h1, h2, h3⟩ := ih (update w k0 (w k0 ++ [c]))
      -- Name the rest's result.
      generalize file L c ks (update w k0 (w k0 ++ [c])) = r at h1 h2 h3
      -- The rest never files under `k0` again: it holds `c` now.
      have hnot : k0 ∉ r.1 := fun hm => by
        have := (h2 k0 hm).1; simp [update_apply] at this
      refine ⟨fun k => ?_, fun k hk => ?_, fun k hk => ?_⟩
      · -- A key's final waiters: by the rest, then by the first change.
        show r.2 k = if k ∈ k0 :: r.1 then w k ++ [c] else w k
        rw [h1 k]; by_cases hk : k = k0
        · rw [hk]; simp [hnot, update_apply]
        · simp [hk, update_apply]
      · -- A key filed under: the first one had room; the rest's had room
        -- in the new waiters, which are the old ones for other keys.
        rcases List.mem_cons.mp hk with h | h
        · rw [h]; exact ⟨hc.2, hc.1⟩
        · have hne : k ≠ k0 := fun e => hnot (e ▸ h)
          have := h2 k h
          simp only [update_apply, hne, if_false] at this
          exact this
      · -- Every key filed under was given.
        rcases List.mem_cons.mp hk with h | h
        · exact List.mem_cons.mpr (Or.inl h)
        · exact List.mem_cons_of_mem _ (h3 k h)
    · -- It does not: skipped, and the rest decides.
      have hf : file L c (k0 :: ks) w = file L c ks w := by rw [file, if_neg hc]
      rw [hf]
      obtain ⟨h1, h2, h3⟩ := ih w
      exact ⟨h1, h2, fun k hk => List.mem_cons_of_mem _ (h3 k hk)⟩

/-- A key that has room and does not hold `c` is filed under. -/
theorem file_takes_room (L : Limits) (c : Cid) (ks : List Key) (w : Waiters) (k : Key)
    (hk : k ∈ ks) (hr : (w k).length < L.perKey) (hc : c ∉ w k) : k ∈ (file L c ks w).1 := by
  -- One key at a time, for every starting `w`.
  induction ks generalizing w with
  -- No key given: impossible.
  | nil => cases hk
  -- The first key, then the rest.
  | cons k0 ks ih =>
    -- Whether `k` is the first key.
    by_cases he : k = k0
    · -- It is: it has room and does not hold `c`, so it is filed under.
      subst he; simp [file, hr, hc]
    · -- It is not: it is in the rest, whose waiters for `k` are unchanged.
      have hk' : k ∈ ks := by simpa [he] using hk
      unfold file
      split
      · exact List.mem_cons_of_mem _ (ih _ hk' (by simpa [update_apply, he] using hr)
          (by simpa [update_apply, he] using hc))
      · exact ih _ hk' hr hc

/-! ## Each operation keeps what stays true -/

/-- The empty index satisfies everything. -/
theorem inv_empty (L : Limits) : Inv L empty where
  -- No entry, no key holding anyone.
  nodup := List.nodup_nil
  cons := fun _ _ => by simp [empty, Waiters.get, rowsGet]
  perKey := fun _ => by simp [empty, Waiters.get, rowsGet]
  capacity := by simp [empty]
  keyed := fun _ he => by cases he

/-- A composite just taken out has no entry. -/
theorem not_mem_takeEntry (c : Cid) (s : State) : c ∉ cids (takeEntry c s).2 := by
  -- Every entry left has another CID.
  simp [cids, takeEntry_entries]

/-- Defer keeps everything that stays true. -/
theorem inv_defer {L : Limits} {s : State} (h : Inv L s) (c : Cid) (ks : List Key) :
    Inv L (defer L c ks s) := by
  -- Follow the branches of `defer`.
  unfold defer
  dsimp only
  -- After taking `c`'s old entry, everything still holds, and `c` is nowhere.
  have h1 := inv_takeEntry h c
  have hc := not_mem_takeEntry c s
  -- Name that state.
  generalize (takeEntry c s).2 = t at h1 hc ⊢
  split; · exact h1
  split; · exact h1
  rename_i _ hcap
  split; · exact h1
  rename_i hne
  -- What filing did.
  obtain ⟨hw, hroom, _⟩ := file_spec L c (ks.take L.perComposite) t.waiters
  -- Name the filing's result.
  generalize file L c (ks.take L.perComposite) t.waiters = r at hw hroom hne
  -- `c` is under no key yet.
  have hcw : ∀ k, c ∉ t.waiters k := fun k hm => by
    obtain ⟨e, he, rfl, _⟩ := (h1.cons k _).mp hm
    exact hc (List.mem_map.mpr ⟨e, he, rfl⟩)
  constructor
  · -- One more CID, new to the list.
    simp only [cids, List.map_append, List.map_cons, List.map_nil]
    unfold List.Nodup
    rw [List.pairwise_append]
    refine ⟨h1.nodup, List.pairwise_singleton _ _, fun a ha b hb => ?_⟩
    simp only [List.mem_singleton] at hb; subst hb
    exact fun e => hc (e ▸ ha)
  · -- A key holds `c` exactly when `c`'s new entry lists it.
    intro k d
    dsimp only
    -- An entry listing `k` is an old one, or `c`'s new one.
    have key : (∃ e ∈ t.entries ++ [⟨c, r.1⟩], e.cid = d ∧ k ∈ e.keys) ↔
        (∃ e ∈ t.entries, e.cid = d ∧ k ∈ e.keys) ∨ (c = d ∧ k ∈ r.1) := by
      constructor
      · rintro ⟨e, he, hd, hk⟩
        rcases List.mem_append.mp he with he | he
        · exact Or.inl ⟨e, he, hd, hk⟩
        · rw [List.mem_singleton] at he; subst he; exact Or.inr ⟨hd, hk⟩
      · rintro (⟨e, he, hd, hk⟩ | ⟨hd, hk⟩)
        · exact ⟨e, List.mem_append_left _ he, hd, hk⟩
        · exact ⟨⟨c, r.1⟩, List.mem_append_right _ (List.mem_singleton.mpr rfl), hd, hk⟩
    -- The old entries are what `k` held before.
    rw [key, ← h1.cons k d, hw k]
    -- `k` gained `c` exactly when it was filed under.
    by_cases hk : k ∈ r.1
    · simp only [hk, if_true, List.mem_append, List.mem_singleton, and_true]
      exact or_congr Iff.rfl ⟨Eq.symm, Eq.symm⟩
    · simp only [hk, if_false, and_false, or_false]
  · -- A key filed under had room for one more.
    intro k
    dsimp only
    rw [hw k]
    split
    · rename_i hk; simp only [List.length_append, List.length_singleton]
      exact (hroom k hk).2
    · exact h1.perKey k
  · -- There was room for one more entry.
    simp only [List.length_append, List.length_singleton]
    exact Nat.lt_of_not_le hcap
  · -- The new entry lists the keys it was filed under, which are some.
    intro e he
    rcases List.mem_append.mp he with he | he
    · exact h1.keyed e he
    · simp only [List.mem_singleton] at he; subst he; exact hne

/-- Park keeps everything that stays true. -/
theorem inv_park {L : Limits} {s : State} (h : Inv L s) (c : Cid) (doc : Nat) (r : Cid) :
    Inv L (park L c doc r s) := by
  -- Parking is a defer, or only taking `c`'s old entry.
  unfold park; dsimp only
  split
  · exact inv_defer h c _
  · exact inv_takeEntry h c

/-- Handing back one composite changes the index exactly as taking its entry. -/
theorem releaseOne_entries (s : State) (c : Cid) :
    (releaseOne s c).entries = (takeEntry c s).2.entries ∧
    (releaseOne s c).waiters = (takeEntry c s).2.waiters := by
  -- Whichever branch, only the queue can change beside the entry taken.
  unfold releaseOne
  split
  · rename_i t ht; rw [ht]; exact ⟨rfl, rfl⟩
  · rename_i t _ ht; rw [ht]; split <;> exact ⟨rfl, rfl⟩

/-- What is handed back after one composite: what was, or that composite. -/
theorem releaseOne_ready (s : State) (c d : Cid) (h : d ∈ (releaseOne s c).ready) :
    d ∈ s.ready ∨ d = c := by
  -- Follow the branches; the queue gains `c` at most.
  unfold releaseOne at h
  split at h
  · rename_i t ht; have := takeEntry_ready c s; rw [ht] at this; exact Or.inl (this ▸ h)
  · rename_i t _ ht
    have := takeEntry_ready c s; rw [ht] at this
    split at h
    · exact Or.inl (this ▸ h)
    · simp only [List.mem_append, List.mem_singleton] at h
      rcases h with h | h; exact Or.inl (this ▸ h); exact Or.inr h

/-- Handing back one composite keeps everything that stays true. -/
theorem inv_releaseOne {L : Limits} {s : State} (h : Inv L s) (c : Cid) :
    Inv L (releaseOne s c) := by
  -- It is taking the entry, as far as the index goes.
  obtain ⟨he, hw⟩ := releaseOne_entries s c
  have := inv_takeEntry h c
  exact ⟨by rw [cids, he]; exact this.nodup, fun k d => by rw [he, hw]; exact this.cons k d,
    fun k => by rw [hw]; exact this.perKey k, by rw [he]; exact this.capacity,
    fun e hm => this.keyed e (he ▸ hm)⟩

/-- What stays true while one key is being released: everything, except that
the released key holds nobody, whatever entries still list it. -/
structure InvExcept (L : Limits) (k : Key) (s : State) : Prop where
  /-- Each composite has at most one entry. -/
  nodup : (cids s).Nodup
  /-- Every other key is consistent. -/
  cons : ∀ k', k' ≠ k → Cons s k'
  /-- The released key holds nobody. -/
  empty : s.waiters k = []
  /-- No key holds more than `perKey`. -/
  perKey : ∀ k', (s.waiters k').length ≤ L.perKey
  /-- No more than `capacity` entries. -/
  capacity : s.entries.length ≤ L.capacity
  /-- Every entry lists a key. -/
  keyed : ∀ e ∈ s.entries, e.keys ≠ []

/-- Handing back composites one after another while key `k` is being
released: what stays true still does, an entry stays exactly when its
composite was not handed back, and the queue gains only those composites. -/
theorem fold_releaseOne {L : Limits} {k : Key} (l : List Cid) :
    ∀ t : State, InvExcept L k t →
      InvExcept L k (l.foldl releaseOne t) ∧
      (∀ e, e ∈ (l.foldl releaseOne t).entries ↔ e ∈ t.entries ∧ e.cid ∉ l) ∧
      (∀ d ∈ (l.foldl releaseOne t).ready, d ∈ t.ready ∨ d ∈ l) := by
  -- One composite at a time.
  induction l with
  -- None: nothing changes.
  | nil => intro t ht; simp [ht]
  -- The first composite, then the rest.
  | cons c l ih =>
    intro t ht
    -- Handing back `c` changes the index as taking its entry does.
    obtain ⟨he, hw⟩ := releaseOne_entries t c
    -- What stays true holds after handing back `c`.
    have ht1 : InvExcept L k (releaseOne t c) := by
      refine ⟨?_, fun k' hk' d => ?_, ?_, fun k' => ?_, ?_, fun e hm => ?_⟩
      · rw [cids, he, takeEntry_entries]; exact ht.nodup.sublist ((List.filter_sublist _).map _)
      · rw [hw, he, takeEntry_waiters ht.nodup (ht.cons k' hk'), takeEntry_entries,
          List.mem_filter, ht.cons k' hk' d]
        simp only [List.mem_filter, ne_eq, decide_eq_true_eq]
        constructor
        · rintro ⟨⟨e, he, rfl, hk⟩, hne⟩; exact ⟨e, ⟨he, hne⟩, rfl, hk⟩
        · rintro ⟨e, ⟨he, hne⟩, rfl, hk⟩; exact ⟨⟨e, he, rfl, hk⟩, hne⟩
      · rw [hw]; exact takeEntry_waiters_nil c t k ht.empty
      · rw [hw]; by_cases hk' : k' = k
        · subst hk'; rw [takeEntry_waiters_nil c t k' ht.empty]; exact Nat.zero_le _
        · rw [takeEntry_waiters ht.nodup (ht.cons k' hk')]
          exact Nat.le_trans (List.length_filter_le _ _) (ht.perKey k')
      · rw [he, takeEntry_entries]; exact Nat.le_trans (List.length_filter_le _ _) ht.capacity
      · rw [he, takeEntry_entries, List.mem_filter] at hm; exact ht.keyed e hm.1
    -- The rest, from there.
    obtain ⟨h1, h2, h3⟩ := ih _ ht1
    refine ⟨h1, fun e => ?_, fun d hd => ?_⟩
    · -- An entry stays when neither `c` nor the rest handed it back.
      simp only [List.foldl_cons] at h2 ⊢
      rw [h2 e, he, takeEntry_entries, List.mem_filter, List.mem_cons]
      simp only [ne_eq, decide_eq_true_eq, not_or]
      constructor
      · rintro ⟨⟨hm, hc⟩, hl⟩; exact ⟨hm, hc, hl⟩
      · rintro ⟨hm, hc, hl⟩; exact ⟨⟨hm, hc⟩, hl⟩
    · -- What is handed back came from before, from `c`, or from the rest.
      simp only [List.foldl_cons] at h3 hd
      rcases h3 d hd with hd | hd
      · rcases releaseOne_ready t c d hd with hd | hd
        · exact Or.inl hd
        · exact Or.inr (List.mem_cons.mpr (Or.inl hd))
      · exact Or.inr (List.mem_cons_of_mem _ hd)

/-- Releasing one key: what stays true still does, an entry stays exactly
when the key did not hold its composite, and the queue gains only what the
key held. -/
theorem releaseKey_spec {L : Limits} {s : State} (h : Inv L s) (k : Key) :
    Inv L (releaseKey s k) ∧
    (∀ e, e ∈ (releaseKey s k).entries ↔ e ∈ s.entries ∧ e.cid ∉ s.waiters k) ∧
    (∀ d ∈ (releaseKey s k).ready, d ∈ s.ready ∨ d ∈ s.waiters k) := by
  -- The state once the key is removed: everything holds except at `k`.
  have h0 : InvExcept L k { s with waiters := update s.waiters k [] } := by
    refine ⟨h.nodup, fun k' hk' d => ?_, by simp [update_apply], fun k' => ?_, h.capacity,
      h.keyed⟩
    · simp only [update_apply, hk', if_false]; exact h.cons k' d
    · simp only [update_apply]; split
      · exact Nat.zero_le _
      · exact h.perKey k'
  -- Hand back what the key held.
  obtain ⟨h1, h2, h3⟩ := fold_releaseOne (s.waiters k) _ h0
  unfold releaseKey
  refine ⟨⟨h1.nodup, fun k' d => ?_, h1.perKey, h1.capacity, h1.keyed⟩, h2, h3⟩
  -- The other keys are consistent; `k` holds nobody and no entry lists it.
  by_cases hk : k' = k
  · subst hk
    rw [h1.empty]
    simp only [List.not_mem_nil, false_iff, not_exists, not_and]
    intro e he hc hke
    -- Such an entry would have been held by the key, so handed back.
    have := (h2 e).mp he
    exact this.2 ((h.cons k' e.cid).mpr ⟨e, this.1, rfl, hke⟩)
  · exact h1.cons k' hk d

/-- Release keeps everything that stays true. -/
theorem inv_release {L : Limits} {s : State} (h : Inv L s) (ks : List Key) :
    Inv L (release s ks) := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => exact h
  | cons k ks ih => exact ih (releaseKey_spec h k).1

/-- The sweep's release keeps everything that stays true. -/
theorem inv_releaseParked {L : Limits} {s : State} (h : Inv L s) (n : Nat) :
    Inv L (releaseParked n s).2 :=
  -- It is a release.
  inv_release h _

/-- Queueing directly keeps everything that stays true. -/
theorem inv_enqueueReady {L : Limits} {s : State} (h : Inv L s) (c : Cid) :
    Inv L (enqueueReady c s).2 := by
  -- Either nothing changes, or an entry is taken and the queue grows.
  unfold enqueueReady; split
  · exact h
  · have := inv_takeEntry h c
    exact ⟨this.nodup, this.cons, this.perKey, this.capacity, this.keyed⟩

/-- Taking from the queue keeps everything that stays true. -/
theorem inv_takeReady {L : Limits} {s : State} (h : Inv L s) :
    Inv L (match takeReady s with | some (_, s) => s | none => s) := by
  -- Either nothing is taken, or only the queue changes.
  cases ht : takeReady s with
  | none => exact h
  | some p =>
    obtain ⟨c, t⟩ := p
    unfold takeReady at ht
    split at ht
    · cases ht
    · cases ht; exact ⟨h.nodup, h.cons, h.perKey, h.capacity, h.keyed⟩

/-- Forgetting keeps everything that stays true. -/
theorem inv_forget {L : Limits} {s : State} (h : Inv L s) (c : Cid) :
    Inv L (forget c s).2 :=
  -- It is taking an entry.
  inv_takeEntry h c

/-- Every operation keeps everything that stays true. -/
theorem inv_step {L : Limits} {s : State} (h : Inv L s) (op : Op) : Inv L (step L s op) := by
  -- Each operation by its own theorem.
  cases op with
  | defer c ks => exact inv_defer h c ks
  | park c doc r => exact inv_park h c doc r
  | release ks => exact inv_release h ks
  | releaseParked n => exact inv_releaseParked h n
  | enqueueReady c => exact inv_enqueueReady h c
  | takeReady => exact inv_takeReady h
  | forget c => exact inv_forget h c

/-- Whatever operations happen, in whatever order, everything that stays true
does. -/
theorem inv_run {L : Limits} {s : State} (h : Inv L s) (ops : List Op) :
    Inv L (run L s ops) := by
  -- One operation at a time.
  induction ops generalizing s with
  | nil => exact h
  | cons op ops ih => exact ih (inv_step h op)

/-- After any operations from the empty index, no key holds more than
`perKey` composites and no more than `capacity` are indexed. -/
theorem bounded_after_any_run (L : Limits) (ops : List Op) :
    (∀ k, ((run L empty ops).waiters k).length ≤ L.perKey) ∧
    (run L empty ops).entries.length ≤ L.capacity :=
  -- The bounds are part of what stays true.
  ⟨(inv_run (inv_empty L) ops).perKey, (inv_run (inv_empty L) ops).capacity⟩

/-- After any operations from the empty index, no key names a composite
without an entry listing that key, and no entry lists a key that does not
hold its composite. -/
theorem no_dangling_after_any_run (L : Limits) (ops : List Op) (k : Key) :
    Cons (run L empty ops) k :=
  -- Consistency is part of what stays true.
  (inv_run (inv_empty L) ops).cons k

/-! ## The Rust `waiters` map's keys -/

/-- A key is in the de-duplicated list exactly when it is in the list. -/
theorem mem_dedup (k : Key) (l : List Key) : k ∈ dedup l ↔ k ∈ l := by
  -- One key at a time.
  induction l with
  | nil => simp [dedup]
  | cons a t ih =>
    -- Whether the first key is kept, the membership is the same.
    unfold dedup; dsimp only; split
    · rename_i ha; rw [ih, List.mem_cons]
      constructor
      · exact Or.inr
      · rintro (rfl | h); exact ih.mp ha; exact h
    · rw [List.mem_cons, ih, List.mem_cons]

/-- The keys `presentKeys` lists are exactly the keys holding someone: the
keys the Rust `waiters` map has, since it drops a key left with nobody. -/
theorem present_iff {L : Limits} {s : State} (h : Inv L s) (k : Key) :
    k ∈ presentKeys s ↔ s.waiters k ≠ [] := by
  -- Listed by some entry, which by consistency means held.
  rw [presentKeys, mem_dedup, List.mem_flatMap]
  constructor
  · rintro ⟨e, he, hk⟩ hnil
    have := (h.cons k e.cid).mpr ⟨e, he, rfl, hk⟩
    rw [hnil] at this; cases this
  · intro hne
    obtain ⟨d, hd⟩ := List.exists_mem_of_ne_nil _ hne
    obtain ⟨e, he, _, hk⟩ := (h.cons k d).mp hd
    exact ⟨e, he, hk⟩

/-- Something waits under some key exactly when something is indexed
(`has_waiters`). -/
theorem has_waiters_iff {L : Limits} {s : State} (h : Inv L s) :
    (∃ k, s.waiters k ≠ []) ↔ s.entries ≠ [] := by
  constructor
  · -- A held composite has an entry.
    rintro ⟨k, hk⟩ hnil
    obtain ⟨d, hd⟩ := List.exists_mem_of_ne_nil _ hk
    obtain ⟨e, he, _⟩ := (h.cons k d).mp hd
    rw [hnil] at he; cases he
  · -- An entry lists a key, which holds it.
    intro hne
    obtain ⟨e, he⟩ := List.exists_mem_of_ne_nil _ hne
    obtain ⟨k, hk⟩ := List.exists_mem_of_ne_nil _ (h.keyed e he)
    refine ⟨k, fun hnil => ?_⟩
    have := (h.cons k e.cid).mpr ⟨e, he, rfl, hk⟩
    rw [hnil] at this; cases this

end DeferredMerges
