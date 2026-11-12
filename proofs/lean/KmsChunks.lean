/-!
# KmsChunks: a split KMS batch asks for every key once, and answers in order

The story, in plain words. A node asks its peers for a batch of keys. Over
iroh gossip one request can carry only so many keys, so the node cuts the
batch into chunks of at most `n` keys, the way Rust's `slice::chunks(n)`
does: the first `n`, then the next `n`, and a shorter last one. It sends
the chunks a few at a time, in waves of `w` chunks, and glues the answers
back together in the same order.

What must never happen:
* a key is asked for twice, left out, or moved (`chunks_flatten`,
  `waves_flatten`);
* a chunk asks for more keys than a request carries (`chunk_size_le`);
* the answers come back in another order than the keys, or with a key's
  answer missing, even when a whole chunk failed and answers each of its
  keys with its error (`answers_in_order`, `answers_length`).

Keys are any values; answers are any values of another type.

Rust anchors: `RemoteKeys::fetch_batch` (`keys.chunks(per_request)`, waves
of `CHUNKS_IN_FLIGHT`) in `crates/kms/src/remote.rs`.
-/

namespace KmsChunks

-- Cut `xs` into chunks of `n`, with `fuel` bounding the steps so the
-- definition is plainly finite: no fuel left, or nothing left, gives no
-- chunks; otherwise take the first `n` and cut the rest.
def chunksFuel {α : Type} (n : Nat) : Nat → List α → List (List α)
  -- Out of fuel: stop.
  | 0, _ => []
  -- Nothing left to cut: no more chunks.
  | _ + 1, [] => []
  -- Something left: one chunk of its first `n`, then the rest cut the same way.
  | f + 1, x :: xs => (x :: xs).take n :: chunksFuel n f ((x :: xs).drop n)

-- The chunks of `xs`: as much fuel as `xs` has keys, which is always enough.
def chunks {α : Type} (n : Nat) (xs : List α) : List (List α) :=
  chunksFuel n xs.length xs

-- Promise: with enough fuel, the chunks glued back together are exactly
-- the keys they were cut from.
theorem chunksFuel_flatten {α : Type} (n : Nat) (hn : 0 < n) :
    ∀ (f : Nat) (xs : List α), xs.length ≤ f → (chunksFuel n f xs).flatten = xs
  -- No fuel: then there were no keys either, and no chunks.
  | 0, xs, h => by
    have hnil : xs = [] := List.eq_nil_of_length_eq_zero (by omega)
    subst hnil
    rfl
  -- No keys: no chunks, nothing glued.
  | _ + 1, [], _ => rfl
  -- Some keys: the first chunk, then the rest, glue back to the whole.
  | f + 1, x :: xs, h => by
    -- Glue the first chunk in front of the rest's chunks.
    simp only [chunksFuel, List.flatten_cons]
    -- The rest's chunks glue back to the rest, which has fewer keys.
    rw [chunksFuel_flatten n hn f]
    -- The first `n` keys followed by the others are all of them.
    · exact List.take_append_drop n (x :: xs)
    -- Taking at least one key leaves the rest within the fuel left.
    · simp only [List.length_drop, List.length_cons] at h ⊢
      omega

-- Promise: the chunks of a batch glue back into the batch: every key once,
-- in its place.
theorem chunks_flatten {α : Type} (n : Nat) (hn : 0 < n) (xs : List α) :
    (chunks n xs).flatten = xs :=
  -- The fuel given is the number of keys, which is enough.
  chunksFuel_flatten n hn xs.length xs (Nat.le_refl _)

