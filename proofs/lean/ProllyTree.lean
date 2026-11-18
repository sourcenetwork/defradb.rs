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
is SHA-256 of its encoding (`Node::encode` in `crates/prolly/src/node.rs`:
every length behind a 64-bit prefix, so the encoding is injective), which this
model takes as giving each different chunk a different hash. Keys and values
are numbers here; in Rust they are bytes, compared the same way.

Rust anchors: `chunk` in `crates/prolly/src/chunk.rs`; `is_boundary` in
`crates/prolly/src/hash.rs`; `Tree::build` and `raise` in `crates/prolly/src/tree.rs`. The incremental
edit path (`Tree::insert`, `Tree::remove`, `rewrite_level`, `splice` in
`crates/prolly/src/edit.rs`) is not modelled here: `insert_comm` and the like
are about `build` of the edited list. That path is checked against a fresh
build by the proptest in `crates/prolly/tests/history.rs`.
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

end ProllyTree
