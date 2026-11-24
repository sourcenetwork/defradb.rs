import DeferredMerges.Invariant

/-! What releasing does, and what the sweep's release of parked composites
does.

The story: a key arrives (a composite merged, a field value appeared) or a
document's relationships changed on this node, and the node releases that
key: everything filed under it is handed back to be tried again. The bounded
sweep releases up to `n` document keys at once, for grants made elsewhere.

What must never happen: a released composite left behind under another of
its keys (it would dangle there); a composite handed back that the released
keys did not hold; a composite filed under none of the released keys
touched; releasing twice doing anything more than releasing once; the
sweep handing back a composite that waits only for governance (another
composite or a field value).
Rust owner: crates/database/db/src/merge/governance/deferred.rs DeferredMerges::release,
DeferredMerges::release_parked. -/
namespace DeferredMerges

/-! ## Facts that need nothing -/

/-- Handing back one composite never adds an entry. -/
theorem releaseOne_shrinks (s : State) (c : Cid) (e : Entry) (h : e ∈ (releaseOne s c).entries) :
    e ∈ s.entries := by
  -- It drops the entries with CID `c` and nothing else.
  rw [(releaseOne_entries s c).1, takeEntry_entries, List.mem_filter] at h; exact h.1

/-- Releasing one key never adds an entry. -/
theorem releaseKey_shrinks (s : State) (k : Key) (e : Entry) (h : e ∈ (releaseKey s k).entries) :
    e ∈ s.entries := by
  -- Removing the key keeps the entries; each hand-back only drops some.
  unfold releaseKey at h
  -- Any start state and any list of composites handed back.
  generalize s.waiters k = l at h
  have : ∀ (l : List Cid) (t : State), e ∈ (l.foldl releaseOne t).entries → e ∈ t.entries := by
    intro l; induction l with
    | nil => intro t h; exact h
    | cons c l ih => intro t h; exact releaseOne_shrinks t c e (ih _ h)
  exact this l { s with waiters := update s.waiters k [] } h

/-- Releasing never adds an entry. -/
theorem release_shrinks (s : State) (ks : List Key) (e : Entry)
    (h : e ∈ (release s ks).entries) : e ∈ s.entries := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => exact h
  | cons k ks ih => exact releaseKey_shrinks s k e (ih _ h)

/-- A composite with no entry gets none by releasing. -/
theorem cids_release_shrinks (s : State) (ks : List Key) (c : Cid)
    (h : c ∈ cids (release s ks)) : c ∈ cids s := by
  -- Its entry after would be an entry before.
  obtain ⟨e, he, rfl⟩ := List.mem_map.mp h
  exact List.mem_map.mpr ⟨e, release_shrinks s ks e he, rfl⟩

/-- A key holding nobody still holds nobody after releasing any key. -/
theorem releaseKey_keeps_nil (s : State) (k k' : Key) (h : s.waiters k' = []) :
    (releaseKey s k).waiters k' = [] := by
  -- Removing `k` leaves `k'` empty, and each hand-back keeps it so.
  unfold releaseKey
  have h0 : ({ s with waiters := update s.waiters k [] } : State).waiters k' = [] := by
    simp only [update_apply]; split <;> simp_all
  generalize s.waiters k = l
  generalize ({ s with waiters := update s.waiters k [] } : State) = t at h0
  induction l generalizing t with
  | nil => exact h0
  | cons c l ih =>
    apply ih
    rw [(releaseOne_entries t c).2]; exact takeEntry_waiters_nil c t k' h0

/-- A released key holds nobody afterwards. -/
theorem releaseKey_clears (s : State) (k : Key) : (releaseKey s k).waiters k = [] := by
  -- The key is emptied first, and each hand-back keeps it empty.
  unfold releaseKey
  have h0 : ({ s with waiters := update s.waiters k [] } : State).waiters k = [] := by
    simp [update_apply]
  generalize s.waiters k = l
  generalize ({ s with waiters := update s.waiters k [] } : State) = t at h0
  induction l generalizing t with
  | nil => exact h0
  | cons c l ih =>
    apply ih
    rw [(releaseOne_entries t c).2]; exact takeEntry_waiters_nil c t k h0