-- Promise: no chunk holds more than `n` keys.
theorem chunksFuel_size_le {α : Type} (n : Nat) :
    ∀ (f : Nat) (xs : List α) (c : List α), c ∈ chunksFuel n f xs → c.length ≤ n
  -- No fuel: no chunks to look at.
  | 0, _, c, h => by simp [chunksFuel] at h
  -- No keys: no chunks to look at.
  | _ + 1, [], c, h => by simp [chunksFuel] at h
  -- The first chunk takes at most `n`; the others are the rest's chunks.
  | f + 1, x :: xs, c, h => by
    simp only [chunksFuel, List.mem_cons] at h
    rcases h with hfirst | hrest
    -- The first chunk: taking `n` keys gives at most `n`.
    · rw [hfirst, List.length_take]
      exact Nat.min_le_left _ _
    -- A later chunk: the same promise, one step on.
    · exact chunksFuel_size_le n f _ c hrest

-- Promise: every chunk of a batch fits one request of `n` keys.
theorem chunk_size_le {α : Type} (n : Nat) (xs c : List α) (h : c ∈ chunks n xs) :
    c.length ≤ n :=
  chunksFuel_size_le n xs.length xs c h

-- Answering each chunk key by key and gluing the answers is answering the
-- glued keys key by key.
theorem flatten_map_map {α β : Type} (g : α → β) :
    ∀ L : List (List α), (L.map (List.map g)).flatten = L.flatten.map g
  -- No chunks: no answers either way.
  | [] => rfl
  -- One chunk then the rest: its answers, then the rest's.
  | l :: L => by
    simp only [List.map_cons, List.flatten_cons, List.map_append, flatten_map_map g L]

-- Promise: when each chunk answers its keys in order, the batch's answers
-- are its keys' answers, in the keys' order.
theorem answers_in_order {α β : Type} (n : Nat) (hn : 0 < n) (g : α → β) (xs : List α) :
    ((chunks n xs).map (List.map g)).flatten = xs.map g := by
  -- Glue the answers, then the chunks glue back to the batch.
  rw [flatten_map_map, chunks_flatten n hn]

-- Gluing answers that each keep their chunk's length keeps the total.
theorem flatten_length_of_lengths {α β : Type} (r : List α → List β)
    (hr : ∀ c, (r c).length = c.length) :
    ∀ L : List (List α), ((L.map r).flatten).length = L.flatten.length
  -- No chunks: nothing on either side.
  | [] => rfl
  -- One chunk then the rest: its answers are as many as its keys.
  | l :: L => by
    simp only [List.map_cons, List.flatten_cons, List.length_append, hr l,
      flatten_length_of_lengths r hr L]

-- Promise: however each chunk is answered (its keys' answers, or one error
-- per key when its request failed), as long as it answers each of its keys,
-- the batch has exactly one answer per key.
theorem answers_length {α β : Type} (n : Nat) (hn : 0 < n) (r : List α → List β)
    (hr : ∀ c, (r c).length = c.length) (xs : List α) :
    (((chunks n xs).map r).flatten).length = xs.length := by
  -- Count the answers chunk by chunk, then the chunks glue back to the batch.
  rw [flatten_length_of_lengths r hr, chunks_flatten n hn]

-- Cutting each wave into chunks and gluing all the chunks gives the waves.
theorem flatten_chunks_of_each {α : Type} (n : Nat) (hn : 0 < n) :
    ∀ L : List (List α), ((L.map (chunks n)).flatten).flatten = L.flatten
  -- No waves: nothing.
  | [] => rfl
  -- One wave then the rest: its chunks glue back to it.
  | l :: L => by
    simp only [List.map_cons, List.flatten_cons, List.flatten_append,
      chunks_flatten n hn, flatten_chunks_of_each n hn L]

-- Promise: sending the chunks in waves of `w` chunks (waves of `n * w`
-- keys, each cut into chunks of `n`) still asks for every key once, in
-- order.
theorem waves_flatten {α : Type} (n w : Nat) (hn : 0 < n) (hw : 0 < w) (xs : List α) :
    (((chunks (n * w) xs).map (chunks n)).flatten).flatten = xs := by
  -- Each wave's chunks glue back to the wave, and the waves to the batch.
  rw [flatten_chunks_of_each n hn, chunks_flatten (n * w) (Nat.mul_pos hn hw)]

end KmsChunks
