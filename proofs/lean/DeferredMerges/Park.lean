import DeferredMerges.Release

/-! What deferring and parking a composite do.

The story: governance defers a composite under the keys it awaits, and the
ACP gate parks a refused composite under its document and under the refused
composite. The index files it where there is room. A parked composite is
filed only if its document has room, since its document is what releases
it.

What must never happen: filing past a bound; filing one composite dropping
another one, or another one's place under a key; a parked composite, with
room for it, missing from either of its two keys; a parked composite that
was released by one of its keys still sitting under the other; a parked
composite its document has no room for holding a place in the index.
Rust owner: crates/database/db/src/merge/governance/deferred.rs DeferredMerges::defer,
DeferredMerges::park; crates/database/db/src/merge/merge_handler/composite.rs
prepare_composite_merge, which parks under `WaitKey::acp_object` and
`WaitKey::Composite` (`Deferral::Parked`). -/
namespace DeferredMerges

/-- Taking the entry of a composite that has none changes nothing. -/
theorem takeEntry_of_not_mem {s : State} {c : Cid} (h : c ∉ cids s) :
    takeEntry c s = (none, s) := by
  -- `lookup` finds nothing.
  unfold takeEntry; rw [lookup_of_not_mem h]

/-- In a consistent index, a composite without an entry is under no key. -/
theorem not_waiting_of_not_mem {L : Limits} {s : State} (h : Inv L s) {c : Cid}
    (hc : c ∉ cids s) (k : Key) : c ∉ s.waiters k := fun hm => by
  -- Under a key means an entry.
  obtain ⟨e, he, rfl, _⟩ := (h.cons k c).mp hm
  exact hc (List.mem_map.mpr ⟨e, he, rfl⟩)

/-- With the index full, a new composite is not indexed and nothing changes. -/
theorem defer_when_full_changes_nothing (L : Limits) {s : State} {c : Cid} (ks : List Key)
    (hc : c ∉ cids s) (hfull : L.capacity ≤ s.entries.length) : defer L c ks s = s := by
  -- Nothing to take, and the index is full.
  unfold defer; dsimp only; rw [takeEntry_of_not_mem hc]; dsimp only
  split
  · rfl
  · first | rfl | rw [if_pos hfull]

/-- A key with no room does not take a new composite, and keeps everything
it held. -/
theorem defer_full_key_unchanged {L : Limits} {s : State} {c : Cid}
    (ks : List Key) (hc : c ∉ cids s) {k : Key} (hfull : L.perKey ≤ (s.waiters k).length) :
    (defer L c ks s).waiters k = s.waiters k := by
  -- Nothing to take; follow the branches of `defer`.
  unfold defer; dsimp only; rw [takeEntry_of_not_mem hc]; dsimp only
  split; · rfl
  split; · rfl
  -- What filing did: `k` had no room, so it was not filed under.
  obtain ⟨hw, hroom, _⟩ := file_spec L c (ks.take L.perComposite) s.waiters
  split; · rfl
  dsimp only
  rw [hw k, if_neg (fun hm => absurd (hroom k hm).2 (Nat.not_lt.mpr hfull))]

/-- Deferring a composite never drops another composite's entry. -/
theorem defer_keeps_others {L : Limits} {s : State} (c : Cid) (ks : List Key) {e : Entry}
    (he : e ∈ s.entries) (hne : e.cid ≠ c) : e ∈ (defer L c ks s).entries := by
  -- Taking `c`'s old entry keeps every other entry.
  have h1 : e ∈ (takeEntry c s).2.entries := by
    rw [takeEntry_entries, List.mem_filter]; exact ⟨he, by simpa using hne⟩
  -- Every branch keeps that state's entries.
  unfold defer; dsimp only
  generalize (takeEntry c s).2 = t at h1
  split; · exact h1
  split; · exact h1
  split; · exact h1
  exact List.mem_append_left _ h1

/-- Deferring a composite never changes where another composite is filed. -/
theorem defer_keeps_other_waiters {L : Limits} {s : State} (h : Inv L s) (c : Cid)
    (ks : List Key) {d : Cid} (hne : d ≠ c) (k : Key) :
    d ∈ s.waiters k ↔ d ∈ (defer L c ks s).waiters k := by
  -- Taking `c`'s old entry only takes `c` out of keys.
  have h1 : d ∈ s.waiters k ↔ d ∈ (takeEntry c s).2.waiters k := by
    rw [takeEntry_waiters h.nodup (h.cons k), List.mem_filter]
    simp [hne]
  rw [h1]
  -- Filing only adds `c` to keys.
  unfold defer; dsimp only
  generalize (takeEntry c s).2 = t
  split; · rfl
  split; · rfl
  obtain ⟨hw, _, _⟩ := file_spec L c (ks.take L.perComposite) t.waiters
  split; · rfl
  dsimp only
  rw [hw k]; split
  · simp [hne]
  · rfl

/-- A composite not yet indexed, parked under a document with room, is
parked as a defer under its two keys. -/
theorem park_of_room {L : Limits} {s : State} {c : Cid} {doc : Nat} {r : Cid}
    (hc : c ∉ cids s) (hd : (s.waiters (.acpObject doc)).length < L.perKey) :
    park L c doc r s = defer L c [.acpObject doc, .composite r] s := by
  -- Nothing to take, so the document's room is read in `s`: it has some.
  unfold park; dsimp only; rw [takeEntry_of_not_mem hc]; dsimp only; rw [if_pos hd]

