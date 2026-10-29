import Std

/-!
# ChunkSplit.Identity: `@chunk` settings are part of a collection's identity

The story, in plain words.

* Who acts: an author node parses a schema with `body: String @chunk(maxChars: 64)`,
  and a peer node later learns that schema from the author's definition blocks.
* What can happen: the author computes the collection's ID as a hash over the
  definition delta. The peer stores the definition it rebuilds from that delta.
* What must never happen: two schemas that cut text differently (other `maxChars`
  or `overlap`) share one ID, or a peer rebuilds a definition with no settings, or
  with settings other than the author's. Either way two nodes would hold "the same"
  collection and cut the same text into different chunks.

The hash and the CBOR bytes are the assumed boundary, as in `Cid/Determinism.lean`:
equal deltas give equal IDs, different deltas give different IDs. So the model's
identity is the delta itself.

Rust anchors:
* `crates/schema/src/cid.rs` `build_collection_delta`: sorts the settings by field
  name and puts them in the delta. Modeled by `build`.
* `crates/query/src/sdl_parse/chunk.rs` `synthesize_chunk_types`: records the
  settings before IDs are computed. The bug it replaced is modeled by `buggyId`.
* `crates/db/src/merge/merge_handler/definition.rs` `process_collection_definition_delta`:
  a peer reads the settings back from the delta. Modeled by `restore`.

Verify: `cd proofs/lean && lake build ChunkSplit`
-/

namespace ChunkSplit.Identity

open List

-- One `@chunk` field's settings. A field name is modeled by a number.
structure Setting where
  -- Which text field is chunked.
  field : Nat
  -- The most characters a chunk may hold.
  maxChars : Nat
  -- How many paragraphs a chunk repeats from the one before.
  overlap : Nat
  -- Two settings can be compared for equality.
  deriving DecidableEq, Repr

-- A collection definition delta, the thing the ID is a hash over.
structure Delta where
  -- Everything that isn't about chunks: name, priority, governance, ...
  base : Nat
  -- The parent's `@chunk` settings, sorted by field.
  chunked : List Setting
  -- For a generated chunk collection: its parent collection and field.
  chunkOf : Option (Nat × Nat)
  -- Two deltas can be compared for equality.
  deriving DecidableEq, Repr

-- The order the settings are sorted in: by field name.
def byField (a b : Setting) : Bool := decide (a.field ≤ b.field)

-- What `build_collection_delta` writes: the rest, the sorted settings, the parent.
def build (base : Nat) (settings : List Setting) (chunkOf : Option (Nat × Nat)) : Delta :=
  -- The settings go in sorted, so their order in the SDL can't matter.
  { base := base, chunked := settings.mergeSort byField, chunkOf := chunkOf }

-- The delta a collection without `@chunk` had before this change existed.
def legacy (base : Nat) : Delta :=
  -- Nothing about chunks: the Rust skips an empty list and an absent parent.
  { base := base, chunked := [], chunkOf := none }

-- What a peer reads back from a delta: the settings and the parent field.
def restore (d : Delta) : List Setting × Option (Nat × Nat) :=
  -- Both come straight out of the delta's fields.
  (d.chunked, d.chunkOf)

/-! ## Compatibility -/

/-- A collection without `@chunk` keeps exactly the delta, and so the ID, it had. -/
theorem no_chunks_keeps_its_identity (base : Nat) : build base [] none = legacy base := by
  -- Sorting an empty list gives an empty list, so both sides are the same record.
  simp [build, legacy]

/-! ## Settings reach the identity -/

/-- Two definitions with the same ID have the same settings and the same parent. -/
theorem same_identity_same_settings {b : Nat} {s s' : List Setting} {c c' : Option (Nat × Nat)}
    (h : build b s c = build b s' c') :
    s.mergeSort byField = s'.mergeSort byField ∧ c = c' := by
  -- Equal records have equal fields; read off the two that matter.
  simp only [build, Delta.mk.injEq] at h
  -- The first conjunct is the base, the other two are what we want.
  exact ⟨h.2.1, h.2.2⟩

