/-!
# Explicit transaction IDs name the worker that owns the transaction

The story, in plain words. A database runs several workers. Each worker hands
out its own transactions and counts them: its first, its second, and so on.
So that nobody has to look up who owns a transaction, the number a worker
gives out is built from two things: how many transactions it has handed out
before (the sequence) and which worker it is.

    id = sequence * workers + worker

To find the owner later, the database takes the remainder after dividing by the
number of workers (`id % workers`). To find the sequence, it takes the quotient
(`id / workers`). This file proves that this really works: for every worker
below `workers`, decoding gives back exactly the worker and the sequence that
went in, so two different (sequence, worker) pairs never share an ID and a
request always lands on the worker that owns its transaction.

Rust owner: crates/db/src/txn/owner/id.rs, `TxnId::new` (encode) and
`TxnId::owner` (decode). The tests that run the same cases against the real
code are in crates/db/src/txn/owner/id.rs and crates/db/src/txn/owner/tests.rs.
Rust checks overflow (`checked_mul`, `checked_add`) and refuses an ID that would
pass `u64::MAX`; this file works on unbounded naturals, so it proves the
arithmetic, and the overflow refusal is the code's own guard.
-/

namespace TxnId

-- The ID worker `worker` of `workers` gives its `sequence`th transaction.
def encode (sequence worker workers : Nat) : Nat :=
  sequence * workers + worker

-- The worker that owns an ID: the remainder after dividing by the worker count.
def owner (id workers : Nat) : Nat :=
  id % workers

-- How many transactions that worker had handed out before this one.
def sequence (id workers : Nat) : Nat :=
  id / workers

-- Promise: decoding the owner of an encoded ID gives back the worker that made it.
theorem owner_encode (seq worker workers : Nat) (h : worker < workers) :
    owner (encode seq worker workers) workers = worker := by
  -- Adding whole multiples of `workers` does not change a remainder, and a
  -- number below `workers` is its own remainder.
  unfold owner encode
  rw [Nat.add_comm, Nat.add_mul_mod_self_right]
  exact Nat.mod_eq_of_lt h

-- Promise: decoding the sequence of an encoded ID gives back the sequence that made it.
theorem sequence_encode (seq worker workers : Nat) (h : worker < workers) :
    sequence (encode seq worker workers) workers = seq := by
  -- Dividing out the whole multiples leaves the sequence; the worker part is
  -- smaller than one whole multiple and contributes nothing.
  unfold sequence encode
  have pos : 0 < workers := Nat.lt_of_le_of_lt (Nat.zero_le worker) h
  rw [Nat.add_comm, Nat.add_mul_div_right _ _ pos, Nat.div_eq_of_lt h, Nat.zero_add]

-- Promise: whatever ID a client sends back, the owner it names is a real worker.
theorem owner_lt (id workers : Nat) (h : 0 < workers) : owner id workers < workers := by
  -- A remainder is always below the divisor.
  exact Nat.mod_lt id h

-- Promise: two different (sequence, worker) pairs never produce the same ID.
theorem encode_injective (s₁ w₁ s₂ w₂ workers : Nat) (h₁ : w₁ < workers) (h₂ : w₂ < workers)
    (same : encode s₁ w₁ workers = encode s₂ w₂ workers) : s₁ = s₂ ∧ w₁ = w₂ := by
  -- Decode both sides: they must agree on the worker and on the sequence.
  constructor
  · rw [← sequence_encode s₁ w₁ workers h₁, same, sequence_encode s₂ w₂ workers h₂]
  · rw [← owner_encode s₁ w₁ workers h₁, same, owner_encode s₂ w₂ workers h₂]

-- Promise: every ID is its decoded sequence and owner put back together.
theorem decode_encode (id workers : Nat) :
    encode (sequence id workers) (owner id workers) workers = id := by
  -- This is the division identity: quotient times divisor plus remainder.
  unfold encode sequence owner
  rw [Nat.mul_comm]
  exact Nat.div_add_mod id workers

end TxnId
