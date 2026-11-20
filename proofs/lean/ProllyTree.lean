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
item to the open chunk. After an item that says "close" (`b c x`, for the open
chunk `c`), the open chunk is closed, unless that item would be the chunk's only
item: a chunk that closes holds at least two items. A close decision reads only
the item and the chunk it is joining, which starts fresh after every close;
that is what keeps edits local. In Rust an item closes its chunk when its hash
says "boundary" or when the chunk reaches `MAX_CHUNK_BYTES` (`capped`), so an
open chunk never grows past the cap (`open_bounded`), whatever the input.

What this model trusts. The boundary test (`b`) and the chunk hash (`H`) are any
functions here: in Rust they are SHA-256 of the item's bytes, and a chunk's hash
is SHA-256 of its encoding (`Node::encode` in `crates/foundation/prolly/src/node.rs`).
That the encoding is injective is proved at the end (`encode_injective`); that
SHA-256 then gives each different chunk a different hash is trusted. Keys and
values are numbers here; in Rust they are bytes, compared the same way.

Rust anchors: `chunk` in `crates/foundation/prolly/src/chunk.rs`; `is_boundary` in
`crates/foundation/prolly/src/hash.rs`; `raise` and `collapse` in
`crates/foundation/prolly/src/tree.rs`; `Builder` in `crates/foundation/prolly/src/build.rs`;
`Tree::edit`, `rewrite_level` and `splice` in `crates/foundation/prolly/src/edit.rs`;
`diff` and `Walk` in `crates/foundation/prolly/src/diff.rs`; `Node::encode` in
`crates/foundation/prolly/src/node.rs`. The edit is proved level by level
(`rewrite_level_refines`, `rewrite_level_up`) and walked up to the root, the
single-child collapse included (`edit_root`); `Builder`'s one-pass build gives
the same root (`builder_root`); and `diff`'s walk reports exactly the keys
whose values differ (`diff_eq`, `mergeDiff_spec`). Each section below says
what it trusts. Rust finds an edit's window by key in a sorted level; here
any window that holds the change will do.
-/

namespace ProllyTree

-- Any kind of item: an entry at the bottom level, a child reference above it.
variable {α : Type}

-- The cutter's state: the chunks already closed, and the open chunk.
abbrev State (α : Type) := List (List α) × List α

-- One step of the cutter: add item `x` to the open chunk, and close the chunk
-- when `x` is a boundary and the chunk already held something before `x`.
def step (b : List α → α → Bool) (s : State α) (x : α) : State α :=
  -- `b s.2 x` says `x` closes the open chunk `s.2`; `!s.2.isEmpty` says `x` is not alone.
  if b s.2 x && !s.2.isEmpty then (s.1 ++ [s.2 ++ [x]], []) else (s.1, s.2 ++ [x])

-- Run the cutter over a whole list, starting from state `s`.
def scan (b : List α → α → Bool) (s : State α) (xs : List α) : State α :=
  -- Apply `step` to each item, left to right.
  xs.foldl (step b) s

-- The chunks of a list: the closed ones, then the open one if it holds anything.
def chunks (b : List α → α → Bool) (xs : List α) : List (List α) :=
  -- Start with nothing closed and nothing open.
  match scan b ([], []) xs with
  -- Nothing left open: the closed chunks are all the chunks.
  | (d, []) => d
  -- Something left open: it is the last chunk, shorter maybe, but kept.
  | (d, c) => d ++ [c]

-- A list "closes" when the cutter ends it with nothing left open.
def closes (b : List α → α → Bool) (xs : List α) : Prop :=
  -- The open part after the whole list is empty.
  (scan b ([], []) xs).2 = []

-- Rust's close rule (`Chunker::push`): the item's hash says "boundary" (`h x`),
-- or the open chunk with `x` added weighs at least `cap` (`w` is an item's
-- encoded bytes, `Item::encoded_len`; `cap` is `MAX_CHUNK_BYTES`).
def capped (h : α → Bool) (w : α → Nat) (cap : Nat) (c : List α) (x : α) : Bool :=
  -- A boundary, or the chunk would reach the cap.
  h x || decide (cap ≤ ((c ++ [x]).map w).sum)

-- An open chunk is "bounded" when it weighs under the cap or holds one item.
def Bounded (w : α → Nat) (cap : Nat) (c : List α) : Prop :=
  -- Under the cap, or a single item (which no rule can split).
  (c.map w).sum < cap ∨ c.length ≤ 1

/-- One step under the capped rule leaves a bounded open chunk, whatever it
started from. -/
theorem step_bounded (h : α → Bool) (w : α → Nat) (cap : Nat) (s : State α) (x : α) :
    Bounded w cap (step (capped h w cap) s x).2 := by
  -- Did `x` close the chunk?
  by_cases hc : (capped h w cap s.2 x && !s.2.isEmpty) = true
  · -- It closed: nothing is open.
    simp [step, hc, Bounded]
  · -- It did not: the open chunk is the old one plus `x`.
    simp only [step, hc]
    cases hs : s.2 with
    | nil => simp [Bounded]
    | cons y ys =>
      -- The chunk was not empty, so `x` did not reach the cap.
      left
      rw [hs] at hc
      simp only [capped, List.isEmpty_cons, Bool.not_false, Bool.and_true,
        Bool.or_eq_true, decide_eq_true_eq, not_or, Nat.not_le] at hc
      exact hc.2

/-- Under the capped rule the open chunk stays bounded for any input, boundary
hits or none: the cutter holds less than `cap` bytes open, or one item. -/
theorem open_bounded (h : α → Bool) (w : α → Nat) (cap : Nat) (xs : List α) :
    ∀ s : State α, Bounded w cap s.2 → Bounded w cap (scan (capped h w cap) s xs).2 := by
  -- One item at a time.
  induction xs with
  | nil => intro s hs; exact hs
  | cons x xs ih => intro s _; exact ih _ (step_bounded h w cap s x)

-- Running from closed chunks `d` is running from nothing, with `d` kept in front:
-- what was closed before never changes what comes after.
theorem scan_shift (b : List α → α → Bool) (ys : List α) :
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
    by_cases h : (b c y && !c.isEmpty) = true
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
theorem chunks_append (b : List α → α → Bool) (xs ys : List α) (h : closes b xs) :
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
theorem closes_append (b : List α → α → Bool) (xs ys : List α)
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
theorem scan_flatten (b : List α → α → Bool) (ys : List α) :
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
    by_cases h : (b c y && !c.isEmpty) = true
    · simp only [step, h, if_true]; rw [ih]; simp
    · simp only [step, h]; rw [ih]; simp

-- The chunks, put back together, are exactly the list that was cut.
theorem chunks_flatten (b : List α → α → Bool) (xs : List α) : (chunks b xs).flatten = xs := by
  -- Use the general law from the empty state.
  have h := scan_flatten b xs [] []
  unfold chunks
  cases hs : scan b ([], []) xs with
  | mk d c => rw [hs] at h; cases c <;> simpa using h

-- Every chunk the cutter closes holds at least two items.
theorem scan_closed_two (b : List α → α → Bool) (ys : List α) :
    ∀ (d : List (List α)) (c : List α), (∀ ch ∈ d, 2 ≤ ch.length) →
      ∀ ch ∈ (scan b (d, c) ys).1, 2 ≤ ch.length := by
  -- Walk the list one item at a time.
  induction ys with
  -- No items: the closed chunks are the ones we started with.
  | nil => intro d c hd; simpa [scan] using hd
  | cons y ys ih =>
    intro d c hd
    simp only [scan, List.foldl_cons] at *
    by_cases h : (b c y && !c.isEmpty) = true
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
theorem chunks_nonempty (b : List α → α → Bool) (xs : List α) :
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
theorem chunks_length_le (b : List α → α → Bool) (xs : List α) :
    (chunks b xs).length ≤ xs.length := by
  -- Each chunk holds an item, and together they hold all of `xs`.
  have h := length_flatten_ge 1 (chunks b xs) (chunks_nonempty b xs)
  rw [chunks_flatten] at h
  omega