/-- After a release, every released key holds nobody. -/
theorem release_clears (s : State) (ks : List Key) : ∀ k ∈ ks, (release s ks).waiters k = [] := by
  -- One key at a time: the first is cleared and stays clear.
  induction ks generalizing s with
  | nil => intro k hk; cases hk
  | cons k0 ks ih =>
    intro k hk
    -- A key of the rest is cleared by the rest.
    by_cases hr : k ∈ ks
    · exact ih _ k hr
    · -- It is the first key; the rest keeps it clear.
      have he : k = k0 := by simpa [hr] using hk
      subst he
      show (release (releaseKey s k) ks).waiters k = []
      have hc := releaseKey_clears s k
      generalize releaseKey s k = t at hc
      clear ih hk
      induction ks generalizing t with
      | nil => exact hc
      | cons k1 ks ih2 =>
        exact ih2 (fun h => hr (List.mem_cons_of_mem _ h)) _ (releaseKey_keeps_nil t k1 k hc)

/-! ## Rows that hold someone

The waiters are rows; a key left with nobody has its row removed. That is
what makes removing a key that already holds nobody no change at all. -/

/-- Every row holds someone. -/
def Waiters.WF (w : Waiters) : Prop :=
  -- No row with an empty list.
  ∀ r ∈ w.rows, r.2 ≠ []

/-- Changing a key keeps every row holding someone. -/
theorem wf_update {w : Waiters} (h : w.WF) (k : Key) (v : List Cid) : (update w k v).WF := by
  intro r hr
  unfold update at hr
  rcases List.mem_append.mp hr with hr | hr
  · -- The new row exists only when it holds someone.
    split at hr
    · cases hr
    · rename_i hv; rw [List.mem_singleton] at hr; rw [hr]; exact hv
  · -- The other rows were there before.
    exact h r (List.mem_filter.mp hr).1

/-- When every row holds someone, a key holding nobody has no row. -/
theorem no_row_of_empty (rows : List (Key × List Cid)) (h : ∀ r ∈ rows, r.2 ≠ []) (k : Key)
    (hk : rowsGet rows k = []) : ∀ r ∈ rows, (r.1 != k) = true := by
  -- One row at a time.
  induction rows with
  | nil => intro r hr; cases hr
  | cons r0 rows ih =>
    obtain ⟨k0, v⟩ := r0
    by_cases he : k = k0
    · -- The first row would answer for `k`, and it holds someone.
      subst he; simp [rowsGet] at hk; exact absurd hk (h _ (List.mem_cons_self _ _))
    · -- It is another key's row; the rest has none for `k` either.
      simp only [rowsGet, he, if_false] at hk
      intro r hr
      rcases List.mem_cons.mp hr with rfl | hr
      · simp only [bne_iff_ne, ne_eq]; exact fun e => he e.symm
      · exact ih (fun r hr => h r (List.mem_cons_of_mem _ hr)) hk r hr

/-- Removing a key that holds nobody changes nothing. -/
theorem update_nil_of_empty {w : Waiters} (h : w.WF) {k : Key} (hk : w k = []) :
    update w k [] = w := by
  -- No row is `k`'s, so dropping its rows drops nothing.
  have hn := no_row_of_empty w.rows h k hk
  cases w with
  | mk rows =>
    unfold update; simp only [if_true, List.nil_append]
    rw [List.filter_eq_self.mpr hn]

/-- Unfiling keeps every row holding someone. -/
theorem wf_unindex (c : Cid) (ks : List Key) {w : Waiters} (h : w.WF) : (unindex c ks w).WF := by
  -- Each step is a change of one key.
  induction ks generalizing w with
  | nil => exact h
  | cons k ks ih => exact ih (wf_update h _ _)

/-- Filing keeps every row holding someone. -/
theorem wf_file (L : Limits) (c : Cid) (ks : List Key) {w : Waiters} (h : w.WF) :
    (file L c ks w).2.WF := by
  -- Each step is a change of one key, or none.
  induction ks generalizing w with
  | nil => exact h
  | cons k ks ih => unfold file; split; exact ih (wf_update h _ _); exact ih h

/-- Taking an entry keeps every row holding someone. -/
theorem wf_takeEntry (c : Cid) {s : State} (h : s.waiters.WF) : (takeEntry c s).2.waiters.WF := by
  -- Nothing changes, or it is an unfiling.
  unfold takeEntry; split; exact h; exact wf_unindex _ _ h

/-- Defer keeps every row holding someone. -/
theorem wf_defer (L : Limits) (c : Cid) (ks : List Key) {s : State} (h : s.waiters.WF) :
    (defer L c ks s).waiters.WF := by
  -- Taking the old entry, then filing.
  unfold defer; dsimp only
  have h1 := wf_takeEntry c h
  generalize (takeEntry c s).2 = t at h1 ⊢
  split; · exact h1
  split; · exact h1
  split; · exact h1
  exact wf_file _ _ _ h1

