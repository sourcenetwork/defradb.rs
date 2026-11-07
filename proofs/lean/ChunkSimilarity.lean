/-!
# ChunkSimilarity: ranking articles by their best chunk from the nearest chunks

The story, in plain words. An article's long text is cut into chunks, and each
chunk has its own vector. A similarity query on articles wants the `k` articles
whose best chunk is nearest to the query vector, and gives each article its best
chunk's score.

The fast way asks the vector index for the nearest chunks first. Some of them
are thrown away: a chunk the article's current text no longer uses, a forged
one, or one whose article the reader may not see or the filter rejects. The
rest are "accepted". Once the accepted chunks read so far name at least `k`
different articles, the query stops reading and ranks those articles by the
best accepted chunk it saw for each.

The slow way reads every chunk of every article and does the same ranking.

What must never happen: the fast way returning other articles, or other scores,
than the slow way. It can't, as long as the chunks read so far really are the
nearest ones (every chunk read scores at least as high as every chunk not read).
The promises are `prefix_best_exact`, `unread_ranks_no_higher` and
`prefix_top_is_top`.

Scores are natural numbers here: only their order matters, and larger is nearer.

Code it models:
  crates/query/src/plan/chunk_similarity/best.rs   best_per_parent
  crates/query/src/plan/chunk_similarity/search.rs ChunkSearch::run (stops at k accepted articles)
-/

namespace ChunkSimilarity

-- One chunk the index can return: its article, its score, and whether it is accepted.
structure Chunk where
  -- The article the chunk belongs to.
  parent : Nat
  -- How near the chunk is to the query vector; larger is nearer.
  score : Nat
  -- Whether it passed every check (listed, honest, readable, filtered in).
  ok : Bool

-- Whether chunk `c` is an accepted chunk of article `p`.
def counts (p : Nat) (c : Chunk) : Bool :=
  -- Accepted, and of this article.
  c.ok && c.parent == p

-- An article's best score among chunks `cs`: the highest accepted chunk of it, 0 for none.
def best (p : Nat) : List Chunk → Nat
  -- No chunks, no score.
  | [] => 0
  -- An accepted chunk of `p` competes with the best of the rest; any other chunk is skipped.
  | c :: cs => if counts p c then max c.score (best p cs) else best p cs

-- Article `p` is found in `cs` when some accepted chunk in `cs` belongs to it.
def found (cs : List Chunk) (p : Nat) : Prop :=
  -- There is a chunk in the list that counts for `p`.
  ∃ c ∈ cs, counts p c = true

-- Reading two lists one after the other gives the better of their two bests.
theorem best_append (p : Nat) (a b : List Chunk) :
    -- The best over both lists is the larger of the two bests.
    best p (a ++ b) = max (best p a) (best p b) := by
  -- Walk down the first list.
  induction a with
  -- An empty first list adds nothing: max 0 x = x.
  | nil => simp [best]
  -- A chunk in front either competes or is skipped, on both sides alike.
  | cons c cs ih =>
    -- Unfold one step and split on whether the chunk counts.
    simp only [List.cons_append, best]
    -- Both branches rearrange maxima.
    split <;> simp [ih, Nat.max_assoc]

-- A chunk that counts for `p` scores no higher than `p`'s best.
theorem le_best (p : Nat) (cs : List Chunk) (c : Chunk)
    (hc : c ∈ cs) (hk : counts p c = true) :
    -- Its score is at most the best.
    c.score ≤ best p cs := by
  -- Walk down the list to the chunk.
  induction cs with
  -- The empty list holds no chunk.
  | nil => cases hc
  -- Either this is the chunk, or it is further down.
  | cons d ds ih =>
    -- Unfold one step of `best`.
    simp only [best]
    -- Split on where `c` sits.
    cases List.mem_cons.mp hc with
    -- It is the head: it counts, so it is one side of the max.
    | inl h => subst h; simp only [hk, if_true]; exact Nat.le_max_left _ _
    -- It is further down: the best of the rest bounds it.
    | inr h =>
      -- Whatever the head does, the rest's best is at most the total.
      have := ih h
      -- Both branches keep the rest's best below the result.
      split <;> omega

-- When every chunk scores at most `b`, so does every article's best.
theorem best_le (p : Nat) (cs : List Chunk) (b : Nat)
    (h : ∀ c ∈ cs, c.score ≤ b) :
    -- The best is at most the bound.
    best p cs ≤ b := by
  -- Walk down the list.
  induction cs with
  -- No chunks: the best is 0.
  | nil => simp [best]
  -- The head is bounded, and so is the rest by induction.
  | cons d ds ih =>
    -- The head's bound.
    have hd := h d (List.mem_cons_self d ds)
    -- The rest's bound.
    have hr := ih (fun c hc => h c (List.mem_cons_of_mem d hc))
    -- Unfold and split; each branch is a max of bounded numbers.
    simp only [best]
    split <;> omega