/-- A composite not yet indexed, parked under a full document, leaves the
index as it was. -/
theorem park_of_full {L : Limits} {s : State} {c : Cid} {doc : Nat} {r : Cid}
    (hc : c ∉ cids s) (hd : L.perKey ≤ (s.waiters (.acpObject doc)).length) :
    park L c doc r s = s := by
  -- Nothing to take, so the document's room is read in `s`: it has none.
  unfold park; dsimp only; rw [takeEntry_of_not_mem hc]; dsimp only
  rw [if_neg (Nat.not_lt.mpr hd)]

/-- A parked composite, with room for it, is indexed under its document and
under the refused composite. -/
theorem park_files_under_both {L : Limits} {s : State} (h : Inv L s) {c : Cid} {doc : Nat}
    {r : Cid} (hc : c ∉ cids s) (hcap : s.entries.length < L.capacity)
    (hper : 2 ≤ L.perComposite) (hd : (s.waiters (.acpObject doc)).length < L.perKey)
    (hr : (s.waiters (.composite r)).length < L.perKey) :
    (∃ e ∈ (park L c doc r s).entries, e.cid = c ∧ Key.acpObject doc ∈ e.keys ∧
        Key.composite r ∈ e.keys) ∧
      c ∈ (park L c doc r s).waiters (.acpObject doc) ∧
      c ∈ (park L c doc r s).waiters (.composite r) := by
  -- Both keys fit within the keys a composite may have.
  have ht : List.take L.perComposite [Key.acpObject doc, .composite r] =
      [.acpObject doc, .composite r] := List.take_of_length_le (by simp; omega)
  -- Both keys have room and do not hold `c`, so filing takes both.
  have hfd := file_takes_room L c [.acpObject doc, .composite r] s.waiters (.acpObject doc)
    (by simp) hd (not_waiting_of_not_mem h hc _)
  have hfr := file_takes_room L c [.acpObject doc, .composite r] s.waiters (.composite r)
    (by simp) hr (not_waiting_of_not_mem h hc _)
  obtain ⟨hw, _, _⟩ := file_spec L c [.acpObject doc, .composite r] s.waiters
  -- The document has room, so parking is a defer.
  rw [park_of_room hc hd]
  -- Follow `defer`: nothing to take, keys given, room in the index, filed.
  unfold defer; dsimp only; rw [takeEntry_of_not_mem hc]; dsimp only
  rw [if_neg (by simp), if_neg (Nat.not_le.mpr hcap), ht, if_neg (List.ne_nil_of_mem hfd)]
  refine ⟨⟨⟨c, _⟩, List.mem_append_right _ (List.mem_singleton.mpr rfl), rfl, hfd, hfr⟩, ?_, ?_⟩
  · dsimp only; rw [hw, if_pos hfd]; simp
  · dsimp only; rw [hw, if_pos hfr]; simp

/-- Past its document's bound, a parked composite is not indexed at all:
the index is exactly as it was, so the composite takes no place in it and
sits under no key, the refused composite's included (its own CID, when it
is the refused one). It stays unmerged, and the replication layer's own
retry tries it again. -/
theorem park_past_document_bound {L : Limits} {s : State} (h : Inv L s) {c : Cid} {doc : Nat}
    {r : Cid} (hc : c ∉ cids s) (hd : L.perKey ≤ (s.waiters (.acpObject doc)).length) :
    park L c doc r s = s ∧ c ∉ cids (park L c doc r s) ∧
      ∀ k, c ∉ (park L c doc r s).waiters k := by
  -- Parking changed nothing.
  rw [park_of_full hc hd]
  -- `c` has no entry, so, the index being consistent, it is under no key.
  exact ⟨rfl, hc, not_waiting_of_not_mem h hc⟩

/-- A parked composite released by either of its keys, its document's change
or the refused composite's merge, leaves the index and both keys. -/
theorem parked_released_by_either {L : Limits} {s : State} (h : Inv L s) {c : Cid}
    {doc : Nat} {r : Cid} (hc : c ∉ cids s) (hcap : s.entries.length < L.capacity)
    (hper : 2 ≤ L.perComposite) (hd : (s.waiters (.acpObject doc)).length < L.perKey)
    (hr : (s.waiters (.composite r)).length < L.perKey) (k : Key)
    (hk : k = .acpObject doc ∨ k = .composite r) :
    c ∉ cids (release (park L c doc r s) [k]) ∧
      c ∉ (release (park L c doc r s) [k]).waiters (.acpObject doc) ∧
      c ∉ (release (park L c doc r s) [k]).waiters (.composite r) := by
  -- After parking, everything still holds and `c` is under both keys.
  have hp := inv_park h c doc r
  obtain ⟨_, hfd, hfr⟩ := park_files_under_both h hc hcap hper hd hr
  -- So `c` is under the released key.
  have hck : c ∈ (park L c doc r s).waiters k := by rcases hk with rfl | rfl <;> assumption
  have hm : k ∈ [k] := List.mem_singleton.mpr rfl
  exact ⟨release_takes hp hm hck, release_leaves_no_trace hp hm hck _,
    release_leaves_no_trace hp hm hck _⟩

/-- The bounds the model is checked with are the production ones; the
conformance test checks them against the Rust constants. -/
theorem production_bounds :
    production.capacity = 16384 ∧ production.perKey = 1024 ∧ production.perComposite = 64 :=
  -- They are written that way.
  ⟨rfl, rfl, rfl⟩

end DeferredMerges
