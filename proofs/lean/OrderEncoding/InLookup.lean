import OrderEncoding.Bytes

/-!
# OrderEncoding.InLookup — `_in` point lookups run in key order, each once

The story. A query asks for `age _in [55, 25, 45, 35, 25]` on an indexed field,
ordered by age. The planner trusts the index to hand rows back already in order,
so it sorts nothing itself. The index answers an `_in` with one point lookup per
value. If those lookups run in the order the filter wrote them, the rows come
back as 55, 25, 45, 35: out of order, and 25 twice. That must never happen.

The fix (`sort_in_key_order` in
`crates/query/src/planner/index_selection/filter_to_scan.rs`, called from
`filter_to_index_scan`) encodes every value into its index key, sorts the keys
in byte order (the same `memcmp` order the store keeps keys in), drops a key
equal to the one before it, and reverses the list when the scan runs backward.

What this file promises:
* the lookup list is strictly increasing in key order (so rows come back in
  index order and no key is looked up twice), `lookups_strictly_increasing`;
* reversed for a backward scan, it is strictly decreasing, `reversed_lookups_strictly_decreasing`;
* no value the filter asked for is lost, and nothing new appears, `mem_lookups`;
* each key appears once, `lookups_nodup`.

Why key order is the requested order: `OrderEncoding.lean` proves the ascending
encoding preserves value order and the descending one inverts it, so byte order
on keys is the index's own order, ASC or DESC.

Model notes. A key is `Bytes` from `OrderEncoding.Bytes`, ordered by `Lex`
(`memcmp`). The Rust code sorts then deduplicates neighbours; the model inserts
each key into an already-sorted list, skipping an equal one. Both produce the
strictly increasing list of the same keys, which is all the theorems below state.
A value that fails to encode is kept after the encoded ones in Rust; the model
covers only encoded keys.
-/

namespace OrderEncoding

/-! ## Byte order can always be decided -/