/-- Releasing keeps every row holding someone. -/
theorem wf_release {s : State} (h : s.waiters.WF) (ks : List Key) : (release s ks).waiters.WF := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => exact h
  | cons k ks ih =>
    apply ih
    -- Removing the key, then handing back, each keeps the rows holding someone.
    unfold releaseKey
    have h0 : ({ s with waiters := update s.waiters k [] } : State).waiters.WF := wf_update h _ _
    generalize s.waiters k = l
    generalize ({ s with waiters := update s.waiters k [] } : State) = t at h0
    induction l generalizing t with
    | nil => exact h0
    | cons c l ih2 => exact ih2 _ (by rw [(releaseOne_entries t c).2]; exact wf_takeEntry c h0)

/-- After any operations from the empty index, every row holds someone. -/
theorem wf_run (L : Limits) (ops : List Op) : (run L empty ops).waiters.WF := by
  -- The empty index has no rows; each operation keeps them holding someone.
  suffices ∀ s : State, s.waiters.WF → (run L s ops).waiters.WF from
    this empty (fun _ hr => by cases hr)
  induction ops with
  | nil => intro s h; exact h
  | cons op ops ih =>
    intro s h; apply ih
    cases op with
    | defer c ks => exact wf_defer L c ks h
    | park c doc r =>
      -- A defer, or only taking `c`'s old entry.
      show (park L c doc r s).waiters.WF
      unfold park; dsimp only
      split
      · exact wf_defer L c _ h
      · exact wf_takeEntry c h
    | release ks => exact wf_release h ks
    | releaseParked n => exact wf_release h _
    | enqueueReady c =>
      show (enqueueReady c s).2.waiters.WF
      unfold enqueueReady; split; exact h; exact wf_takeEntry c h
    | takeReady =>
      show (match takeReady s with | some (_, s) => s | none => s).waiters.WF
      cases ht : takeReady s with
      | none => exact h
      | some p =>
        obtain ⟨c, t⟩ := p
        unfold takeReady at ht; split at ht
        · cases ht
        · cases ht; exact h
    | forget c => exact wf_takeEntry c h

/-- Releasing keys that hold nobody changes nothing. -/
theorem release_of_clear (s : State) (hw : s.waiters.WF) (ks : List Key)
    (h : ∀ k ∈ ks, s.waiters k = []) : release s ks = s := by
  -- One key at a time.
  induction ks with
  | nil => rfl
  | cons k ks ih =>
    -- The first key holds nobody: removing it and handing back nothing is no change.
    have hk : releaseKey s k = s := by
      unfold releaseKey
      rw [h k (List.mem_cons_self _ _), update_nil_of_empty hw (h k (List.mem_cons_self _ _))]
      rfl
    show release (releaseKey s k) ks = s
    rw [hk]; exact ih (fun k' hk' => h k' (List.mem_cons_of_mem _ hk'))

/-- Releasing twice is releasing once. -/
theorem release_idempotent (s : State) (hw : s.waiters.WF) (ks : List Key) :
    release (release s ks) ks = release s ks :=
  -- After the first release the keys hold nobody.
  release_of_clear _ (wf_release hw ks) ks (release_clears s ks)

/-! ## Facts that need the index to be consistent -/