-- An article not found in `cs` has best 0 there.
theorem best_of_not_found (p : Nat) (cs : List Chunk) (h : ¬ found cs p) :
    -- No accepted chunk of it, so nothing to score.
    best p cs = 0 := by
  -- Walk down the list.
  induction cs with
  -- No chunks: 0 by definition.
  | nil => rfl
  -- The head does not count, and the rest is not found either.
  | cons d ds ih =>
    -- The head would make `p` found if it counted.
    have hd : counts p d = false := by
      -- Suppose it counted.
      cases hk : counts p d with
      -- It doesn't: done.
      | false => rfl
      -- It does: then `p` is found, against the hypothesis.
      | true => exact absurd ⟨d, List.mem_cons_self d ds, hk⟩ h
    -- The rest cannot make `p` found either.
    have hr : ¬ found ds p := fun ⟨c, hc, hk⟩ => h ⟨c, List.mem_cons_of_mem d hc, hk⟩
    -- Skip the head and use the rest.
    simp [best, hd, ih hr]

-- The setting every promise below shares: all chunks are the ones read (`pre`)
-- followed by the ones not read (`rest`), and every chunk read scores at least as
-- high as every chunk not read. That is what "nearest first" gives.
def NearestFirst (pre rest : List Chunk) : Prop :=
  -- Each read chunk is at least as near as each unread one.
  ∀ c ∈ pre, ∀ d ∈ rest, d.score ≤ c.score

-- Promise (prefix_best_exact): an article found among the chunks read already has
-- its true best score. Reading further cannot raise it.
theorem prefix_best_exact (pre rest : List Chunk) (p : Nat)
    (near : NearestFirst pre rest) (hp : found pre p) :
    -- The best over everything equals the best over what was read.
    best p (pre ++ rest) = best p pre := by
  -- Take the read chunk that makes `p` found.
  obtain ⟨c, hc, hk⟩ := hp
  -- It scores at most `p`'s best among the read chunks.
  have h1 := le_best p pre c hc hk
  -- Every unread chunk scores at most it, so their best does too.
  have h2 := best_le p rest c.score (fun d hd => near c hc d hd)
  -- Split the total into read and unread parts.
  rw [best_append]
  -- The unread part never wins the max.
  omega

-- Promise (unread_ranks_no_higher): an article not found among the chunks read
-- scores no higher, over all chunks, than any article that was found.
theorem unread_ranks_no_higher (pre rest : List Chunk) (p q : Nat)
    (near : NearestFirst pre rest) (hp : found pre p) (hq : ¬ found pre q) :
    -- The unseen article's true best is at most the seen article's true best.
    best q (pre ++ rest) ≤ best p (pre ++ rest) := by
  -- The seen article's true best is its best among what was read.
  rw [prefix_best_exact pre rest p near hp]
  -- The unseen article has nothing among what was read.
  rw [best_append, best_of_not_found q pre hq]
  -- Take the read chunk that makes `p` found.
  obtain ⟨c, hc, hk⟩ := hp
  -- It bounds every unread chunk, and it is at most `p`'s best.
  have h1 := le_best p pre c hc hk
  have h2 := best_le q rest c.score (fun d hd => near c hc d hd)
  -- So max 0 (unread best) ≤ c.score ≤ p's best.
  omega

-- `top` is a top-k of the articles found in `cs`: every article in it is found,
-- and every found article outside it scores no higher than any article in it.
-- Ties are why this says "a" top-k: any order between equal scores is fine.
def IsTop (cs : List Chunk) (top : List Nat) : Prop :=
  -- Every chosen article is found,
  (∀ p ∈ top, found cs p) ∧
  -- and none left out beats one chosen.
  (∀ p ∈ top, ∀ q, found cs q → q ∉ top → best q cs ≤ best p cs)

-- Promise (prefix_top_is_top): pick the top `k` articles from the chunks read,
-- `k` being however many the query wants. They are a top `k` of the exhaustive
-- answer too, with the very same scores. Reading stops once `k` different
-- articles are found, so `top` can be filled from what was read.
theorem prefix_top_is_top (pre rest : List Chunk) (top : List Nat)
    (near : NearestFirst pre rest) (h : IsTop pre top) :
    -- A top-k of everything, and each chosen article keeps its score.
    IsTop (pre ++ rest) top ∧ ∀ p ∈ top, best p (pre ++ rest) = best p pre := by
  -- Name the two halves of the hypothesis.
  obtain ⟨hfound, hbeat⟩ := h
  -- The scores of chosen articles do not change.
  have same : ∀ p ∈ top, best p (pre ++ rest) = best p pre :=
    fun p hp => prefix_best_exact pre rest p near (hfound p hp)
  -- Assemble the three facts.
  refine ⟨⟨?_, ?_⟩, same⟩
  -- A chosen article found in the read chunks is found in all chunks.
  · intro p hp
    -- Its witness chunk is still there.
    obtain ⟨c, hc, hk⟩ := hfound p hp
    exact ⟨c, List.mem_append_left rest hc, hk⟩
  -- No article left out beats a chosen one.
  · intro p hp q _ hqt
    -- Either the left-out article was seen among the read chunks, or not.
    by_cases hq : found pre q
    -- Seen: its score did not change, and the prefix ranking already put it lower.
    · rw [same p hp, prefix_best_exact pre rest q near hq]
      exact hbeat p hp q hq hqt
    -- Not seen: it scores no higher than any seen article.
    · exact unread_ranks_no_higher pre rest p q near (hfound p hp) hq

end ChunkSimilarity
