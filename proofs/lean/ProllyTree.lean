/-!
# ProllyTree: one collection's documents in a tree that forgets its history

The story, in plain words. A node keeps, for one collection, a list of entries:
a document ID (the key) and that document's current heads (the value). The list
is kept sorted by key, with each key at most once. To let two nodes compare
their lists cheaply, the list is cut into chunks, each chunk gets a hash, the
list of chunk hashes is cut into chunks again, and so on until one chunk is
left: the root. Two nodes with the same entries must get the same root, however
their writes were ordered; that is history independence.

Who acts: a writer inserts or removes entries, in any order. What can happen:
any sequence of inserts and removes. What must never happen: two nodes that end
with the same entries hold different trees, the chunks lose or invent an entry,
an edit that re-cuts only the chunks near it gets a different answer than
cutting the whole list again, or building the levels never stops.

How an entry list is cut. The cutter walks the list left to right, adding each
item to the open chunk. After an item whose hash says "boundary" (`b x`), the
open chunk is closed, unless that item would be the chunk's only item: a chunk
that closes holds at least two items. A boundary decision reads only the item
and whether the open chunk is empty; that is what keeps edits local.

What this model trusts. The boundary test (`b`) and the chunk hash (`H`) are any
functions here: in Rust they are SHA-256 of the item's bytes, and a chunk's hash
is SHA-256 of its encoding (`Node::encode` in `crates/prolly/src/node.rs`).
That the encoding is injective is proved at the end (`encode_injective`); that
SHA-256 then gives each different chunk a different hash is trusted. Keys and
values are numbers here; in Rust they are bytes, compared the same way.

Rust anchors: `chunk` in `crates/prolly/src/chunk.rs`; `is_boundary` in
`crates/prolly/src/hash.rs`; `Tree::build` and `raise` in `crates/prolly/src/tree.rs`;
`rewrite_level` and `splice` in `crates/prolly/src/edit.rs`; `Node::encode` in
`crates/prolly/src/node.rs`. One level of the edit path is modelled
(`rewrite_level_refines`, `rewrite_level_up`), with the window given by
position; Rust finds it by key in a sorted level. Not modelled: walking those
levels up to the root, and `raise` dropping a root that holds a single child.
Those, and the edit as a whole, are checked against a fresh build by the
proptest in `crates/prolly/tests/history.rs`.
-/

namespace ProllyTree

-- Any kind of item: an entry at the bottom level, a child reference above it.
variable {α : Type}

-- The cutter's state: the chunks already closed, and the open chunk.
abbrev State (α : Type) := List (List α) × List α

-- One step of the cutter: add item `x` to the open chunk, and close the chunk
-- when `x` is a boundary and the chunk already held something before `x`.
def step (b : α → Bool) (s : State α) (x : α) : State α :=
  -- `b x` says `x` is a boundary; `!s.2.isEmpty` says `x` is not alone.
  if b x && !s.2.isEmpty then (s.1 ++ [s.2 ++ [x]], []) else (s.1, s.2 ++ [x])

-- Run the cutter over a whole list, starting from state `s`.
def scan (b : α → Bool) (s : State α) (xs : List α) : State α :=
  -- Apply `step` to each item, left to right.
  xs.foldl (step b) s

-- The chunks of a list: the closed ones, then the open one if it holds anything.
def chunks (b : α → Bool) (xs : List α) : List (List α) :=
  -- Start with nothing closed and nothing open.
  match scan b ([], []) xs with
  -- Nothing left open: the closed chunks are all the chunks.
  | (d, []) => d
  -- Something left open: it is the last chunk, shorter maybe, but kept.
  | (d, c) => d ++ [c]

-- A list "closes" when the cutter ends it with nothing left open.
def closes (b : α → Bool) (xs : List α) : Prop :=
  -- The open part after the whole list is empty.
  (scan b ([], []) xs).2 = []

-- Running from closed chunks `d` is running from nothing, with `d` kept in front:
-- what was closed before never changes what comes after.
theorem scan_shift (b : α → Bool) (ys : List α) :
    ∀ (d : List (List α)) (c : List α),
      scan b (d, c) ys = (d ++ (scan b ([], c) ys).1, (scan b ([], c) ys).2) := by
  -- Walk the list one item at a time.
  induction ys with
  -- No items: nothing changes.
  | nil => intro d c; simp [scan]
  -- One item `y`, then the rest `ys`.
  | cons y ys ih =>
    intro d c
    -- Unfold one step of the cutter on both sides.
    simp only [scan, List.foldl_cons] at *
    -- Either `y` closes the chunk or it does not.
    by_cases h : (b y && !c.isEmpty) = true
    · -- It closes: both sides close the same chunk, then run on from empty.
      simp only [step]
      rw [if_pos h, if_pos h, List.nil_append, ih (d ++ [c ++ [y]]) [], ih [c ++ [y]] []]
      simp [List.append_assoc]
    · -- It does not close: both sides just grow the open chunk.
      simp only [step]
      rw [if_neg h, if_neg h, ih d (c ++ [y])]

-- Locality: when a list closes, cutting it with more items after it is cutting
-- each part on its own. So an edit that re-cuts a closed stretch, and stops at a
-- place where the cutter closes, gets the same chunks as cutting everything.
theorem chunks_append (b : α → Bool) (xs ys : List α) (h : closes b xs) :
    chunks b (xs ++ ys) = chunks b xs ++ chunks b ys := by
  -- Name the state after `xs`: closed chunks `d` and an empty open chunk.
  unfold closes at h
  unfold chunks
  -- Cutting `xs ++ ys` is cutting `ys` from where `xs` left off.
  have hs : scan b ([], []) (xs ++ ys) = scan b (scan b ([], []) xs) ys := by
    simp [scan, List.foldl_append]
  -- Where `xs` left off is `(d, [])`.
  cases hs0 : scan b ([], []) xs with
  | mk d c =>
    rw [hs0] at h
    simp only at h
    subst h
    rw [hs, hs0, scan_shift b ys d []]
    -- Now the two sides differ only in where `d` is written.
    cases hr : scan b ([], []) ys with
    | mk d' c' => cases c' <;> simp

