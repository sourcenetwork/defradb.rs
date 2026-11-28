-- A caller saves the successful answers from a batch of embedding requests.
-- The request is the key. A missing answer leaves earlier answers alone.
-- Correspondence: resolve_embeddings in crates/database/db/src/write/prepare/embedding_batches.rs
-- and PreparedInputMemo::insert in crates/engine/query/src/prepared.rs.

namespace ChunkEmbeddingBatches

-- Store a returned vector only if the request has no answer yet.
def remember (memo : Nat → Option Nat) (key : Nat) (answer : Option Nat) :
    Nat → Option Nat :=
  -- Every other request keeps its current answer.
  fun request => if request = key then (memo request).or answer else memo request

-- Adding an answer leaves an answer already saved under the same key intact.
theorem keeps_first (memo : Nat → Option Nat) (key vector : Nat) (answer : Option Nat)
    (saved : memo key = some vector) : remember memo key answer key = some vector := by
  -- The matching key already has a vector, so it wins.
  simp [remember, saved]

-- A vector for a request with no saved answer becomes available by that key.
theorem stores_answer (memo : Nat → Option Nat) (key vector : Nat)
    (missing : memo key = none) : remember memo key (some vector) key = some vector := by
  -- The matching key has no previous vector to take precedence.
  simp [remember, missing]

-- A batch error cannot clear any answer.
theorem failure_keeps_memo (memo : Nat → Option Nat) (key request : Nat) :
    remember memo key none request = memo request := by
  -- An absent response leaves either branch's existing value in place.
  by_cases sameKey : request = key <;> simp [remember, sameKey]

-- An answer for one key cannot change the answer for a different key.
theorem preserves_other (memo : Nat → Option Nat) (key other : Nat) (answer : Option Nat)
    (distinct : other ≠ key) : remember memo key answer other = memo other := by
  -- A different request takes the unchanged branch.
  simp [remember, distinct]

end ChunkEmbeddingBatches