/-- So a schema that cuts differently gets a different ID. -/
theorem different_settings_different_identity {b : Nat} {s s' : List Setting}
    {c : Option (Nat × Nat)} (h : s.mergeSort byField ≠ s'.mergeSort byField) :
    build b s c ≠ build b s' c := by
  -- If the IDs were equal, the sorted settings would be equal, which `h` denies.
  intro heq
  exact h (same_identity_same_settings heq).1

/-! ## The order fields appear in doesn't matter

Field names on one collection are distinct, so sorting by field is a strict sort,
and two strictly sorted lists holding the same settings are the same list. -/

/-- Two lists sorted strictly by field, holding the same settings, are equal. -/
theorem eq_of_field_sorted_perm :
    ∀ {l1 l2 : List Setting},
      Pairwise (fun a b => a.field < b.field) l1 →
      Pairwise (fun a b => a.field < b.field) l2 → l1 ~ l2 → l1 = l2 := by
  -- Walk down the first list.
  intro l1
  induction l1 with
  -- An empty list is only a permutation of an empty list.
  | nil => intro l2 _ _ hp; simpa using hp.symm.eq_nil
  | cons a t ih =>
    -- The second list can't be empty, since it holds `a`.
    intro l2 h1 h2 hp
    cases l2 with
    | nil => simp at hp
    | cons b s =>
      -- Each head is somewhere in the other list.
      have hmem_a : a ∈ b :: s := hp.mem_iff.mp (mem_cons_self a t)
      have hmem_b : b ∈ a :: t := hp.symm.mem_iff.mp (mem_cons_self b s)
      -- The two heads are the same setting.
      have hab : a = b := by
        rcases List.mem_cons.mp hmem_a with h | h
        -- Either `a` is the other head already,
        · exact h
        · rcases List.mem_cons.mp hmem_b with h' | h'
          -- or `b` is this head,
          · exact h'.symm
          -- or each sits below the other's head, which strict order rules out.
          · exfalso
            have hlt1 : b.field < a.field := (List.pairwise_cons.mp h2).1 a h
            have hlt2 : a.field < b.field := (List.pairwise_cons.mp h1).1 b h'
            exact absurd (Nat.lt_trans hlt1 hlt2) (Nat.lt_irrefl b.field)
      -- With equal heads, the tails are permutations of each other, so equal.
      subst hab
      have hts : t ~ s := (List.perm_cons a).mp hp
      rw [ih (List.pairwise_cons.mp h1).2 (List.pairwise_cons.mp h2).2 hts]

/-- Sorting settings whose fields are distinct gives a list sorted strictly by field. -/
theorem sorted_strictly {xs : List Setting} (hnodup : (xs.map Setting.field).Nodup) :
    Pairwise (fun a b => a.field < b.field) (xs.mergeSort byField) := by
  -- `byField` is transitive,
  have htrans : ∀ a b c : Setting, byField a b = true → byField b c = true → byField a c = true := by
    intro a b c hab hbc
    simp only [byField, decide_eq_true_eq] at *
    exact Nat.le_trans hab hbc
  -- and total, so the sort's output is ordered by `≤` on fields.
  have htotal : ∀ a b : Setting, (byField a b || byField b a) = true := by
    intro a b
    simp only [byField, Bool.or_eq_true, decide_eq_true_eq]
    exact Nat.le_total a.field b.field
  have hsorted := List.sorted_mergeSort (le := byField) htrans htotal xs
  -- The sort only reorders, so the fields stay distinct.
  have hperm : (xs.mergeSort byField) ~ xs := List.mergeSort_perm xs byField
  have hnodup' : ((xs.mergeSort byField).map Setting.field).Nodup :=
    (hperm.map Setting.field).nodup_iff.mpr hnodup
  -- Distinct fields in the sorted list, pair by pair.
  have hne : Pairwise (fun a b => a.field ≠ b.field) (xs.mergeSort byField) :=
    List.pairwise_map.mp hnodup'
  -- `≤` and `≠` together give `<`.
  have hle : Pairwise (fun a b => a.field ≤ b.field) (xs.mergeSort byField) := by
    refine hsorted.imp ?_
    intro a b h; simpa [byField, decide_eq_true_eq] using h
  refine (hle.and hne).imp ?_
  intro a b h
  exact Nat.lt_of_le_of_ne h.1 h.2

/-- The same settings written in any order give the same delta, so the same ID. -/
theorem field_order_does_not_change_identity {b : Nat} {s s' : List Setting}
    {c : Option (Nat × Nat)} (hnodup : (s.map Setting.field).Nodup) (hp : s ~ s') :
    build b s c = build b s' c := by
  -- The other order has distinct fields too.
  have hnodup' : (s'.map Setting.field).Nodup := (hp.map Setting.field).nodup_iff.mp hnodup
  -- Both sorted lists hold the same settings,
  have hsame : s.mergeSort byField ~ s'.mergeSort byField :=
    ((List.mergeSort_perm s byField).trans hp).trans (List.mergeSort_perm s' byField).symm
  -- and both are strictly sorted, so they are the same list.
  have heq := eq_of_field_sorted_perm (sorted_strictly hnodup) (sorted_strictly hnodup') hsame
  -- Then the two deltas agree field by field.
  simp [build, heq]

/-! ## A peer rebuilds what the author parsed -/

/-- A peer reads back exactly the settings and parent the author put in. -/
theorem peer_restores_the_settings (b : Nat) (s : List Setting) (c : Option (Nat × Nat)) :
    restore (build b s c) = (s.mergeSort byField, c) := by
  -- `restore` reads the two fields `build` wrote.
  rfl

/-- And it holds the same settings as the author, whatever order the author wrote. -/
theorem peer_holds_the_same_settings_as_the_author (b : Nat) (s : List Setting)
    (c : Option (Nat × Nat)) :
    (restore (build b s c)).1 ~ s := by
  -- What it reads back is the author's list, sorted.
  rw [peer_restores_the_settings]
  exact List.mergeSort_perm s byField

/-! ## The bug this replaces

Before, the settings were attached after the ID was computed, so the ID was taken
over a delta with no settings in it. -/

-- The ID the old code computed: over the delta as if there were no chunks.
def buggyId (base : Nat) (_settings : List Setting) : Delta :=
  -- The settings are ignored here and attached to the record afterwards.
  legacy base

/-- With the old order, `maxChars: 64` and `maxChars: 1600` get the same ID. -/
theorem buggy_identity_ignores_settings :
    ∃ s s' : List Setting, s ≠ s' ∧ buggyId 0 s = buggyId 0 s' := by
  -- Two schemas that differ only in `maxChars`.
  refine ⟨[⟨0, 64, 0⟩], [⟨0, 1600, 0⟩], ?_, rfl⟩
  -- They are different settings, yet `rfl` above shows the same ID.
  decide

/-- The fixed code gives those two schemas different IDs. -/
theorem fixed_identity_tells_them_apart :
    build 0 [⟨0, 64, 0⟩] none ≠ build 0 [⟨0, 1600, 0⟩] none := by
  -- Equal IDs would mean equal sorted settings.
  intro h
  have hs := (same_identity_same_settings h).1
  -- Sorting one setting leaves it alone, and 64 is not 1600.
  simp [List.mergeSort_singleton] at hs

end ChunkSplit.Identity