/-- Any two keys compare one way, the other way, or are equal. -/
theorem Lex.trichotomy : ∀ (xs ys : Bytes), xs <ₗ ys ∨ xs = ys ∨ ys <ₗ xs
  -- two empty keys are equal
  | [], [] => Or.inr (Or.inl rfl)
  -- the empty key comes before any non-empty key
  | [], _ :: _ => Or.inl Lex.nil
  -- and any non-empty key comes after the empty one
  | _ :: _, [] => Or.inr (Or.inr Lex.nil)
  -- two non-empty keys: compare first bytes, then the rests
  | a :: xs, b :: ys => by
    -- three cases on the first bytes
    rcases Nat.lt_trichotomy a b with h | h | h
    -- first byte smaller: this key is smaller
    · exact Or.inl (Lex.head h)
    -- first bytes equal: the rests decide
    · subst h
      -- compare the rests the same way
      rcases Lex.trichotomy xs ys with h' | h' | h'
      -- rest smaller: key smaller
      · exact Or.inl (Lex.cons h')
      -- rest equal: keys equal
      · exact Or.inr (Or.inl (by rw [h']))
      -- rest larger: key larger
      · exact Or.inr (Or.inr (Lex.cons h'))
    -- first byte larger: this key is larger
    · exact Or.inr (Or.inr (Lex.head h))

/-- A computer can check `xs <ₗ ys`, so the lookup builder can branch on it. -/
instance Lex.decidable : (xs ys : Bytes) → Decidable (xs <ₗ ys)
  -- two empty keys: neither is smaller
  | [], [] => isFalse (fun h => nomatch h)
  -- empty before non-empty
  | [], _ :: _ => isTrue Lex.nil
  -- non-empty is never before empty
  | _ :: _, [] => isFalse (fun h => nomatch h)
  -- two non-empty keys
  | a :: xs, b :: ys =>
    -- first byte smaller: yes
    if hab : a < b then isTrue (Lex.head hab)
    -- first bytes equal: ask about the rests
    else if e : a = b then
      match Lex.decidable xs ys with
      -- rests ordered: keys ordered
      | isTrue h => isTrue (e ▸ Lex.cons h)
      -- rests not ordered: keys not ordered
      | isFalse h => isFalse (fun h' => by
          -- a smaller first byte is ruled out, so the rests must be ordered
          rcases Lex.cons_inv h' with hl | ⟨_, ht⟩
          · exact hab hl
          · exact h ht)
    -- first byte larger: no
    else isFalse (fun h' => by
      -- either first byte smaller or equal, both ruled out
      rcases Lex.cons_inv h' with hl | ⟨he, _⟩
      · exact hab hl
      · exact e he)

/-! ## The lookup list -/

/-- "Strictly increasing": every key is before every key after it. -/
abbrev Increasing (ks : List Bytes) : Prop := ks.Pairwise Lex

/-- Put `k` into a sorted list at its place, or leave the list alone when `k`
    is already there. -/
def insertKey (k : Bytes) : List Bytes → List Bytes
  -- nothing there yet: the list is just `k`
  | [] => [k]
  -- `k` is before the head: it goes first
  | x :: xs => if k <ₗ x then k :: x :: xs
    -- `k` is the head: it is already there, look it up once
    else if k = x then x :: xs
    -- `k` is after the head: keep the head, place `k` in the rest
    else x :: insertKey k xs

/-- The lookups for the filter's keys, in key order, each once. -/
def lookups (ks : List Bytes) : List Bytes := ks.foldr insertKey []

/-- Placing `k` keeps exactly the old keys plus `k`. -/
theorem mem_insertKey (k y : Bytes) : ∀ xs, y ∈ insertKey k xs ↔ y = k ∨ y ∈ xs
  -- into nothing: the only key is `k`
  | [] => by simp [insertKey]
  -- into `x :: xs`: one case per branch of `insertKey`
  | x :: xs => by
    unfold insertKey
    -- `k` before the head: the list gains `k` in front
    by_cases h1 : k <ₗ x
    · simp [h1]
    -- `k` is the head: the list already holds it
    · by_cases h2 : k = x
      · subst h2; simp [h1]
      -- `k` after the head: the rest gains `k`, by the same fact one level down
      · simp only [h1, h2, if_false, List.mem_cons, mem_insertKey k y xs]
        constructor
        · rintro (h | h | h) <;> simp [h]
        · rintro (h | h | h) <;> simp [h]

/-- Placing a key into an increasing list keeps it increasing. -/
theorem insertKey_increasing (k : Bytes) : ∀ xs, Increasing xs → Increasing (insertKey k xs)
  -- a single key is increasing
  | [], _ => by simp [insertKey]
  -- `x :: xs` is increasing: `x` is before all of `xs`, and `xs` is increasing
  | x :: xs, h => by
    obtain ⟨hx, hxs⟩ := List.pairwise_cons.mp h
    unfold insertKey
    by_cases h1 : k <ₗ x
    -- `k` in front: it is before `x`, and through `x` before all of `xs`
    · simp only [h1, if_true]
      refine List.pairwise_cons.mpr ⟨?_, List.pairwise_cons.mpr ⟨hx, hxs⟩⟩
      intro y hy
      rcases List.mem_cons.mp hy with rfl | hy
      · exact h1
      · exact Lex.trans h1 (hx y hy)
    · by_cases h2 : k = x
      -- `k` already there: the list did not change
      · subst h2; simp only [h1, if_false, if_true]
        exact List.pairwise_cons.mpr ⟨hx, hxs⟩
      -- `k` goes into the rest: `x` is before `k`, since `k` is neither before `x` nor `x`
      · simp only [h1, h2, if_false]
        refine List.pairwise_cons.mpr ⟨?_, insertKey_increasing k xs hxs⟩
        intro y hy
        rcases (mem_insertKey k y xs).mp hy with rfl | hy
        · rcases Lex.trichotomy y x with h | h | h
          · exact absurd h h1
          · exact absurd h h2
          · exact h
        · exact hx y hy

/-! ## The promises -/

/-- The lookup list is strictly increasing in key order. -/
theorem lookups_strictly_increasing : ∀ ks, Increasing (lookups ks)
  -- no keys: nothing to order
  | [] => List.Pairwise.nil
  -- one more key placed into an increasing list stays increasing
  | k :: ks => insertKey_increasing k _ (lookups_strictly_increasing ks)

/-- A backward scan reverses the list, which is then strictly decreasing. -/
theorem reversed_lookups_strictly_decreasing (ks : List Bytes) :
    (lookups ks).reverse.Pairwise (fun a b => b <ₗ a) :=
  -- reversing flips every pair
  List.pairwise_reverse.mpr (lookups_strictly_increasing ks)

/-- Every key the filter asked for is looked up, and no other. -/
theorem mem_lookups (y : Bytes) : ∀ ks, y ∈ lookups ks ↔ y ∈ ks
  -- no keys: nothing looked up
  | [] => by simp [lookups]
  -- one more key: it joins the looked-up set
  | k :: ks => by
    show y ∈ insertKey k (lookups ks) ↔ y ∈ k :: ks
    rw [mem_insertKey, mem_lookups y ks, List.mem_cons]

/-- Each key is looked up once: a repeated `_in` value yields one row. -/
theorem lookups_nodup (ks : List Bytes) : (lookups ks).Nodup :=
  -- two keys in increasing order are never equal, since no key is before itself
  (lookups_strictly_increasing ks).imp (fun {a b} (h : a <ₗ b) (e : a = b) => by
    -- an equal pair would put a key before itself
    subst e; exact Lex.irrefl h)

/-! ## Red: the bugs these rule out -/

/-- RED, the original bug: running the lookups as written. `[55, 25]` (as
    one-byte keys) is not increasing, so the rows came back out of order. -/
theorem neg_written_order_not_increasing : ¬ Increasing [[55], [25]] := by
  -- the pair says 55 is before 25 in byte order, which is false
  intro h
  have h' := List.rel_of_pairwise_cons h (List.mem_singleton_self _)
  rcases Lex.cons_inv h' with hl | ⟨he, _⟩
  · exact absurd hl (by decide)
  · exact absurd he (by decide)

/-- A broken placer that keeps a duplicate key instead of skipping it. -/
def insertKeyKeepDup (k : Bytes) : List Bytes → List Bytes
  -- nothing there: just `k`
  | [] => [k]
  -- `k` not after the head: it goes first, even when equal
  | x :: xs => if x <ₗ k then x :: insertKeyKeepDup k xs else k :: x :: xs

/-- RED, without deduplication: a repeated `25` is looked up twice and the list
    is no longer strictly increasing, so the row would come back twice. -/
theorem neg_keep_dup_not_increasing : ¬ Increasing (insertKeyKeepDup [25] [[25]]) := by
  -- the result is `[[25], [25]]`, and 25 is not before itself
  intro h
  -- either branch builds `[[25], [25]]`
  rw [show insertKeyKeepDup [25] [[25]] = [[25], [25]] by simp [insertKeyKeepDup]] at h
  exact Lex.irrefl (List.rel_of_pairwise_cons h (List.mem_singleton_self _))

end OrderEncoding