-- Two stretches that each close make a stretch that closes.
theorem closes_append (b : α → Bool) (xs ys : List α)
    (hx : closes b xs) (hy : closes b ys) : closes b (xs ++ ys) := by
  -- Cutting `xs ++ ys` is cutting `ys` from the closed state after `xs`.
  unfold closes at *
  have hs : scan b ([], []) (xs ++ ys) = scan b (scan b ([], []) xs) ys := by
    simp [scan, List.foldl_append]
  cases hs0 : scan b ([], []) xs with
  | mk d c =>
    rw [hs0] at hx
    simp only at hx
    subst hx
    rw [hs, hs0, scan_shift b ys d []]
    -- The open part is the open part after `ys` alone, which is empty.
    exact hy

-- Nothing lost, nothing added: the closed chunks, then the open one, read back as
-- what was there before plus the items just cut.
theorem scan_flatten (b : α → Bool) (ys : List α) :
    ∀ (d : List (List α)) (c : List α),
      (scan b (d, c) ys).1.flatten ++ (scan b (d, c) ys).2 = d.flatten ++ c ++ ys := by
  -- Walk the list one item at a time.
  induction ys with
  -- No items: the state is unchanged.
  | nil => intro d c; simp [scan]
  -- One item: whether it closes or not, it moves from `ys` into the state.
  | cons y ys ih =>
    intro d c
    simp only [scan, List.foldl_cons] at *
    by_cases h : (b y && !c.isEmpty) = true
    · simp only [step, h, if_true]; rw [ih]; simp
    · simp only [step, h]; rw [ih]; simp

-- The chunks, put back together, are exactly the list that was cut.
theorem chunks_flatten (b : α → Bool) (xs : List α) : (chunks b xs).flatten = xs := by
  -- Use the general law from the empty state.
  have h := scan_flatten b xs [] []
  unfold chunks
  cases hs : scan b ([], []) xs with
  | mk d c => rw [hs] at h; cases c <;> simpa using h