/-- In a consistent index, an entry's composite is under exactly the keys the
entry lists. -/
theorem mem_waiters_iff {L : Limits} {s : State} (h : Inv L s) {e : Entry} (he : e ∈ s.entries)
    (k : Key) : e.cid ∈ s.waiters k ↔ k ∈ e.keys := by
  -- Consistency, and only one entry per composite.
  rw [h.cons k e.cid]
  constructor
  · rintro ⟨e', he', hc, hk⟩
    rw [entry_unique h.nodup he' he hc] at hk; exact hk
  · intro hk; exact ⟨e, he, rfl, hk⟩

/-- A composite a released key held has no entry afterwards. -/
theorem release_takes {L : Limits} {s : State} (h : Inv L s) {ks : List Key} {k : Key}
    {c : Cid} (hk : k ∈ ks) (hc : c ∈ s.waiters k) : c ∉ cids (release s ks) := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => cases hk
  | cons k0 ks ih =>
    -- What releasing the first key does.
    obtain ⟨h1, h2, _⟩ := releaseKey_spec h k0
    show c ∉ cids (release (releaseKey s k0) ks)
    by_cases hc1 : c ∈ cids (releaseKey s k0)
    · -- Still indexed after the first key: its entry is the one from before.
      obtain ⟨e, he, hec⟩ := List.mem_map.mp hc1
      have he0 := (h2 e).mp he
      rcases List.mem_cons.mp hk with hk0 | hk'
      · -- The first key held it, so it was handed back: impossible.
        subst hk0; exact absurd (hec ▸ hc) he0.2
      · -- Its entry still lists `k`, so `k` still holds it; the rest takes it.
        obtain ⟨e', he', hc', hke⟩ := (h.cons k c).mp hc
        have heq : e = e' := entry_unique h.nodup he0.1 he' (hec.trans hc'.symm)
        subst heq
        exact ih h1 hk' ((h1.cons k c).mpr ⟨e, he, hec, hke⟩)
    · -- Already gone; releasing never brings it back.
      exact fun hm => hc1 (cids_release_shrinks _ ks c hm)

/-- A composite a released key held is under no key afterwards: released by
any of its keys, it leaves all of them. -/
theorem release_leaves_no_trace {L : Limits} {s : State} (h : Inv L s) {ks : List Key}
    {k : Key} {c : Cid} (hk : k ∈ ks) (hc : c ∈ s.waiters k) (k' : Key) :
    c ∉ (release s ks).waiters k' := by
  -- Under a key means an entry, which it no longer has.
  intro hm
  obtain ⟨e, he, rfl, _⟩ := ((inv_release h ks).cons k' c).mp hm
  exact release_takes h hk hc (List.mem_map.mpr ⟨e, he, rfl⟩)

/-- An entry listing none of the released keys stays. -/
theorem release_keeps {L : Limits} {s : State} (h : Inv L s) {ks : List Key} {e : Entry}
    (he : e ∈ s.entries) (hk : ∀ k ∈ e.keys, k ∉ ks) : e ∈ (release s ks).entries := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => exact he
  | cons k0 ks ih =>
    -- What releasing the first key does.
    obtain ⟨h1, h2, _⟩ := releaseKey_spec h k0
    -- The first key does not hold it, so it stays; then the rest.
    refine ih h1 ((h2 e).mpr ⟨he, fun hm => ?_⟩)
      (fun k hk' hm => hk k hk' (List.mem_cons_of_mem _ hm))
    exact hk k0 ((mem_waiters_iff h he k0).mp hm) (List.mem_cons_self _ _)

/-- An entry listing none of the released keys keeps every place it was
filed. -/
theorem release_keeps_waiting {L : Limits} {s : State} (h : Inv L s) {ks : List Key}
    {e : Entry} (he : e ∈ s.entries) (hk : ∀ k ∈ e.keys, k ∉ ks) (k : Key) :
    e.cid ∈ s.waiters k ↔ e.cid ∈ (release s ks).waiters k := by
  -- Both sides say the entry lists `k`.
  rw [mem_waiters_iff h he, mem_waiters_iff (inv_release h ks) (release_keeps h he hk)]

/-- A release hands back only composites the released keys held. -/
theorem release_hands_back {L : Limits} {s : State} (h : Inv L s) {ks : List Key} {c : Cid}
    (hc : c ∈ (release s ks).ready) :
    c ∈ s.ready ∨ ∃ k ∈ ks, ∃ e ∈ s.entries, e.cid = c ∧ k ∈ e.keys := by
  -- One key at a time.
  induction ks generalizing s with
  | nil => exact Or.inl hc
  | cons k0 ks ih =>
    -- What releasing the first key does.
    obtain ⟨h1, h2, h3⟩ := releaseKey_spec h k0
    rcases ih h1 hc with hc | ⟨k, hk, e, he, hec, hke⟩
    · -- Handed back by the first key, or before.
      rcases h3 c hc with hc | hc
      · exact Or.inl hc
      · obtain ⟨e, he, hec, hke⟩ := (h.cons k0 c).mp hc
        exact Or.inr ⟨k0, List.mem_cons_self _ _, e, he, hec, hke⟩
    · -- Handed back by the rest: its entry was there before.
      exact Or.inr ⟨k, List.mem_cons_of_mem _ hk, e, ((h2 e).mp he).1, hec, hke⟩

/-! ## The sweep's release of parked composites -/

/-- The sweep reports how many documents it released, at most `n`. -/
theorem releaseParked_count (n : Nat) (s : State) :
    (releaseParked n s).1 = (pickParked n s).length ∧ (releaseParked n s).1 ≤ n :=
  -- It counts the keys it picked, of which there are at most `n`.
  ⟨rfl, List.length_take_le _ _⟩

/-- The sweep picks only document keys. -/
theorem picked_are_documents (n : Nat) (s : State) (k : Key) (h : k ∈ pickParked n s) :
    k.isAcp = true :=
  -- They pass the document filter.
  (List.mem_filter.mp (List.mem_of_mem_take h)).2

/-- The sweep picks only keys that hold someone. -/
theorem picked_hold_someone {L : Limits} {s : State} (h : Inv L s) (n : Nat) (k : Key)
    (hk : k ∈ pickParked n s) : s.waiters k ≠ [] :=
  -- They are present keys.
  (present_iff h k).mp (List.mem_filter.mp (List.mem_of_mem_take hk)).1

/-- The sweep hands back only composites parked under a document. -/
theorem release_parked_hands_back_only_parked {L : Limits} {s : State} (h : Inv L s) (n : Nat)
    {c : Cid} (hc : c ∈ (releaseParked n s).2.ready) :
    c ∈ s.ready ∨ ∃ e ∈ s.entries, e.cid = c ∧ ∃ k ∈ e.keys, k.isAcp = true := by
  -- What a release hands back, with the keys picked being documents.
  rcases release_hands_back h hc with hc | ⟨k, hk, e, he, hec, hke⟩
  · exact Or.inl hc
  · exact Or.inr ⟨e, he, hec, k, hke, picked_are_documents n s k hk⟩

/-- The sweep leaves a governance waiter, one filed under no document,
exactly where it was. -/
theorem release_parked_keeps_governance {L : Limits} {s : State} (h : Inv L s) (n : Nat)
    {e : Entry} (he : e ∈ s.entries) (hg : ∀ k ∈ e.keys, k.isAcp = false) :
    e ∈ (releaseParked n s).2.entries ∧
      ∀ k, (e.cid ∈ s.waiters k ↔ e.cid ∈ (releaseParked n s).2.waiters k) := by
  -- None of its keys is a document, so none was picked.
  have hk : ∀ k ∈ e.keys, k ∉ pickParked n s := fun k hk hp => by
    have := picked_are_documents n s k hp; rw [hg k hk] at this; cases this
  exact ⟨release_keeps h he hk, release_keeps_waiting h he hk⟩

/-- A sweep that may pick every parked document leaves nothing parked under
a document. -/
theorem release_parked_drains {L : Limits} {s : State} (h : Inv L s) (n : Nat)
    (hn : ((presentKeys s).filter Key.isAcp).length ≤ n) :
    ∀ e ∈ (releaseParked n s).2.entries, ∀ k ∈ e.keys, k.isAcp = false := by
  intro e he k hk
  -- Suppose `k` were a document.
  cases hd : k.isAcp
  · rfl
  · exfalso
    -- The entry was there before, so `k` was present, so it was picked.
    have he0 := release_shrinks s _ e he
    have hp : k ∈ pickParked n s := by
      unfold pickParked; rw [List.take_of_length_le hn]
      exact List.mem_filter.mpr ⟨(mem_dedup k _).mpr (List.mem_flatMap.mpr ⟨e, he0, hk⟩), hd⟩
    -- A picked key's composites have no entry afterwards.
    exact release_takes h hp ((mem_waiters_iff h he0 k).mpr hk) (List.mem_map.mpr ⟨e, he, rfl⟩)

/-- After such a sweep, another one finds no document to release. -/
theorem release_parked_twice_finds_nothing {L : Limits} {s : State} (h : Inv L s) (n m : Nat)
    (hn : ((presentKeys s).filter Key.isAcp).length ≤ n) :
    (releaseParked m (releaseParked n s).2).1 = 0 := by
  -- No present key of the swept index is a document.
  have hd := release_parked_drains h n hn
  show (pickParked m (releaseParked n s).2).length = 0
  unfold pickParked
  rw [List.filter_eq_nil_iff.mpr ?_]
  · simp
  · intro k hk
    obtain ⟨e, he, hke⟩ := List.mem_flatMap.mp ((mem_dedup k _).mp hk)
    simp [hd e he k hke]

end DeferredMerges