-- With two items or more, cutting gives strictly fewer chunks than items, so each
-- level above is shorter than the one below and building the levels stops.
theorem chunks_shrink (b : List α → α → Bool) (xs : List α) (h2 : 2 ≤ xs.length) :
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
def up (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : List Entry :=
  -- The last key of a chunk is the largest key under it, used to search down.
  (chunks b xs).map (fun c => ((c.getLast?.map Prod.fst).getD 0, H c))

-- The root level: cut and go up until a level is a single chunk.
def build (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : List Entry :=
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
theorem build_same_entries (b : List Entry → Entry → Bool) (H : List Entry → Nat)
    (xs ys : List Entry) (hx : Sorted xs) (hy : Sorted ys)
    (h : ∀ e, e ∈ xs ↔ e ∈ ys) : build b H xs = build b H ys := by
  -- The lists are equal, so everything built from them is.
  rw [eq_of_same_entries xs ys hx hy h]

-- Writes to two different documents, in either order, give the same tree.
theorem insert_comm (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
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
theorem erase_insert_comm (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
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
theorem insert_insert (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
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
level again. At one level (`rewrite_level` in `crates/foundation/prolly/src/edit.rs`) they
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
theorem chunks_eq_finish (b : List α → α → Bool) (xs : List α) :
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
theorem scan_nil (b : List α → α → Bool) (s : State α) : scan b s [] = s := rfl

-- Cutting two lists one after the other is cutting them joined.
theorem scan_append (b : List α → α → Bool) (s : State α) (xs ys : List α) :
    scan b s (xs ++ ys) = scan b (scan b s xs) ys := by
  -- A fold over a joined list is a fold over each part in turn.
  simp [scan, List.foldl_append]

-- A chunk the cutter closes exactly at its end, and nowhere before.
def Closed (b : List α → α → Bool) (c : List α) : Prop :=
  -- Cut on its own, it gives itself as one closed chunk and leaves nothing open.
  scan b ([], []) c = ([c], [])

-- A run the cutter never closes: cut on its own, all of it stays open.
def Pending (b : List α → α → Bool) (c : List α) : Prop :=
  -- Nothing closed, and everything still open.
  scan b ([], []) c = ([], c)

-- What a cut level looks like: closed chunks, then perhaps one open last chunk.
inductive Canon (b : List α → α → Bool) : List (List α) → Prop
  -- No chunks at all.
  | nil : Canon b []
  -- One last chunk the cutter never closed, holding something.
  | last (p : List α) : p ≠ [] → Pending b p → Canon b [p]
  -- A closed chunk, then a cut level.
  | cons (c : List α) (L : List (List α)) : Closed b c → Canon b L → Canon b (c :: L)

-- Nothing open is a run that never closed.
theorem pending_nil (b : List α → α → Bool) : Pending b ([] : List α) := rfl

-- While cutting from an open run that never closed, every chunk closed on the
-- way is `Closed` and what is left open is `Pending`.
theorem scan_canon (b : List α → α → Bool) (ys : List α) :
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
    by_cases h : (b c y && !c.isEmpty) = true
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
theorem canon_finish (b : List α → α → Bool) (e : List (List α)) (c : List α)
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
theorem canon_chunks (b : List α → α → Bool) (xs : List α) : Canon b (chunks b xs) := by
  -- Cut from nothing, and read off the closed chunks and the open run.
  obtain ⟨e, he, hall, hp⟩ := scan_canon b xs [] [] (pending_nil b)
  rw [chunks_eq_finish]
  generalize scan b ([], []) xs = s at he hp
  obtain ⟨d, c⟩ := s
  simp only [List.nil_append] at he
  subst he
  exact canon_finish b d c hall hp

-- A closed chunk ends with nothing open.
theorem closes_of_closed (b : List α → α → Bool) (c : List α) (h : Closed b c) : closes b c := by
  -- Its cut leaves nothing open.
  unfold closes
  rw [h]

-- Cutting a cut level again, put back together, gives the same chunks.
theorem chunks_of_canon (b : List α → α → Bool) (L : List (List α)) (h : Canon b L) :
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
theorem canon_tail (b : List α → α → Bool) (c : List α) (L : List (List α)) (h : Canon b (c :: L)) :
    Canon b L := by
  -- Either `c` was the open last chunk, or a closed one in front of a cut level.
  cases h with
  | last _ _ _ => exact Canon.nil
  | cons _ _ _ hL => exact hL

-- The chunks after any prefix of a cut level form a cut level.
theorem canon_suffix (b : List α → α → Bool) (P Q : List (List α)) (h : Canon b (P ++ Q)) :
    Canon b Q := by
  -- Drop the prefix one chunk at a time.
  induction P with
  | nil => simpa using h
  | cons c P ih => exact ih (canon_tail b c (P ++ Q) h)

-- A chunk with more chunks after it is a closed one.
theorem canon_cons_closed (b : List α → α → Bool) (c : List α) (L : List (List α))
    (h : Canon b (c :: L)) (hL : L ≠ []) : Closed b c := by
  -- Only the last chunk may be left open.
  cases h with
  | last _ _ _ => exact absurd rfl hL
  | cons _ _ hc _ => exact hc

-- Every chunk before a non-empty rest of a cut level is closed.
theorem canon_prefix_closed (b : List α → α → Bool) (P Q : List (List α)) (h : Canon b (P ++ Q))
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
theorem canon_of_closed (b : List α → α → Bool) (P : List (List α)) (h : ∀ c ∈ P, Closed b c) :
    Canon b P := by
  -- One closed chunk at a time.
  induction P with
  | nil => exact Canon.nil
  | cons c P ih => exact Canon.cons c P (h c (by simp)) (ih (fun x hx => h x (by simp [hx])))

-- Closed chunks put back together end with nothing open.
theorem closes_of_all_closed (b : List α → α → Bool) (P : List (List α)) (h : ∀ c ∈ P, Closed b c) :
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
def recut (b : List α → α → Bool) (s : State α) : List (List α) → List (List α)
  -- No old chunks left: finish the cut (`Chunker::finish`).
  | [] => finish s
  -- An old chunk `ch` is next.
  | ch :: S =>
    -- Nothing open: stop, and keep `ch` and the rest unchanged.
    if s.2.isEmpty then s.1 ++ ch :: S
    -- Something open: cut `ch` too, and look again.
    else recut b (scan b s ch) S

-- Re-cutting into a cut level gives what cutting it all through would.
theorem recut_eq (b : List α → α → Bool) (S : List (List α)) (hS : Canon b S) :
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
theorem rewrite_level_refines (b : List α → α → Bool) (xs ys : List α) (P W S : List (List α))
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
theorem recut_keeps_tail (b : List α → α → Bool) (S : List (List α)) :
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
theorem up_eq_map (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) :
    up b H xs = (chunks b xs).map (ref H) := rfl

/-- One level up, the edit is again a window swap (`splice` in `edit.rs`): the
level above the old level and the level above the edited level share the
references before (`P`) and after (`R`), and differ only in that the old
window's references (`W` and the old chunks the re-cut used up, `S₁`) give way
to the new chunks' references (`N`). So the next level is edited the same way,
and `rewrite_level_refines` applies to it again. -/
theorem rewrite_level_up (b : List Entry → Entry → Bool) (H : List Entry → Nat)
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
## The edit, walked up to the root

The story, in plain words. An edit (`Tree::edit` in
`crates/foundation/prolly/src/edit.rs`) runs `rewrite_level` once per level
below the old root, bottom first. Each level hands its new chunks to the level
above as references, which replace the references of the old chunks it used
up (`splice`). At the old root's level the references are spliced in, and
`raise` goes on cutting until a level is one chunk; then it drops every root
that holds a single child (`collapse` in `crates/foundation/prolly/src/tree.rs`).
What must never happen: that root differs from the root a fresh build of the
edited entries gives. The theorems below show it never does, whatever window
each level uses, and whether the tree grows taller, keeps its height, or
shrinks.

What this model trusts. Rust finds each level's window by key (the chunks
whose keys reach the edited run); here a window is any run of chunks outside
which the old and the edited level agree. Rust reads the old levels from the
store; here they are the levels of a fresh build of the old entries, which is
what history independence (`build_same_entries`) makes every stored tree.
-/

-- Level `j` of a fresh build of `xs`, counted from the bottom.
def lvl (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : Nat → List Entry
  -- The bottom level is the entries themselves.
  | 0 => xs
  -- Each level above holds one reference per chunk of the level below.
  | j + 1 => up b H (lvl b H xs j)

/-- What an edit holds after each level, as `Tree::edit` runs it.
`Walk b H xs xs' i L E` says: the edit has reached level `i`, where the old
tree's level is `L` and the edit has made the edited level's items `E`. -/
inductive Walk (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs xs' : List Entry) :
    Nat → List Entry → List Entry → Prop
  -- At the bottom the old level is the old entries, and the edited level the
  -- edited ones.
  | start : Walk b H xs xs' 0 xs xs'
  -- One `rewrite_level` and the `splice` one level up. The old level's chunks
  -- are `P`, a window `W` (never empty) and `S`; outside the window the old
  -- and edited levels agree, so the edited level is `P`'s items, new items
  -- `ys`, then `S`'s items. The re-cut of `ys` into `S` makes new chunks `N`
  -- and keeps an old tail `R`. One level up, the old level is the references
  -- of the old chunks, and the edited level is those of `P`, `N` and `R`.
  | step (i : Nat) (L E ys : List Entry) (P W S N R : List (List Entry))
      (hw : Walk b H xs xs' i L E) (hc : chunks b L = P ++ W ++ S) (hW : W ≠ [])
      (hE : E = P.flatten ++ ys ++ S.flatten)
      (hr : recut b (scan b ([], []) ys) S = N ++ R) :
      Walk b H xs xs' (i + 1) (up b H L) (P.map (ref H) ++ N.map (ref H) ++ R.map (ref H))

-- One level up has one reference per chunk.
theorem length_up (b : List Entry → Entry → Bool) (H : List Entry → Nat) (X : List Entry) :
    (up b H X).length = (chunks b X).length := by
  -- `up` maps each chunk to its reference.
  simp [up]

/-- Every level the edit makes is the level a fresh build of the edited entries
has at that height, and the old level is the old tree's: at each level the
windowed re-cut is the cut of the whole edited level (`rewrite_level_refines`),
so its references are the fresh level above. -/
theorem walk_levels (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs xs' : List Entry)
    (i : Nat) (L E : List Entry) (hw : Walk b H xs xs' i L E) :
    L = lvl b H xs i ∧ E = lvl b H xs' i := by
  -- Follow the walk one level at a time.
  induction hw with
  -- At the bottom both are the entries.
  | start => exact ⟨rfl, rfl⟩
  | step i L E ys P W S N R _ hc hW hE hr ih =>
    obtain ⟨hL, hEl⟩ := ih
    -- The old level above is the references of the old level's chunks.
    refine ⟨by rw [hL]; rfl, ?_⟩
    -- The edited level above is the references of the edited level's chunks.
    show _ = up b H (lvl b H xs' i)
    rw [← hEl, hE, up_eq_map, rewrite_level_refines b L ys P W S hc hW, hr]
    simp [List.map_append, List.append_assoc]

-- `raise` in `tree.rs`, keeping count of the level: while a level cuts into two
-- chunks or more, go up; stop at the first that is one chunk, or none.
def raise (b : List Entry → Entry → Bool) (H : List Entry → Nat) (X : List Entry) (j : Nat) :
    Nat × List Entry :=
  -- Two chunks or more: there is another level above.
  if 2 ≤ (chunks b X).length then raise b H (up b H X) (j + 1) else (j, X)
termination_by X.length
decreasing_by
  -- The level above has fewer items than this one, as in `build`.
  rename_i h
  simp only [up, List.length_map]
  exact chunks_shrink b X (Nat.le_trans h (chunks_length_le b X))

-- `raise` stops at the root level `build` gives.
theorem raise_snd (b : List Entry → Entry → Bool) (H : List Entry → Nat) (X : List Entry) (j : Nat) :
    (raise b H X j).2 = build b H X := by
  -- Both take the same steps.
  rw [raise, build]
  by_cases h : 2 ≤ (chunks b X).length
  · rw [if_pos h, if_pos h]
    exact raise_snd b H (up b H X) (j + 1)
  · rw [if_neg h, if_neg h]
termination_by X.length
decreasing_by
  -- The level above is shorter, as in `build`.
  simp only [up, List.length_map]
  exact chunks_shrink b X (Nat.le_trans h (chunks_length_le b X))

-- `k` is where `raise`, started at level `j` of the edited entries' fresh build,
-- stops: the first level from `j` on that cuts into fewer than two chunks.
def Top (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) (j k : Nat) : Prop :=
  -- It is at or above `j`, it is one chunk or none, and every level between
  -- `j` and it cuts into two chunks or more.
  j ≤ k ∧ (chunks b (lvl b H xs k)).length < 2 ∧
    ∀ i, j ≤ i → i < k → 2 ≤ (chunks b (lvl b H xs i)).length

-- Started on a fresh level, `raise` goes up the fresh levels and stops at the
-- first one that is one chunk.
theorem raise_lvl (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) (j : Nat) :
    ∃ k, Top b H xs j k ∧ raise b H (lvl b H xs j) j = (k, lvl b H xs k) := by
  rw [raise]
  by_cases hc : 2 ≤ (chunks b (lvl b H xs j)).length
  · -- Two chunks or more: it goes on from the next level, which is fresh too.
    rw [if_pos hc]
    obtain ⟨k, ⟨hjk, hk, hall⟩, hr⟩ := raise_lvl b H xs (j + 1)
    refine ⟨k, ⟨by omega, hk, fun i hi hik => ?_⟩, hr⟩
    -- Every level from `j` up to `k` cuts into two chunks or more.
    by_cases hij : i = j
    · subst hij; exact hc
    · exact hall i (by omega) hik
  · -- One chunk or none: it stops here.
    rw [if_neg hc]
    exact ⟨j, ⟨Nat.le_refl j, by omega, fun i hi hik => by omega⟩, rfl⟩
termination_by (lvl b H xs j).length
decreasing_by
  -- The next level is shorter.
  show (up b H (lvl b H xs j)).length < _
  rw [length_up]
  exact chunks_shrink b _ (Nat.le_trans hc (chunks_length_le b _))

-- A level that is one chunk or none has one or no reference above it, so the
-- level above is one chunk or none too.
theorem one_chunk_up (b : List Entry → Entry → Bool) (H : List Entry → Nat) (X : List Entry)
    (h : (chunks b X).length < 2) : (chunks b (up b H X)).length < 2 := by
  -- There are never more chunks than items.
  have := chunks_length_le b (up b H X)
  rw [length_up] at this
  omega

-- Once a fresh level is one chunk or none, so is every level above it.
theorem one_chunk_above (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (k : Nat) (h : (chunks b (lvl b H xs k)).length < 2) :
    ∀ d, (chunks b (lvl b H xs (k + d))).length < 2 := by
  -- One level at a time.
  intro d
  induction d with
  | zero => exact h
  | succ d ih => exact one_chunk_up b H _ ih

-- A list with an item cuts into at least one chunk.
theorem chunks_ne_nil (b : List α → α → Bool) (X : List α) (h : X ≠ []) : chunks b X ≠ [] := by
  -- The chunks hold every item, so none would hold nothing.
  intro hc
  have := chunks_flatten b X
  rw [hc] at this
  exact h this.symm

-- Every fresh level of entries that are not empty holds an item.
theorem lvl_ne_nil (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (h : xs ≠ []) (j : Nat) : lvl b H xs j ≠ [] := by
  -- One level at a time: a level with an item has a chunk, so a reference above.
  induction j with
  | zero => exact h
  | succ j ih =>
    intro hn
    have := congrArg List.length hn
    simp only [lvl, length_up, List.length_nil] at this
    exact chunks_ne_nil b _ ih (List.eq_nil_of_length_eq_zero this)

/-- Why `collapse` goes down to the level below: a level holding a single
reference refers to the whole level below, which is then one chunk. -/
theorem single_ref (b : List Entry → Entry → Bool) (H : List Entry → Nat) (X : List Entry)
    (h : (up b H X).length = 1) : up b H X = [ref H X] := by
  -- One reference means one chunk, and a single chunk holds the whole level.
  rw [length_up] at h
  obtain ⟨c, hc⟩ := List.length_eq_one_iff.mp h
  have hf := chunks_flatten b X
  rw [hc] at hf
  simp only [List.flatten_cons, List.flatten_nil, List.append_nil] at hf
  rw [up_eq_map, hc, hf]
  rfl

-- `collapse` in `tree.rs`: from root level `j`, while the root is above the
-- bottom and holds a single reference, the root becomes the node it refers to,
-- one level down (`single_ref`). `node j` is the node at level `j`.
def descend (node : Nat → List Entry) : Nat → Nat
  -- At the bottom it stops.
  | 0 => 0
  -- A single reference: go down; anything else: stop here.
  | j + 1 => if (node (j + 1)).length = 1 then descend node j else j + 1

-- From any level at or above a fresh build's root, `collapse` comes down to that
-- root: above it every level holds a single reference, and the root itself is
-- the bottom or holds two references or more.
theorem descend_top (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hne : xs ≠ []) (t : Nat) (ht : Top b H xs 0 t) :
    ∀ d, descend (lvl b H xs) (t + d) = t := by
  intro d
  induction d with
  | zero =>
    cases t with
    -- The root is the bottom: it stops there.
    | zero => rfl
    | succ j =>
      -- The level below the root cuts into two chunks or more, so the root holds
      -- two references or more and is kept.
      have h2 := ht.2.2 j (Nat.zero_le _) (Nat.lt_succ_self j)
      have hl : (lvl b H xs (j + 1)).length ≠ 1 := by
        simp only [lvl, length_up]; omega
      simp only [Nat.add_zero, descend, if_neg hl]
  | succ d ih =>
    -- Above the root: the level below is one chunk, and not empty, so this level
    -- holds a single reference and `collapse` goes down to it.
    rw [show t + (d + 1) = (t + d) + 1 by omega, descend]
    have hk := one_chunk_above b H xs t ht.2.1 d
    have hpos : (chunks b (lvl b H xs (t + d))).length ≠ 0 := by
      intro h0
      exact chunks_ne_nil b _ (lvl_ne_nil b H xs hne _) (List.eq_nil_of_length_eq_zero h0)
    have hl : (lvl b H xs (t + d + 1)).length = 1 := by
      simp only [lvl, length_up]; omega
    rw [if_pos hl]
    exact ih

-- The root a `raise` and its `collapse` leave: the level they stop at, and the
-- node there.
def rootOf (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) (r : Nat) :
    Nat × List Entry :=
  -- Come down from where `raise` stopped, and take that level's node.
  (descend (lvl b H xs) r, lvl b H xs (descend (lvl b H xs) r))

/-- Wherever `raise` stops, as long as that level is one chunk or none,
`collapse` comes down to the root of a fresh build: every level under the fresh
root cuts into two chunks or more, so the stop is at or above it. -/
theorem root_from (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hne : xs ≠ []) (k : Nat) (hk : (chunks b (lvl b H xs k)).length < 2) :
    rootOf b H xs k = ((raise b H xs 0).1, build b H xs) := by
  -- Where `raise` stops from the bottom: the fresh root.
  obtain ⟨t, ht, hrt⟩ := raise_lvl b H xs 0
  change raise b H xs 0 = (t, lvl b H xs t) at hrt
  -- The stop is not below the fresh root.
  have hkt : t ≤ k := by
    rcases Nat.lt_or_ge k t with hlt | hge
    · have := ht.2.2 k (Nat.zero_le _) hlt
      omega
    · exact hge
  -- So `collapse` comes down to the fresh root, whose node is `build`'s.
  have hd := descend_top b H xs hne t ht (k - t)
  rw [show t + (k - t) = k by omega] at hd
  have hb := raise_snd b H xs 0
  rw [hrt] at hb
  rw [hrt]
  simp only [rootOf, hd]
  exact congrArg _ hb

/-- The edit walked to the root (`Tree::edit`). Whatever windows the levels
used and whatever level `h` the old root was at, raising the edited root level
and collapsing it gives the root of a fresh build of the edited entries: the
level `raise` reaches from the bottom, with the node `build` gives. -/
theorem edit_root (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs xs' L E : List Entry)
    (h : Nat) (hw : Walk b H xs xs' h L E) (hne : xs' ≠ []) :
    (raise b H E h).2 = lvl b H xs' (raise b H E h).1 ∧
      rootOf b H xs' (raise b H E h).1 = ((raise b H xs' 0).1, build b H xs') := by
  -- The edited root level is the fresh level `h`.
  obtain ⟨_, hE⟩ := walk_levels b H xs xs' h L E hw
  subst hE
  -- `raise` from level `h` goes up the fresh levels to one that is one chunk.
  obtain ⟨k, hk, hrk⟩ := raise_lvl b H xs' h
  rw [hrk]
  exact ⟨rfl, root_from b H xs' hne k hk.2.1⟩

/-- A fresh build's own `collapse` changes nothing: its root holds two references
or more, or is the bottom level. -/
theorem build_root (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hne : xs ≠ []) :
    rootOf b H xs (raise b H xs 0).1 = ((raise b H xs 0).1, build b H xs) :=
  -- The fresh build is the edit that changes nothing.
  (edit_root b H xs xs xs xs 0 Walk.start hne).2

-- Edited to no entries, every level is empty, so `collapse` stores nothing and
-- the tree is empty, as a fresh build of no entries is.
theorem lvl_nil (b : List Entry → Entry → Bool) (H : List Entry → Nat) (j : Nat) : lvl b H [] j = [] := by
  -- No items, no chunks, no references, at every level.
  induction j with
  | zero => rfl
  | succ j ih => simp [lvl, ih, up, chunks, scan]


/-!
## Building in one pass, one open chunk per level

The story, in plain words. `Builder` in `crates/foundation/prolly/src/build.rs`
takes the entries in key order. Each level keeps only its open chunk, which
under Rust's close rule weighs under `MAX_CHUNK_BYTES` or is one item
(`open_bounded`), so the builder holds at most that per level, whatever the
input; the theorems below hold for any close rule. An entry goes into the bottom level; when it closes that
level's chunk, the chunk is stored and its reference goes into the level above
the same way, which may close a chunk there too. At the end (`finish`), from
the bottom up, a level that never closed a chunk is the root, its open chunk
being the whole level; any other level stores its open chunk and feeds that
reference up. What must never happen: the root differs from a fresh build of
the same entries. The theorems below show each level is fed exactly the fresh
level (`lvl`), so the root `collapse` leaves is the fresh one.

What this model leaves out. The model keeps each level's closed chunks, which
Rust only stores and counts. Rust also drops an entry whose key repeats the
one before it (the last value wins) and refuses a smaller key; here the
entries are given sorted with each key once, as in `build_same_entries`.
-/

-- `Builder::feed`: add `x` to the lowest level's open chunk, making the level
-- when there is none. When `x` closes the chunk (`step`'s rule), the chunk is
-- stored and its reference fed to the level above, the same way.
def feed (b : List Entry → Entry → Bool) (H : List Entry → Nat) :
    List (State Entry) → Entry → List (State Entry)
  -- No level yet: a new one, holding `x` open.
  | [], x => [step b ([], []) x]
  | s :: rest, x =>
    -- `x` closes the open chunk: store it, and feed its reference up.
    if b s.2 x && !s.2.isEmpty then (s.1 ++ [s.2 ++ [x]], []) :: feed b H rest (ref H (s.2 ++ [x]))
    -- Otherwise `x` joins the open chunk.
    else (s.1, s.2 ++ [x]) :: rest

/-- What the levels hold after a level was fed the items `ins`: its cutter state
is the cut of `ins`, and the level above (there only once a chunk closed) was
fed the references of the chunks it closed, in order. -/
inductive Fed (b : List Entry → Entry → Bool) (H : List Entry → Nat) : List Entry → List (State Entry) → Prop
  -- Fed nothing: no level.
  | nil : Fed b H [] []
  -- Fed `ins`: this level is the cut of `ins`, and the levels above were fed
  -- the references of its closed chunks.
  | cons (ins : List Entry) (rest : List (State Entry)) (hne : ins ≠ [])
      (hrest : Fed b H ((scan b ([], []) ins).1.map (ref H)) rest) :
      Fed b H ins (scan b ([], []) ins :: rest)

-- Feeding one more item keeps the promise: every level is still the cut of
-- what it was fed, with `x` added at the bottom.
theorem feed_fed (b : List Entry → Entry → Bool) (H : List Entry → Nat) (ins : List Entry)
    (L : List (State Entry)) (h : Fed b H ins L) : ∀ x, Fed b H (ins ++ [x]) (feed b H L x) := by
  -- One level at a time, from the bottom.
  induction h with
  | nil =>
    -- A new level holding `x` alone, which closes nothing.
    intro x
    have h0 : (scan b ([], []) [x]).1 = [] := by simp [scan, step]
    exact Fed.cons [x] [] (by simp) (by rw [h0]; exact Fed.nil)
  | cons ins rest hne hrest ih =>
    intro x
    -- This level's new state is the cut of `ins` with `x` added.
    have hs : scan b ([], []) (ins ++ [x]) = step b (scan b ([], []) ins) x := by
      rw [scan_append]; rfl
    rw [feed]
    by_cases hb : (b (scan b ([], []) ins).2 x && !(scan b ([], []) ins).2.isEmpty) = true
    · -- `x` closes a chunk: the level above is fed its reference.
      rw [if_pos hb]
      have h' : scan b ([], []) (ins ++ [x]) =
          ((scan b ([], []) ins).1 ++ [(scan b ([], []) ins).2 ++ [x]], []) := by
        rw [hs, step, if_pos hb]
      rw [← h']
      refine Fed.cons _ _ (by simp) ?_
      rw [h']
      simpa [List.map_append] using ih (ref H ((scan b ([], []) ins).2 ++ [x]))
    · -- `x` joins the open chunk: nothing above changes.
      rw [if_neg hb]
      have h' : scan b ([], []) (ins ++ [x]) =
          ((scan b ([], []) ins).1, (scan b ([], []) ins).2 ++ [x]) := by
        rw [hs, step, if_neg hb]
      rw [← h']
      refine Fed.cons _ _ (by simp) ?_
      rw [h']
      exact hrest

-- `Builder::push` over all the entries, from no levels.
def pushAll (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) : List (State Entry) :=
  -- Feed each entry in turn.
  xs.foldl (feed b H) []

-- After every push, the bottom level was fed all the entries so far.
theorem pushAll_fed (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry) :
    Fed b H xs (pushAll b H xs) := by
  -- Feeding the entries one at a time keeps the promise from no levels on.
  have key : ∀ (xs ins : List Entry) (L : List (State Entry)), Fed b H ins L →
      Fed b H (ins ++ xs) (xs.foldl (feed b H) L) := by
    intro xs
    induction xs with
    | nil => intro ins L h; simpa using h
    | cons x xs ih =>
      intro ins L h
      have := ih (ins ++ [x]) (feed b H L x) (feed_fed b H ins L h x)
      simpa [List.foldl_cons, List.append_assoc] using this
  simpa using key xs [] [] Fed.nil

-- `Builder::finish`, allowed `n` levels: bottom first, a level that closed no
-- chunk is the root, its open chunk; any other level stores its open chunk, if
-- there is one, and feeds the reference up, and `finish` goes up a level.
-- `none` only when it runs out of levels (`finish_fed` shows it never does).
def fin (b : List Entry → Entry → Bool) (H : List Entry → Nat) :
    Nat → List (State Entry) → Nat → Option (Nat × List Entry)
  -- Out of levels.
  | 0, _, _ => none
  -- No level: an empty tree.
  | _ + 1, [], j => some (j, [])
  | n + 1, s :: rest, j =>
    -- Nothing closed here: the open chunk is the root.
    if s.1 = [] then some (j, s.2)
    -- Something closed: flush the open chunk up, then go up.
    else fin b H n (if s.2 = [] then rest else feed b H rest (ref H s.2)) (j + 1)

-- A level that closed no chunk holds all its items open, in one chunk or none.
theorem none_closed (b : List Entry → Entry → Bool) (X : List Entry) (h : (scan b ([], []) X).1 = []) :
    (scan b ([], []) X).2 = X ∧ (chunks b X).length < 2 := by
  -- Nothing was lost, and nothing was closed.
  have hf := scan_flatten b X [] []
  rw [h] at hf
  simp only [List.flatten_nil, List.nil_append, List.append_nil] at hf
  refine ⟨hf, ?_⟩
  unfold chunks
  cases hs : scan b ([], []) X with
  | mk d c =>
    rw [hs] at h
    simp only at h
    subst h
    cases c <;> simp

-- Flushing a level's open chunk feeds the level above every reference of the
-- level's chunks: the fresh level above.
theorem flush_fed (b : List Entry → Entry → Bool) (H : List Entry → Nat) (ins : List Entry)
    (rest : List (State Entry)) (h : Fed b H ((scan b ([], []) ins).1.map (ref H)) rest) :
    Fed b H (up b H ins) (if (scan b ([], []) ins).2 = [] then rest
      else feed b H rest (ref H (scan b ([], []) ins).2)) := by
  -- The chunks are the closed ones, then the open one if it holds anything.
  rw [up_eq_map]
  unfold chunks
  cases hs : scan b ([], []) ins with
  | mk d c =>
    rw [hs] at h
    cases c with
    | nil => simpa using h
    | cons z zs =>
      simpa [List.map_append] using feed_fed b H _ rest h (ref H (z :: zs))

-- A list among others holds no more items than all of them together.
theorem length_le_flatten (L : List (List α)) (ch : List α) (h : ch ∈ L) :
    ch.length ≤ L.flatten.length := by
  -- Look for `ch` one list at a time.
  induction L with
  | nil => cases h
  | cons l L ih =>
    rw [List.flatten_cons, List.length_append]
    rcases List.mem_cons.1 h with rfl | h
    · omega
    · have := ih h; omega

-- A level that closed a chunk holds two items or more.
theorem two_of_closed (b : List Entry → Entry → Bool) (X : List Entry) (h : (scan b ([], []) X).1 ≠ []) :
    2 ≤ X.length := by
  -- The closed chunk alone holds two, and the closed chunks are part of `X`.
  obtain ⟨ch, hch⟩ := List.exists_mem_of_ne_nil _ h
  have h2 := scan_closed_two b X [] [] (by simp) ch hch
  have hf := congrArg List.length (scan_flatten b X [] [])
  have hle : ch.length ≤ (scan b ([], []) X).1.flatten.length :=
    length_le_flatten _ ch hch
  simp only [List.length_append, List.flatten_nil, List.nil_append, List.length_nil,
    Nat.zero_add] at hf
  omega

/-- `finish` goes up the fresh levels: started at level `i` with the levels fed
the fresh level `i`, it returns a fresh level `k ≥ i` that is one chunk or none,
with that level's node, given more steps than level `i` has items. -/
theorem finish_fed (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hne : xs ≠ []) :
    ∀ n i L, Fed b H (lvl b H xs i) L → (lvl b H xs i).length < n →
      ∃ k, i ≤ k ∧ (chunks b (lvl b H xs k)).length < 2 ∧
        fin b H n L i = some (k, lvl b H xs k) := by
  intro n
  induction n with
  | zero => intro i L _ hn; omega
  | succ n ih =>
    intro i L hF hn
    generalize hX : lvl b H xs i = X at hF hn
    cases hF with
    | nil => exact absurd hX (lvl_ne_nil b H xs hne i)
    | cons _ rest _ hrest =>
      subst hX
      rw [fin]
      by_cases hc : (scan b ([], []) (lvl b H xs i)).1 = []
      · -- Nothing closed: this fresh level is the root, one chunk or none.
        rw [if_pos hc]
        obtain ⟨hopen, hk⟩ := none_closed b _ hc
        exact ⟨i, Nat.le_refl i, hk, by rw [hopen]⟩
      · -- Something closed: the level above was fed the fresh level above,
        -- which is shorter.
        rw [if_neg hc]
        have hF' := flush_fed b H _ rest hrest
        have hshort : (up b H (lvl b H xs i)).length < (lvl b H xs i).length := by
          rw [length_up]
          exact chunks_shrink b _ (two_of_closed b _ hc)
        obtain ⟨k, hik, hk, hfin⟩ := ih (i + 1) _ hF' (by show (up b H _).length < n; omega)
        exact ⟨k, by omega, hk, hfin⟩

/-- `Tree::build` through `Builder`: pushing sorted entries one at a time and
finishing gives the root of a fresh build, after `collapse`. -/
theorem builder_root (b : List Entry → Entry → Bool) (H : List Entry → Nat) (xs : List Entry)
    (hne : xs ≠ []) :
    ∃ k, fin b H (xs.length + 1) (pushAll b H xs) 0 = some (k, lvl b H xs k) ∧
      rootOf b H xs k = ((raise b H xs 0).1, build b H xs) := by
  -- The bottom level was fed every entry, so `finish` stops on a fresh level
  -- that is one chunk or none, and `collapse` comes down to the fresh root.
  obtain ⟨k, _, hk, hfin⟩ :=
    finish_fed b H xs hne (xs.length + 1) 0 _ (pushAll_fed b H xs) (Nat.lt_succ_self _)
  exact ⟨k, hfin, root_from b H xs hne k hk⟩

-- No entries: no level, and `finish` gives the empty tree.
theorem builder_empty (b : List Entry → Entry → Bool) (H : List Entry → Nat) :
    fin b H 1 (pushAll b H []) 0 = some (0, []) := rfl
/-!
## Diff's walk

The story, in plain words. `diff` in `crates/foundation/prolly/src/diff.rs`
walks two trees side by side, each as a stack of what is left to read, in key
order: a subtree not opened yet, or an entry. Each step looks at the two tops.
Two subtrees with the same hash are dropped unread. Otherwise a subtree on top
is opened into its children (the higher of two, or both at the same level). Two
entries on top are compared by key: the smaller is reported as missing on the
other side, and the same key with different values is reported as changed.
What must never happen: the walk reports a key the trees agree on, misses one
they disagree on, or reports the wrong values. The theorems below show the
walk reports exactly `mergeDiff` of the two trees' entries, and that
`mergeDiff` of two sorted entry lists holds exactly the keys whose values
differ, each with its value on both sides.

What this model trusts. A subtree is named by its hash. The store gives the
children under each hash (`kids`) and the entries under it (`content`), and a
well-formed store has a subtree's entries be its children's, read in order.
Two subtrees with the same hash are then the same name, so they hold the same
entries; in Rust that is SHA-256 being collision-free, as above. That the walk
ends is not proved here: each open goes one level down a finite tree, and the
Rust tests run it to the end.
-/

-- One reported key: the key, its value on the left, its value on the right
-- (`none` when that side lacks it), as `Difference` in `diff.rs`.
abbrev Diff := Nat × Option Nat × Option Nat

-- The reference report: merge two entry lists by key, reporting a key on one
-- side only, and a key on both sides with different values.
def mergeDiff : List Entry → List Entry → List Diff
  -- Both empty: nothing to report.
  | [], [] => []
  -- Only the left has entries left: each is missing on the right.
  | x :: xs, [] => (x.1, some x.2, none) :: mergeDiff xs []
  -- Only the right has entries left: each is missing on the left.
  | [], y :: ys => (y.1, none, some y.2) :: mergeDiff [] ys
  | x :: xs, y :: ys =>
    -- The left key is smaller: it is missing on the right.
    if x.1 < y.1 then (x.1, some x.2, none) :: mergeDiff xs (y :: ys)
    -- The right key is smaller: it is missing on the left.
    else if y.1 < x.1 then (y.1, none, some y.2) :: mergeDiff (x :: xs) ys
    -- The same key: reported only when the values differ.
    else (if x.2 = y.2 then [] else [(x.1, some x.2, some y.2)]) ++ mergeDiff xs ys
termination_by xs ys => xs.length + ys.length

-- Entries both sides share at the front are never reported.
theorem mergeDiff_append_same (P A B : List Entry) :
    mergeDiff (P ++ A) (P ++ B) = mergeDiff A B := by
  -- One shared entry at a time: the same key with the same value.
  induction P with
  | nil => rfl
  | cons p P ih => simp [mergeDiff, ih]

-- What is left of a tree to walk: a subtree by its hash and level, or an entry.
inductive Elem where
  -- An entry, read already.
  | entry (e : Entry)
  -- A subtree not opened yet.
  | sub (h : Nat) (level : Nat)

-- The entries a stack still holds, in key order: a subtree stands for the
-- entries under it.
def flat (content : Nat → List Entry) : List Elem → List Entry
  -- An empty stack holds nothing.
  | [] => []
  -- An entry holds itself.
  | .entry e :: s => e :: flat content s
  -- A subtree holds the entries under it.
  | .sub h _ :: s => content h ++ flat content s

-- The entries of two stacks one after the other.
theorem flat_append (content : Nat → List Entry) (l r : List Elem) :
    flat content (l ++ r) = flat content l ++ flat content r := by
  -- One element at a time.
  induction l with
  | nil => rfl
  | cons e l ih => cases e <;> simp [flat, ih, List.append_assoc]

-- Opening a subtree (`Walk::open`) leaves the entries the stack holds alone,
-- in a well-formed store.
theorem flat_open (kids : Nat → List Elem) (content : Nat → List Entry)
    (hk : ∀ h, flat content (kids h) = content h) (a la : Nat) (l : List Elem) :
    flat content (kids a ++ l) = flat content (.sub a la :: l) := by
  -- The children hold what the subtree holds.
  rw [flat_append, hk]
  rfl

-- One step of `diff`'s loop: what it reports, and the two stacks after it;
-- `none` when both stacks are empty and it returns.
def walkStep (kids : Nat → List Elem) (l r : List Elem) :
    Option (List Diff × List Elem × List Elem) :=
  match l, r with
  -- Both empty: done.
  | [], [] => none
  -- Two subtrees: skip both when they are the same, else open the higher, or
  -- both at the same level.
  | .sub a la :: l, .sub c lc :: r =>
    if a = c then some ([], l, r)
    else some ([], if lc ≤ la then kids a ++ l else .sub a la :: l,
      if la ≤ lc then kids c ++ r else .sub c lc :: r)
  -- A subtree on the left only: open it.
  | .sub a _ :: l, r => some ([], kids a ++ l, r)
  -- A subtree on the right only: open it.
  | l, .sub c _ :: r => some ([], l, kids c ++ r)
  -- Two entries: take the smaller key, or both when the keys are the same.
  | .entry x :: l, .entry y :: r =>
    if x.1 < y.1 then some ([(x.1, some x.2, none)], l, .entry y :: r)
    else if y.1 < x.1 then some ([(y.1, none, some y.2)], .entry x :: l, r)
    else some (if x.2 = y.2 then [] else [(x.1, some x.2, some y.2)], l, r)
  -- An entry on the left only: missing on the right.
  | .entry x :: l, [] => some ([(x.1, some x.2, none)], l, [])
  -- An entry on the right only: missing on the left.
  | [], .entry y :: r => some ([(y.1, none, some y.2)], [], r)

-- Whichever subtree a step opens, the stack holds the same entries.
theorem flat_open_if (kids : Nat → List Elem) (content : Nat → List Entry)
    (hk : ∀ h, flat content (kids h) = content h) (p : Prop) [Decidable p]
    (a la : Nat) (l : List Elem) :
    flat content (if p then kids a ++ l else .sub a la :: l) = flat content (.sub a la :: l) := by
  -- Opened or not, it holds the same entries.
  split
  · exact flat_open kids content hk a la l
  · rfl

/-- Each step keeps the promise: what it reports, followed by the reference
report of what is left, is the reference report of what was there before. -/
theorem walkStep_keeps (kids : Nat → List Elem) (content : Nat → List Entry)
    (hk : ∀ h, flat content (kids h) = content h) (l r l' r' : List Elem) (out : List Diff)
    (h : walkStep kids l r = some (out, l', r')) :
    out ++ mergeDiff (flat content l') (flat content r') =
      mergeDiff (flat content l) (flat content r) := by
  -- Look at the step the two tops lead to.
  unfold walkStep at h
  split at h
  · -- Both empty: no step.
    cases h
  · -- Two subtrees.
    rename_i a la l c lc r
    split at h
    · -- The same hash: the same entries in front of both, never reported.
      rename_i hac
      subst hac
      simp only [Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl, rfl⟩ := h
      simp only [flat, List.nil_append, mergeDiff_append_same]
    · -- Different hashes: an open changes neither side's entries.
      simp only [Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl, rfl⟩ := h
      rw [List.nil_append, flat_open_if kids content hk, flat_open_if kids content hk]
  · -- A subtree on the left: opened.
    rename_i a la l r _
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, rfl⟩ := h
    rw [List.nil_append, flat_open kids content hk]
  · -- A subtree on the right: opened.
    rename_i l c lc r _ _
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, rfl⟩ := h
    rw [List.nil_append, flat_open kids content hk]
  · -- Two entries: the step reports what the reference report does first.
    rename_i x l y r
    split at h
    · rename_i hxy
      simp only [Option.some.injEq, Prod.mk.injEq] at h
      obtain ⟨rfl, rfl, rfl⟩ := h
      simp [flat, mergeDiff, hxy]
    · rename_i hxy
      split at h
      · rename_i hyx
        simp only [Option.some.injEq, Prod.mk.injEq] at h
        obtain ⟨rfl, rfl, rfl⟩ := h
        simp [flat, mergeDiff, hxy, hyx]
      · rename_i hyx
        simp only [Option.some.injEq, Prod.mk.injEq] at h
        obtain ⟨rfl, rfl, rfl⟩ := h
        simp only [flat]
        rw [mergeDiff, if_neg hxy, if_neg hyx]
  · -- An entry on the left only.
    rename_i x l
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, rfl⟩ := h
    simp [flat, mergeDiff]
  · -- An entry on the right only.
    rename_i y r
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, rfl⟩ := h
    simp [flat, mergeDiff]

-- A step returns only when both stacks are empty.
theorem walkStep_none (kids : Nat → List Elem) (l r : List Elem)
    (h : walkStep kids l r = none) : l = [] ∧ r = [] := by
  -- Every other case takes a step.
  unfold walkStep at h
  split at h
  · exact ⟨rfl, rfl⟩
  all_goals (try split at h) <;> (try split at h) <;> simp at h

-- `diff`'s loop, run for at most `n` steps: the report once both stacks are
-- empty, or `none` when it has not finished in `n` steps.
def walk (kids : Nat → List Elem) : Nat → List Elem → List Elem → Option (List Diff)
  -- Out of steps.
  | 0, _, _ => none
  | n + 1, l, r =>
    match walkStep kids l r with
    -- Done: nothing more to report.
    | none => some []
    -- A step: its report, then the rest of the loop's.
    | some (out, l', r') => (walk kids n l' r').map (out ++ ·)

/-- Whenever `diff`'s loop finishes, its report is the reference report of the
entries the two stacks held. -/
theorem walk_eq (kids : Nat → List Elem) (content : Nat → List Entry)
    (hk : ∀ h, flat content (kids h) = content h) :
    ∀ n l r res, walk kids n l r = some res →
      res = mergeDiff (flat content l) (flat content r) := by
  -- One step at a time, keeping the promise of `walkStep_keeps`.
  intro n
  induction n with
  | zero => intro l r res h; cases h
  | succ n ih =>
    intro l r res h
    simp only [walk] at h
    split at h
    · -- Done: both stacks are empty, and so is the reference report.
      rename_i hs
      obtain ⟨rfl, rfl⟩ := walkStep_none kids l r hs
      cases h
      simp [flat, mergeDiff]
    · -- A step: its report, then the rest, which is the reference report of
      -- what is left.
      rename_i out l' r' hs
      cases hw : walk kids n l' r' with
      | none => rw [hw] at h; cases h
      | some rest =>
        rw [hw] at h
        cases h
        rw [ih l' r' rest hw]
        exact walkStep_keeps kids content hk l r l' r' out hs

-- A tree's stack at the start (`Walk::new`): its root subtree, or nothing.
def start : Option (Nat × Nat) → List Elem
  -- An empty tree: nothing to walk.
  | none => []
  -- A root: one subtree, by its hash and level.
  | some (h, level) => [.sub h level]

-- `diff`: the same roots report nothing at once; otherwise walk both trees.
def diffTrees (kids : Nat → List Elem) (n : Nat) (left right : Option (Nat × Nat)) :
    Option (List Diff) :=
  -- `left.root == right.root` returns before reading either store.
  if left = right then some [] else walk kids n (start left) (start right)

/-- `diff()` reports the reference report of the two trees' entries, whenever it
finishes, both when the roots are the same and when it walks. -/
theorem diff_eq (kids : Nat → List Elem) (content : Nat → List Entry)
    (hk : ∀ h, flat content (kids h) = content h) (n : Nat) (left right : Option (Nat × Nat))
    (res : List Diff) (h : diffTrees kids n left right = some res) :
    res = mergeDiff (flat content (start left)) (flat content (start right)) := by
  unfold diffTrees at h
  split at h
  · -- The same roots: the same entries, and nothing to report.
    rename_i he
    subst he
    cases h
    have := mergeDiff_append_same (flat content (start left)) [] []
    simp only [List.append_nil] at this
    rw [this, mergeDiff]
  · -- Different roots: the walk.
    exact walk_eq kids content hk n _ _ res h

-- The value of key `k` in an entry list, `none` when it is not there.
def lookup (k : Nat) : List Entry → Option Nat
  -- Not there.
  | [] => none
  -- This entry, or further on.
  | e :: t => if e.1 = k then some e.2 else lookup k t

-- A key smaller than every key of a list is not in it.
theorem lookup_none (k : Nat) (t : List Entry) (h : ∀ e ∈ t, k < e.1) : lookup k t = none := by
  -- No entry has key `k`.
  induction t with
  | nil => rfl
  | cons e t ih =>
    have he := h e (by simp)
    simp only [lookup, if_neg (show e.1 ≠ k by omega)]
    exact ih (fun e' he' => h e' (by simp [he']))

-- In a sorted list every key after the first is larger than it.
theorem sorted_head (x : Entry) (t : List Entry) (h : Sorted (x :: t)) :
    Sorted t ∧ ∀ e ∈ t, x.1 < e.1 := by
  -- That is what `Pairwise` says of the first entry.
  unfold Sorted at h ⊢
  rw [List.pairwise_cons] at h
  exact ⟨h.2, h.1⟩

/-- The reference report means what `diff` promises: for two sorted entry lists
it holds a key with its two values exactly when the values differ, `none`
standing for a side that lacks the key. -/
theorem mergeDiff_spec (xs ys : List Entry) (hx : Sorted xs) (hy : Sorted ys) (d : Diff) :
    d ∈ mergeDiff xs ys ↔
      d.2.1 = lookup d.1 xs ∧ d.2.2 = lookup d.1 ys ∧ d.2.1 ≠ d.2.2 := by
  -- Follow the merge one step at a time.
  obtain ⟨k, l, r⟩ := d
  induction xs, ys using mergeDiff.induct with
  | case1 => simp only [mergeDiff, lookup, List.not_mem_nil, false_iff]; rintro ⟨rfl, rfl, h⟩; exact h rfl
  | case2 x xs ih =>
    -- Only the left: `k` is reported exactly when it is the left's key.
    obtain ⟨hx', hxl⟩ := sorted_head x xs hx
    have := ih hx' hy
    simp only [mergeDiff, List.mem_cons, Prod.mk.injEq, lookup] at this ⊢
    by_cases hk : x.1 = k
    · subst hk
      rw [lookup_none _ xs hxl] at this
      simp only [if_pos rfl]
      constructor
      · rintro (⟨-, rfl, rfl⟩ | hm)
        · simp
        · exact absurd ((this.1 hm).1.trans (this.1 hm).2.1.symm) (this.1 hm).2.2
      · rintro ⟨rfl, rfl, -⟩
        simp
    · simp only [if_neg hk]
      rw [← this]
      constructor
      · rintro (⟨rfl, -, -⟩ | hm)
        · exact absurd rfl hk
        · exact hm
      · exact Or.inr
  | case3 y ys ih =>
    -- Only the right: `k` is reported exactly when it is the right's key.
    obtain ⟨hy', hyl⟩ := sorted_head y ys hy
    have := ih hx hy'
    simp only [mergeDiff, List.mem_cons, Prod.mk.injEq, lookup] at this ⊢
    by_cases hk : y.1 = k
    · subst hk
      rw [lookup_none _ ys hyl] at this
      simp only [if_pos rfl]
      constructor
      · rintro (⟨-, rfl, rfl⟩ | hm)
        · simp
        · exact absurd ((this.1 hm).1.trans (this.1 hm).2.1.symm) (this.1 hm).2.2
      · rintro ⟨rfl, rfl, -⟩
        simp
    · simp only [if_neg hk]
      rw [← this]
      constructor
      · rintro (⟨rfl, -, -⟩ | hm)
        · exact absurd rfl hk
        · exact hm
      · exact Or.inr
  | case4 x xs y ys hlt ih =>
    -- The left key is smaller: it is missing on the right; the rest is merged.
    obtain ⟨hx', hxl⟩ := sorted_head x xs hx
    obtain ⟨_, hyl⟩ := sorted_head y ys hy
    have := ih hx' hy
    rw [mergeDiff, if_pos hlt]
    simp only [List.mem_cons, Prod.mk.injEq, lookup] at this ⊢
    by_cases hk : x.1 = k
    · subst hk
      rw [lookup_none _ xs hxl] at this
      have hyn : y.1 ≠ x.1 := by omega
      have hr : lookup x.1 ys = none := lookup_none _ ys (fun e he => by have := hyl e he; omega)
      simp only [if_pos rfl, if_neg hyn, hr] at this ⊢
      constructor
      · rintro (⟨-, rfl, rfl⟩ | hm)
        · simp
        · exact absurd ((this.1 hm).1.trans (this.1 hm).2.1.symm) (this.1 hm).2.2
      · rintro ⟨rfl, rfl, -⟩
        simp
    · simp only [if_neg hk]
      rw [← this]
      constructor
      · rintro (⟨rfl, -, -⟩ | hm)
        · exact absurd rfl hk
        · exact hm
      · exact Or.inr
  | case5 x xs y ys hlt hgt ih =>
    -- The right key is smaller: it is missing on the left; the rest is merged.
    obtain ⟨_, hxl⟩ := sorted_head x xs hx
    obtain ⟨hy', hyl⟩ := sorted_head y ys hy
    have := ih hx hy'
    rw [mergeDiff, if_neg hlt, if_pos hgt]
    simp only [List.mem_cons, Prod.mk.injEq, lookup] at this ⊢
    by_cases hk : y.1 = k
    · subst hk
      rw [lookup_none _ ys hyl] at this
      have hxn : x.1 ≠ y.1 := by omega
      have hl : lookup y.1 xs = none := lookup_none _ xs (fun e he => by have := hxl e he; omega)
      simp only [if_pos rfl, if_neg hxn, hl] at this ⊢
      constructor
      · rintro (⟨-, rfl, rfl⟩ | hm)
        · simp
        · exact absurd ((this.1 hm).1.trans (this.1 hm).2.1.symm) (this.1 hm).2.2
      · rintro ⟨rfl, rfl, -⟩
        simp
    · simp only [if_neg hk]
      rw [← this]
      constructor
      · rintro (⟨rfl, -, -⟩ | hm)
        · exact absurd rfl hk
        · exact hm
      · exact Or.inr
  | case6 x xs y ys hlt hgt ih =>
    -- The same key: reported when the values differ; the rest is merged.
    obtain ⟨hx', hxl⟩ := sorted_head x xs hx
    obtain ⟨hy', hyl⟩ := sorted_head y ys hy
    have := ih hx' hy'
    have hxy : x.1 = y.1 := by omega
    rw [mergeDiff, if_neg hlt, if_neg hgt]
    simp only [List.mem_append, lookup] at this ⊢
    by_cases hk : x.1 = k
    · subst hk
      rw [lookup_none _ xs hxl, lookup_none _ ys (fun e he => by have := hyl e he; omega)] at this
      simp only [if_pos rfl, if_pos hxy.symm, if_true]
      have hm : (x.1, l, r) ∉ mergeDiff xs ys := fun hm => (this.1 hm).2.2 ((this.1 hm).1.trans (this.1 hm).2.1.symm)
      by_cases hv : x.2 = y.2
      · rw [if_pos hv]
        simp only [List.not_mem_nil, false_or]
        exact ⟨fun h' => absurd h' hm, fun ⟨h1, h2, h3⟩ => (h3 (by rw [h1, h2, hv])).elim⟩
      · simp only [if_neg hv, List.mem_singleton, Prod.mk.injEq]
        constructor
        · rintro (⟨-, rfl, rfl⟩ | h')
          · simpa using hv
          · exact absurd h' hm
        · rintro ⟨rfl, rfl, -⟩
          simp
    · have hk' : y.1 ≠ k := by omega
      simp only [if_neg hk, if_neg hk']
      rw [← this]
      have hn : (k, l, r) ∉ (if x.2 = y.2 then [] else [(x.1, some x.2, some y.2)]) := by
        split <;> simp [Ne.symm hk]
      exact ⟨fun h' => h'.resolve_left hn, Or.inr⟩

/-!
## The node encoding is injective

The story, in plain words. A node's hash is SHA-256 of its bytes
(`Node::encode` in `crates/foundation/prolly/src/node.rs`): the level byte, the item
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