-- Every chunk the cutter closes holds at least two items.
theorem scan_closed_two (b : α → Bool) (ys : List α) :
    ∀ (d : List (List α)) (c : List α), (∀ ch ∈ d, 2 ≤ ch.length) →
      ∀ ch ∈ (scan b (d, c) ys).1, 2 ≤ ch.length := by
  -- Walk the list one item at a time.
  induction ys with
  -- No items: the closed chunks are the ones we started with.
  | nil => intro d c hd; simpa [scan] using hd
  | cons y ys ih =>
    intro d c hd
    simp only [scan, List.foldl_cons] at *
    by_cases h : (b y && !c.isEmpty) = true
    · -- A chunk closes: it is `c` plus `y`, and `c` was not empty.
      simp only [step, h, if_true]
      apply ih
      intro ch hch
      simp only [List.mem_append, List.mem_singleton] at hch
      rcases hch with hch | hch
      · exact hd ch hch
      · subst hch
        simp only [Bool.and_eq_true, Bool.not_eq_true', List.isEmpty_eq_false_iff] at h
        have : c.length ≠ 0 := by simpa [List.length_eq_zero_iff] using h.2
        simp; omega
    · -- Nothing closes: the closed chunks are unchanged.
      simp only [step, h]
      exact ih d (c ++ [y]) hd

-- Every chunk holds at least one item: no empty chunk is ever written.
theorem chunks_nonempty (b : α → Bool) (xs : List α) :
    ∀ ch ∈ chunks b xs, 1 ≤ ch.length := by
  -- Closed chunks hold two or more; the open one is kept only when not empty.
  have hc := scan_closed_two b xs [] [] (by simp)
  unfold chunks
  cases hs : scan b ([], []) xs with
  | mk d c =>
    rw [hs] at hc
    intro ch hch
    cases c with
    | nil => have := hc ch hch; omega
    | cons z zs =>
      simp only [List.mem_append, List.mem_singleton] at hch
      rcases hch with hch | hch
      · have := hc ch hch; omega
      · subst hch; simp

-- A list of lists, each with at least `k` items, has at least `k` items per list.
theorem length_flatten_ge (k : Nat) (L : List (List α)) (h : ∀ ch ∈ L, k ≤ ch.length) :
    k * L.length ≤ L.flatten.length := by
  -- Count one list at a time.
  induction L with
  | nil => simp
  | cons l L ih =>
    have hl := h l (by simp)
    have hL := ih (fun ch hch => h ch (by simp [hch]))
    rw [List.flatten_cons, List.length_append, List.length_cons, Nat.mul_succ]
    omega

-- There are never more chunks than items.
theorem chunks_length_le (b : α → Bool) (xs : List α) :
    (chunks b xs).length ≤ xs.length := by
  -- Each chunk holds an item, and together they hold all of `xs`.
  have h := length_flatten_ge 1 (chunks b xs) (chunks_nonempty b xs)
  rw [chunks_flatten] at h
  omega

-- With two items or more, cutting gives strictly fewer chunks than items, so each
-- level above is shorter than the one below and building the levels stops.
theorem chunks_shrink (b : α → Bool) (xs : List α) (h2 : 2 ≤ xs.length) :
    (chunks b xs).length < xs.length := by
  -- Count closed chunks (two items or more each) and the open part.
  have hc := scan_closed_two b xs [] [] (by simp)
  have hf := scan_flatten b xs [] []
  unfold chunks
  cases hs : scan b ([], []) xs with
  | mk d c =>
    rw [hs] at hc hf
    have hd := length_flatten_ge 2 d hc
    have hn : d.flatten.length + c.length = xs.length := by
      have := congrArg List.length hf
      simpa only [List.length_append, List.flatten_nil, List.nil_append] using this
    cases c with
    | nil => simp only [List.length_nil, Nat.add_zero] at hn ⊢; omega
    | cons z zs =>
      simp only [List.length_append, List.length_cons, List.length_nil] at hn ⊢; omega

-- An entry: a document key and its heads, both numbers in this model.
abbrev Entry := Nat × Nat

-- One level up: each chunk becomes one item, its last key and its hash `H`.
def up (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : List Entry :=
  -- The last key of a chunk is the largest key under it, used to search down.
  (chunks b xs).map (fun c => ((c.getLast?.map Prod.fst).getD 0, H c))

-- The root level: cut and go up until a level is a single chunk.
def build (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : List Entry :=
  -- Two chunks or more: there is another level above.
  if 2 ≤ (chunks b xs).length then build b H (up b H xs) else xs
termination_by xs.length
decreasing_by
  -- The level above has one item per chunk, and there are fewer chunks than items.
  rename_i h
  simp only [up, List.length_map]
  exact chunks_shrink b xs (Nat.le_trans h (chunks_length_le b xs))

-- An entry list is sorted when each key is smaller than every key after it.
def Sorted (xs : List Entry) : Prop :=
  -- `Pairwise` checks every pair, in list order.
  xs.Pairwise (fun x y => x.1 < y.1)

-- Write `(k, v)`: before the first larger key, replacing the entry for `k` if any.
def insert (k v : Nat) : List Entry → List Entry
  -- At the end: append it.
  | [] => [(k, v)]
  | (k', v') :: t =>
    -- A larger key: `k` goes here.
    if k < k' then (k, v) :: (k', v') :: t
    -- The same key: the new heads replace the old ones.
    else if k = k' then (k, v) :: t
    -- A smaller key: keep looking further on.
    else (k', v') :: insert k v t

-- Remove the entry for key `k`, if there is one.
def erase (k : Nat) : List Entry → List Entry
  -- Nothing to remove.
  | [] => []
  | (k', v') :: t =>
    -- Found it: drop it.
    if k = k' then t
    -- Not yet: keep this one and look further on.
    else (k', v') :: erase k t

-- An entry is in a sorted list after inserting `(k, v)` exactly when it is
-- `(k, v)`, or it was there and has another key.
theorem mem_insert (k v : Nat) (xs : List Entry) (hs : Sorted xs) (e : Entry) :
    e ∈ insert k v xs ↔ e = (k, v) ∨ (e ∈ xs ∧ e.1 ≠ k) := by
  -- Walk the list one entry at a time.
  induction xs with
  | nil => simp [insert]
  | cons x t ih =>
    obtain ⟨k', v'⟩ := x
    unfold Sorted at hs
    rw [List.pairwise_cons] at hs
    have ih := ih hs.2
    simp only [insert]
    -- Every later key is larger than `k'`.
    have hlt : ∀ y ∈ t, k' < y.1 := fun y hy => hs.1 y hy
    by_cases h1 : k < k'
    · simp only [h1, if_true, List.mem_cons]
      constructor
      · rintro (h | h | h)
        · exact Or.inl h
        · subst h; exact Or.inr ⟨Or.inl rfl, by simp; omega⟩
        · exact Or.inr ⟨Or.inr h, by have := hlt e h; omega⟩
      · rintro (h | ⟨h | h, _⟩)
        · exact Or.inl h
        · exact Or.inr (Or.inl h)
        · exact Or.inr (Or.inr h)
    · by_cases h2 : k = k'
      · subst h2
        simp only [Nat.lt_irrefl, if_false, if_true, List.mem_cons]
        constructor
        · rintro (h | h)
          · exact Or.inl h
          · exact Or.inr ⟨Or.inr h, by have := hlt e h; omega⟩
        · rintro (h | ⟨h | h, hne⟩)
          · exact Or.inl h
          · subst h; simp at hne
          · exact Or.inr h
      · simp only [h1, h2, if_false, List.mem_cons, ih]
        constructor
        · rintro (h | h | ⟨h, hne⟩)
          · subst h; exact Or.inr ⟨Or.inl rfl, by simp; omega⟩
          · exact Or.inl h
          · exact Or.inr ⟨Or.inr h, hne⟩
        · rintro (h | ⟨h | h, hne⟩)
          · exact Or.inr (Or.inl h)
          · exact Or.inl h
          · exact Or.inr (Or.inr ⟨h, hne⟩)

-- Every key in the list after inserting `k` is `k` or a key that was there.
theorem key_insert (k v : Nat) (xs : List Entry) (hs : Sorted xs) (e : Entry)
    (he : e ∈ insert k v xs) : e.1 = k ∨ ∃ y ∈ xs, y.1 = e.1 := by
  -- Read it off the membership law.
  rcases (mem_insert k v xs hs e).1 he with h | ⟨h, _⟩
  · subst h; exact Or.inl rfl
  · exact Or.inr ⟨e, h, rfl⟩

-- Inserting keeps the list sorted.
theorem sorted_insert (k v : Nat) (xs : List Entry) (hs : Sorted xs) :
    Sorted (insert k v xs) := by
  -- Walk the list one entry at a time.
  induction xs with
  | nil => simp [insert, Sorted]
  | cons x t ih =>
    obtain ⟨k', v'⟩ := x
    have hs' := hs
    unfold Sorted at hs
    rw [List.pairwise_cons] at hs
    simp only [insert]
    by_cases h1 : k < k'
    · simp only [h1, if_true]
      unfold Sorted
      rw [List.pairwise_cons]
      refine ⟨?_, hs'⟩
      intro y hy
      simp only [List.mem_cons] at hy
      rcases hy with hy | hy
      · subst hy; exact h1
      · have := hs.1 y hy; simp at this ⊢; omega
    · by_cases h2 : k = k'
      · subst h2
        simp only [Nat.lt_irrefl, if_false, if_true]
        unfold Sorted
        rw [List.pairwise_cons]
        exact ⟨hs.1, hs.2⟩
      · simp only [h1, h2, if_false]
        unfold Sorted
        rw [List.pairwise_cons]
        refine ⟨?_, ih hs.2⟩
        intro y hy
        rcases key_insert k v t hs.2 y hy with hk | ⟨z, hz, hzk⟩
        · simp; omega
        · have := hs.1 z hz; simp at this ⊢; omega

-- An entry is in a sorted list after removing `k` exactly when it was there and
-- has another key.
theorem mem_erase (k : Nat) (xs : List Entry) (hs : Sorted xs) (e : Entry) :
    e ∈ erase k xs ↔ e ∈ xs ∧ e.1 ≠ k := by
  -- Walk the list one entry at a time.
  induction xs with
  | nil => simp [erase]
  | cons x t ih =>
    obtain ⟨k', v'⟩ := x
    unfold Sorted at hs
    rw [List.pairwise_cons] at hs
    have ih := ih hs.2
    simp only [erase]
    by_cases h : k = k'
    · subst h
      simp only [if_true, List.mem_cons]
      constructor
      · intro he; exact ⟨Or.inr he, by have := hs.1 e he; omega⟩
      · rintro ⟨h | h, hne⟩
        · subst h; simp at hne
        · exact h
    · simp only [h, if_false, List.mem_cons, ih]
      constructor
      · rintro (h' | ⟨h', hne⟩)
        · subst h'; exact ⟨Or.inl rfl, by simp; omega⟩
        · exact ⟨Or.inr h', hne⟩
      · rintro ⟨h' | h', hne⟩
        · exact Or.inl h'
        · exact Or.inr ⟨h', hne⟩

-- Removing keeps the list sorted: it only drops an entry.
theorem sorted_erase (k : Nat) (xs : List Entry) (hs : Sorted xs) :
    Sorted (erase k xs) := by
  -- Walk the list one entry at a time.
  induction xs with
  | nil => simp [erase, Sorted]
  | cons x t ih =>
    obtain ⟨k', v'⟩ := x
    unfold Sorted at hs
    rw [List.pairwise_cons] at hs
    simp only [erase]
    by_cases h : k = k'
    · simp only [h, if_true]; exact hs.2
    · simp only [h, if_false]
      unfold Sorted
      rw [List.pairwise_cons]
      refine ⟨?_, ih hs.2⟩
      intro y hy
      exact hs.1 y ((mem_erase k t hs.2 y).1 hy).1

-- Two sorted lists holding the same entries are the same list: a sorted list is
-- fixed by what it holds, not by how it was written.
theorem eq_of_same_entries (xs ys : List Entry) (hx : Sorted xs) (hy : Sorted ys)
    (h : ∀ e, e ∈ xs ↔ e ∈ ys) : xs = ys := by
  -- Compare the lists from the front.
  induction xs generalizing ys with
  | nil =>
    cases ys with
    | nil => rfl
    | cons y t => exact absurd ((h y).2 (by simp)) (by simp)
  | cons x t ih =>
    cases ys with
    | nil => exact absurd ((h x).1 (by simp)) (by simp)
    | cons y u =>
      unfold Sorted at hx hy
      rw [List.pairwise_cons] at hx hy
      -- The first entries are the same: each list's first key is its smallest.
      have hxy : x = y := by
        rcases List.mem_cons.1 ((h x).1 (by simp)) with h1 | h1
        · exact h1
        rcases List.mem_cons.1 ((h y).2 (by simp)) with h2 | h2
        · exact h2.symm
        have a := hy.1 x h1
        have c := hx.1 y h2
        omega
      subst hxy
      -- The rest hold the same entries, since none of them is the first entry.
      congr 1
      apply ih u hx.2 hy.2
      intro e
      constructor
      · intro he
        rcases List.mem_cons.1 ((h e).1 (by simp [he])) with h1 | h1
        · subst h1; have := hx.1 e he; omega
        · exact h1
      · intro he
        rcases List.mem_cons.1 ((h e).2 (by simp [he])) with h1 | h1
        · subst h1; have := hy.1 e he; omega
        · exact h1

-- History independence: two sorted entry lists that hold the same entries build
-- the same root, whatever writes produced them.
theorem build_same_entries (b : Entry → Bool) (H : List Entry → Nat)
    (xs ys : List Entry) (hx : Sorted xs) (hy : Sorted ys)
    (h : ∀ e, e ∈ xs ↔ e ∈ ys) : build b H xs = build b H ys := by
  -- The lists are equal, so everything built from them is.
  rw [eq_of_same_entries xs ys hx hy h]

-- Writes to two different documents, in either order, give the same tree.
theorem insert_comm (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hs : Sorted xs) (k₁ v₁ k₂ v₂ : Nat) (hk : k₁ ≠ k₂) :
    build b H (insert k₁ v₁ (insert k₂ v₂ xs)) = build b H (insert k₂ v₂ (insert k₁ v₁ xs)) := by
  -- Both orders hold the same entries.
  apply build_same_entries
  · exact sorted_insert _ _ _ (sorted_insert _ _ _ hs)
  · exact sorted_insert _ _ _ (sorted_insert _ _ _ hs)
  intro e
  rw [mem_insert _ _ _ (sorted_insert _ _ _ hs), mem_insert _ _ _ hs,
    mem_insert _ _ _ (sorted_insert _ _ _ hs), mem_insert _ _ _ hs]
  constructor
  · rintro (h | ⟨h | ⟨h, _⟩, _⟩)
    · subst h; exact Or.inr ⟨Or.inl rfl, by simpa using hk⟩
    · exact Or.inl h
    · exact Or.inr ⟨Or.inr ⟨h, by assumption⟩, by assumption⟩
  · rintro (h | ⟨h | ⟨h, _⟩, _⟩)
    · subst h; exact Or.inr ⟨Or.inl rfl, by simpa using hk.symm⟩
    · exact Or.inl h
    · exact Or.inr ⟨Or.inr ⟨h, by assumption⟩, by assumption⟩

-- A removal and a write to a different document, in either order, give the
-- same tree.
theorem erase_insert_comm (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hs : Sorted xs) (k₁ k₂ v₂ : Nat) (hk : k₁ ≠ k₂) :
    build b H (erase k₁ (insert k₂ v₂ xs)) = build b H (insert k₂ v₂ (erase k₁ xs)) := by
  -- Both orders hold the same entries.
  apply build_same_entries
  · exact sorted_erase _ _ (sorted_insert _ _ _ hs)
  · exact sorted_insert _ _ _ (sorted_erase _ _ hs)
  intro e
  rw [mem_erase _ _ (sorted_insert _ _ _ hs), mem_insert _ _ _ hs,
    mem_insert _ _ _ (sorted_erase _ _ hs), mem_erase _ _ hs]
  constructor
  · rintro ⟨h | ⟨h, h'⟩, hne⟩
    · exact Or.inl h
    · exact Or.inr ⟨⟨h, hne⟩, h'⟩
  · rintro (h | ⟨⟨h, hne⟩, h'⟩)
    · subst h; exact ⟨Or.inl rfl, by simpa using hk.symm⟩
    · exact ⟨Or.inr ⟨h, h'⟩, hne⟩

-- The last write to a document wins: writing it twice is writing it once, last.
theorem insert_insert (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hs : Sorted xs) (k v₁ v₂ : Nat) :
    build b H (insert k v₂ (insert k v₁ xs)) = build b H (insert k v₂ xs) := by
  -- Both hold `(k, v₂)` and the other documents' entries.
  apply build_same_entries
  · exact sorted_insert _ _ _ (sorted_insert _ _ _ hs)
  · exact sorted_insert _ _ _ hs
  intro e
  rw [mem_insert _ _ _ (sorted_insert _ _ _ hs), mem_insert _ _ _ hs, mem_insert _ _ _ hs]
  constructor
  · rintro (h | ⟨h | ⟨h, h'⟩, hne⟩)
    · exact Or.inl h
    · subst h; simp at hne
    · exact Or.inr ⟨h, h'⟩
  · rintro (h | ⟨h, h'⟩)
    · exact Or.inl h
    · exact Or.inr ⟨Or.inr ⟨h, h'⟩, h'⟩

/-!
## The edit path: re-cutting only the chunks around an edit

The story, in plain words. `Tree::insert` and `Tree::remove` do not cut a whole
level again. At one level (`rewrite_level` in `crates/prolly/src/edit.rs`) they
take the old chunks: `P` before the edit, a window `W` that holds the edited
items, and `S` after it. They replace the window's items by new ones `ys`, cut
`ys` from an empty cutter, and then keep feeding in whole old chunks of `S`, one
at a time, until the cutter has nothing open (or `S` runs out). Whatever of `S`
is left is kept as it was. What must never happen: that answer differs from
cutting the whole edited level from scratch. The theorems below show it never
does, for any window and any new items, and that one level up the edit is again
"swap the old window's references for the new chunks' references".
-/

-- Finish a cut: the closed chunks, then the open one if it holds anything
-- (`Chunker::finish`).
def finish (s : State α) : List (List α) :=
  -- Look at what is left open.
  match s with
  -- Nothing open: the closed chunks are all of them.
  | (d, []) => d
  -- Something open: it is the last chunk.
  | (d, c) => d ++ [c]

-- The chunks of a list are its cut, finished.
theorem chunks_eq_finish (b : α → Bool) (xs : List α) :
    chunks b xs = finish (scan b ([], []) xs) := by
  -- Both read the same state the same way.
  unfold chunks finish
  rfl

-- Chunks closed before the state are kept in front when finishing.
theorem finish_shift (d e : List (List α)) (c : List α) :
    finish (d ++ e, c) = d ++ finish (e, c) := by
  -- Either nothing is open or something is.
  cases c <;> simp [finish]

-- Cutting a list with no items changes nothing.
theorem scan_nil (b : α → Bool) (s : State α) : scan b s [] = s := rfl

-- Cutting two lists one after the other is cutting them joined.
theorem scan_append (b : α → Bool) (s : State α) (xs ys : List α) :
    scan b s (xs ++ ys) = scan b (scan b s xs) ys := by
  -- A fold over a joined list is a fold over each part in turn.
  simp [scan, List.foldl_append]

-- A chunk the cutter closes exactly at its end, and nowhere before.
def Closed (b : α → Bool) (c : List α) : Prop :=
  -- Cut on its own, it gives itself as one closed chunk and leaves nothing open.
  scan b ([], []) c = ([c], [])

-- A run the cutter never closes: cut on its own, all of it stays open.
def Pending (b : α → Bool) (c : List α) : Prop :=
  -- Nothing closed, and everything still open.
  scan b ([], []) c = ([], c)

-- What a cut level looks like: closed chunks, then perhaps one open last chunk.
inductive Canon (b : α → Bool) : List (List α) → Prop
  -- No chunks at all.
  | nil : Canon b []
  -- One last chunk the cutter never closed, holding something.
  | last (p : List α) : p ≠ [] → Pending b p → Canon b [p]
  -- A closed chunk, then a cut level.
  | cons (c : List α) (L : List (List α)) : Closed b c → Canon b L → Canon b (c :: L)

-- Nothing open is a run that never closed.
theorem pending_nil (b : α → Bool) : Pending b ([] : List α) := rfl

-- While cutting from an open run that never closed, every chunk closed on the
-- way is `Closed` and what is left open is `Pending`.
theorem scan_canon (b : α → Bool) (ys : List α) :
    ∀ (d : List (List α)) (c : List α), Pending b c →
      ∃ e, (scan b (d, c) ys).1 = d ++ e ∧ (∀ ch ∈ e, Closed b ch) ∧
        Pending b (scan b (d, c) ys).2 := by
  -- Walk the list one item at a time.
  induction ys with
  -- No items: nothing new is closed, and the open run is the one we had.
  | nil => intro d c hc; exact ⟨[], by simp [scan_nil], by simp, hc⟩
  | cons y ys ih =>
    intro d c hc
    -- One step of the cutter, then the rest.
    have hcons : ∀ s, scan b s (y :: ys) = scan b (step b s y) ys := fun _ => rfl
    rw [hcons]
    by_cases h : (b y && !c.isEmpty) = true
    · -- `y` closes `c ++ [y]`, which is `Closed`: `c` never closed on its own.
      have hstep : step b (d, c) y = (d ++ [c ++ [y]], []) := by simp [step, h]
      have hcl : Closed b (c ++ [y]) := by
        unfold Closed
        rw [scan_append, hc]
        simp [scan, step, h]
      obtain ⟨e, he, hall, hp⟩ := ih (d ++ [c ++ [y]]) [] (pending_nil b)
      rw [hstep]
      refine ⟨[c ++ [y]] ++ e, by simp [he], ?_, hp⟩
      intro ch hch
      simp only [List.mem_append, List.mem_singleton] at hch
      rcases hch with hch | hch
      · subst hch; exact hcl
      · exact hall ch hch
    · -- `y` joins the open run, which still never closed.
      have hstep : step b (d, c) y = (d, c ++ [y]) := by simp [step, h]
      have hp : Pending b (c ++ [y]) := by
        unfold Pending
        rw [scan_append, hc]
        simp [scan, step, h]
      rw [hstep]
      exact ih d (c ++ [y]) hp

-- Closed chunks, then an open run that never closed, finish into a cut level.
theorem canon_finish (b : α → Bool) (e : List (List α)) (c : List α)
    (he : ∀ ch ∈ e, Closed b ch) (hc : Pending b c) : Canon b (finish (e, c)) := by
  -- Add the closed chunks one at a time, from the front.
  induction e with
  | nil =>
    -- No closed chunks: just the open run, if it holds anything.
    cases c with
    | nil => exact Canon.nil
    | cons z zs => exact Canon.last (z :: zs) (by simp) hc
  | cons ch e ih =>
    -- One closed chunk in front of a cut level.
    have hf : finish (ch :: e, c) = ch :: finish (e, c) := by
      cases c <;> simp [finish]
    rw [hf]
    exact Canon.cons ch _ (he ch (by simp)) (ih (fun x hx => he x (by simp [hx])))

-- Every level the cutter makes is a cut level.
theorem canon_chunks (b : α → Bool) (xs : List α) : Canon b (chunks b xs) := by
  -- Cut from nothing, and read off the closed chunks and the open run.
  obtain ⟨e, he, hall, hp⟩ := scan_canon b xs [] [] (pending_nil b)
  rw [chunks_eq_finish]
  generalize scan b ([], []) xs = s at he hp
  obtain ⟨d, c⟩ := s
  simp only [List.nil_append] at he
  subst he
  exact canon_finish b d c hall hp

-- A closed chunk ends with nothing open.
theorem closes_of_closed (b : α → Bool) (c : List α) (h : Closed b c) : closes b c := by
  -- Its cut leaves nothing open.
  unfold closes
  rw [h]

-- Cutting a cut level again, put back together, gives the same chunks.
theorem chunks_of_canon (b : α → Bool) (L : List (List α)) (h : Canon b L) :
    chunks b L.flatten = L := by
  -- One chunk at a time.
  induction h with
  -- No chunks: nothing to cut.
  | nil => rfl
  -- The open last chunk alone: it stays one open chunk.
  | last p hp hpe =>
    rw [List.flatten_singleton, chunks_eq_finish, hpe]
    cases p with
    | nil => exact absurd rfl hp
    | cons z zs => simp [finish]
  -- A closed chunk in front: it closes, so the rest is cut on its own.
  | cons c L hc _ ih =>
    rw [List.flatten_cons, chunks_append b c L.flatten (closes_of_closed b c hc), ih,
      chunks_eq_finish, hc]
    simp [finish]

-- Dropping the first chunk of a cut level leaves a cut level.
theorem canon_tail (b : α → Bool) (c : List α) (L : List (List α)) (h : Canon b (c :: L)) :
    Canon b L := by
  -- Either `c` was the open last chunk, or a closed one in front of a cut level.
  cases h with
  | last _ _ _ => exact Canon.nil
  | cons _ _ _ hL => exact hL

-- The chunks after any prefix of a cut level form a cut level.
theorem canon_suffix (b : α → Bool) (P Q : List (List α)) (h : Canon b (P ++ Q)) :
    Canon b Q := by
  -- Drop the prefix one chunk at a time.
  induction P with
  | nil => simpa using h
  | cons c P ih => exact ih (canon_tail b c (P ++ Q) h)

-- A chunk with more chunks after it is a closed one.
theorem canon_cons_closed (b : α → Bool) (c : List α) (L : List (List α))
    (h : Canon b (c :: L)) (hL : L ≠ []) : Closed b c := by
  -- Only the last chunk may be left open.
  cases h with
  | last _ _ _ => exact absurd rfl hL
  | cons _ _ hc _ => exact hc

-- Every chunk before a non-empty rest of a cut level is closed.
theorem canon_prefix_closed (b : α → Bool) (P Q : List (List α)) (h : Canon b (P ++ Q))
    (hQ : Q ≠ []) : ∀ c ∈ P, Closed b c := by
  -- One chunk of the prefix at a time.
  induction P with
  | nil => simp
  | cons c P ih =>
    intro x hx
    simp only [List.mem_cons] at hx
    rcases hx with hx | hx
    · subst hx
      exact canon_cons_closed b x (P ++ Q) h (by simp [hQ])
    · exact ih (canon_tail b c (P ++ Q) h) x hx

-- Closed chunks only make a cut level.
theorem canon_of_closed (b : α → Bool) (P : List (List α)) (h : ∀ c ∈ P, Closed b c) :
    Canon b P := by
  -- One closed chunk at a time.
  induction P with
  | nil => exact Canon.nil
  | cons c P ih => exact Canon.cons c P (h c (by simp)) (ih (fun x hx => h x (by simp [hx])))

-- Closed chunks put back together end with nothing open.
theorem closes_of_all_closed (b : α → Bool) (P : List (List α)) (h : ∀ c ∈ P, Closed b c) :
    closes b P.flatten := by
  -- One closed chunk at a time.
  induction P with
  | nil => rfl
  | cons c P ih =>
    rw [List.flatten_cons]
    exact closes_append b c P.flatten (closes_of_closed b c (h c (by simp)))
      (ih (fun x hx => h x (by simp [hx])))

-- The re-cut after the new items (`rewrite_level`'s second loop): while the
-- cutter holds an open run, feed it the next old chunk whole; once nothing is
-- open, keep the old chunks that are left as they are; at the end, finish.
def recut (b : α → Bool) (s : State α) : List (List α) → List (List α)
  -- No old chunks left: finish the cut (`Chunker::finish`).
  | [] => finish s
  -- An old chunk `ch` is next.
  | ch :: S =>
    -- Nothing open: stop, and keep `ch` and the rest unchanged.
    if s.2.isEmpty then s.1 ++ ch :: S
    -- Something open: cut `ch` too, and look again.
    else recut b (scan b s ch) S

-- Re-cutting into a cut level gives what cutting it all through would.
theorem recut_eq (b : α → Bool) (S : List (List α)) (hS : Canon b S) :
    ∀ s : State α, finish (scan b s S.flatten) = recut b s S := by
  -- One old chunk at a time.
  induction S with
  -- No old chunks: both just finish.
  | nil => intro s; rfl
  | cons ch S ih =>
    intro s
    obtain ⟨d, c⟩ := s
    by_cases hc : c = []
    · -- Nothing open: the old chunks are cut from scratch, and come out as they were.
      subst hc
      simp only [recut, List.isEmpty_nil, if_true]
      -- A pair rebuilt from its two halves is the pair itself.
      have heta : ((scan b ([], []) (ch :: S).flatten).1, (scan b ([], []) (ch :: S).flatten).2)
          = scan b ([], []) (ch :: S).flatten := rfl
      rw [scan_shift b _ d [], finish_shift, heta, ← chunks_eq_finish,
        chunks_of_canon b (ch :: S) hS]
    · -- Something open: cutting `ch` and then the rest is cutting them joined.
      have hne : c.isEmpty = false := by cases c <;> simp_all
      simp only [recut, hne]
      rw [List.flatten_cons, scan_append]
      exact ih (canon_tail b ch S hS) _

/-- Refinement of `rewrite_level`. If a level's chunks are `P ++ W ++ S`, with a
non-empty window `W`, then replacing the window's items by any `ys` and doing the
windowed re-cut gives exactly the chunks of the whole edited level: `P` is kept,
the re-cut makes the middle, and the old chunks it did not need are kept. -/
theorem rewrite_level_refines (b : α → Bool) (xs ys : List α) (P W S : List (List α))
    (h : chunks b xs = P ++ W ++ S) (hW : W ≠ []) :
    chunks b (P.flatten ++ ys ++ S.flatten) = P ++ recut b (scan b ([], []) ys) S := by
  -- The old level is a cut level, split as `P`, then `W ++ S`.
  have hc := canon_chunks b xs
  rw [h, List.append_assoc] at hc
  -- Every chunk of `P` is closed, since the window comes after it.
  have hP := canon_prefix_closed b P (W ++ S) hc (by simp [hW])
  -- The old chunks after the window form a cut level.
  have hS := canon_suffix b W S (canon_suffix b P (W ++ S) hc)
  -- `P` closes, so the edited level is `P` followed by the cut of the rest.
  rw [List.append_assoc, chunks_append b _ _ (closes_of_all_closed b P hP),
    chunks_of_canon b P (canon_of_closed b P hP)]
  -- The cut of the rest is the cut of `ys`, carried on into `S`: the re-cut.
  rw [chunks_eq_finish, scan_append, recut_eq b S hS]

-- The re-cut keeps a tail `R` of the old chunks `S` as it was, after new chunks `N`.
theorem recut_keeps_tail (b : α → Bool) (S : List (List α)) :
    ∀ s : State α, ∃ N S₁ R, S = S₁ ++ R ∧ recut b s S = N ++ R := by
  -- One old chunk at a time.
  induction S with
  -- No old chunks: everything is new.
  | nil => intro s; exact ⟨finish s, [], [], rfl, by simp [recut]⟩
  | cons ch S ih =>
    intro s
    by_cases hs : s.2.isEmpty = true
    · -- It stops here: `ch` and the rest are kept.
      exact ⟨s.1, [], ch :: S, rfl, by simp [recut, hs]⟩
    · -- It cuts `ch` too: `ch` is used up, and the rest is decided later.
      obtain ⟨N, S₁, R, h1, h2⟩ := ih (scan b s ch)
      refine ⟨N, ch :: S₁, R, by simp [h1], ?_⟩
      simp only [recut, hs]
      exact h2

-- The reference one level up for a chunk: its last key and its hash (`child`).
def ref (H : List Entry → Nat) (c : List Entry) : Entry :=
  -- The last key of the chunk, and the chunk's hash.
  ((c.getLast?.map Prod.fst).getD 0, H c)

-- One level up is one reference per chunk.
theorem up_eq_map (b : Entry → Bool) (H : List Entry → Nat) (xs : List Entry) :
    up b H xs = (chunks b xs).map (ref H) := rfl

/-- One level up, the edit is again a window swap (`splice` in `edit.rs`): the
level above the old level and the level above the edited level share the
references before (`P`) and after (`R`), and differ only in that the old
window's references (`W` and the old chunks the re-cut used up, `S₁`) give way
to the new chunks' references (`N`). So the next level is edited the same way,
and `rewrite_level_refines` applies to it again. -/
theorem rewrite_level_up (b : Entry → Bool) (H : List Entry → Nat)
    (xs ys : List Entry) (P W S : List (List Entry))
    (h : chunks b xs = P ++ W ++ S) (hW : W ≠ []) :
    ∃ N S₁ R : List (List Entry), S = S₁ ++ R ∧
      up b H xs = P.map (ref H) ++ (W ++ S₁).map (ref H) ++ R.map (ref H) ∧
      up b H (P.flatten ++ ys ++ S.flatten) =
        P.map (ref H) ++ N.map (ref H) ++ R.map (ref H) := by
  -- The re-cut keeps a tail `R` of `S` and makes new chunks `N`.
  obtain ⟨N, S₁, R, hS, hr⟩ := recut_keeps_tail b S (scan b ([], []) ys)
  refine ⟨N, S₁, R, hS, ?_, ?_⟩
  · -- Above the old level: the references of `P`, `W`, `S₁` and `R`, in order.
    rw [up_eq_map, h, hS]
    simp [List.map_append, List.append_assoc]
  · -- Above the edited level: the references of `P`, `N` and `R`, in order.
    rw [up_eq_map, rewrite_level_refines b xs ys P W S h hW, hr]
    simp [List.map_append, List.append_assoc]

/-!
## The node encoding is injective

The story, in plain words. A node's hash is SHA-256 of its bytes
(`Node::encode` in `crates/prolly/src/node.rs`): the level byte, the item
count, then each key and each value behind its length, every number written as
8 big-endian bytes. What must never happen: two different nodes write the same
bytes, so that they share a hash without a SHA-256 collision. The theorem below
shows it never does, as long as every count and length fits in 64 bits (Rust's
`usize` is at most 64 bits, so it always does). Bytes are numbers here.
-/

-- A number as 8 big-endian bytes, as `u64::to_be_bytes` writes it.
def be64 (n : Nat) : List Nat :=
  -- The highest byte first, the lowest last.
  [n / 72057594037927936 % 256, n / 281474976710656 % 256, n / 1099511627776 % 256,
    n / 4294967296 % 256, n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256]

-- Always 8 bytes.
theorem be64_length (n : Nat) : (be64 n).length = 8 := rfl

-- Two numbers that fit in 64 bits and write the same 8 bytes are equal.
theorem be64_inj (m n : Nat) (hm : m < 18446744073709551616) (hn : n < 18446744073709551616)
    (h : be64 m = be64 n) : m = n := by
  -- Byte by byte, the digits are equal, and the digits fix the number.
  simp only [be64, List.cons.injEq] at h
  omega

-- Reading 8 bytes off the front tells the number and leaves the rest.
theorem be64_cancel (m n : Nat) (r r' : List Nat) (hm : m < 18446744073709551616)
    (hn : n < 18446744073709551616) (h : be64 m ++ r = be64 n ++ r') : m = n ∧ r = r' := by
  -- The first 8 bytes on each side are the two numbers.
  have hs := List.append_inj h (by simp [be64_length])
  exact ⟨be64_inj m n hm hn hs.1, hs.2⟩

-- An item's bytes: its key's length, its key, its value's length, its value.
def encItem (it : List Nat × List Nat) : List Nat :=
  -- Each part behind its length.
  be64 it.1.length ++ it.1 ++ be64 it.2.length ++ it.2

-- The items' bytes, one item after the other.
def encItems : List (List Nat × List Nat) → List Nat
  -- No items: no bytes.
  | [] => []
  -- One item, then the rest.
  | it :: is => encItem it ++ encItems is

-- A node's bytes: its level, its item count, then its items (`Node::encode`).
def encode (level : Nat) (items : List (List Nat × List Nat)) : List Nat :=
  -- The level, the count behind 8 bytes, then the items.
  level :: (be64 items.length ++ encItems items)

-- Every key and value length fits in 64 bits.
def Fits (items : List (List Nat × List Nat)) : Prop :=
  -- For each item, both lengths are below 2^64.
  ∀ it ∈ items, it.1.length < 18446744073709551616 ∧ it.2.length < 18446744073709551616

-- The same number of items, written the same way with anything after, are the
-- same items with the same thing after.
theorem encItems_cancel (is js : List (List Nat × List Nat)) (r r' : List Nat)
    (hl : is.length = js.length) (hi : Fits is) (hj : Fits js)
    (h : encItems is ++ r = encItems js ++ r') : is = js ∧ r = r' := by
  -- Read the items off the front one at a time.
  induction is generalizing js with
  | nil =>
    -- No items on one side, so none on the other.
    cases js with
    | nil => simpa [encItems] using h
    | cons _ _ => simp at hl
  | cons i is ih =>
    cases js with
    | nil => simp at hl
    | cons j js =>
      obtain ⟨hi1, hi2⟩ := hi i (by simp)
      obtain ⟨hj1, hj2⟩ := hj j (by simp)
      simp only [encItems, encItem, List.append_assoc] at h
      -- The key's length, then the key itself.
      obtain ⟨hk, h⟩ := be64_cancel _ _ _ _ hi1 hj1 h
      obtain ⟨hk', h⟩ := List.append_inj h hk
      -- The value's length, then the value itself.
      obtain ⟨hv, h⟩ := be64_cancel _ _ _ _ hi2 hj2 h
      obtain ⟨hv', h⟩ := List.append_inj h hv
      -- The rest is the other items, read the same way.
      obtain ⟨his, hr⟩ := ih js (by simpa using hl) (fun x hx => hi x (by simp [hx]))
        (fun x hx => hj x (by simp [hx])) h
      exact ⟨by rw [Prod.ext hk' hv', his], hr⟩

/-- Two nodes whose counts and lengths fit in 64 bits and that write the same
bytes are the same node: same level, same items. -/
theorem encode_injective (l l' : Nat) (is js : List (List Nat × List Nat))
    (hn : is.length < 18446744073709551616) (hn' : js.length < 18446744073709551616)
    (hi : Fits is) (hj : Fits js) (h : encode l is = encode l' js) : l = l' ∧ is = js := by
  -- The first byte is the level.
  simp only [encode, List.cons.injEq] at h
  obtain ⟨hlv, h⟩ := h
  -- The next 8 bytes are the count.
  obtain ⟨hc, h⟩ := be64_cancel _ _ _ _ hn hn' h
  -- Then come the items, with nothing after them.
  have h' : encItems is ++ [] = encItems js ++ [] := by simpa using h
  exact ⟨hlv, (encItems_cancel is js [] [] hc hi hj h').1⟩

end ProllyTree
