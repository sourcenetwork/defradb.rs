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

Rust owner: crates/database/db/src/txn/owner/id.rs, `TxnId::new` (encode) and
`TxnId::owner` (decode). The tests that run the same cases against the real
code are in crates/database/db/src/txn/owner/id.rs and crates/database/db/src/txn/owner/tests.rs.
This file works on unbounded naturals; the last section proves that the route
Rust builds stays below 2^32, because a worker that runs out of routes moves to
the next start token instead.
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

/-!
## The start token and the JSON-number bound

The number a client sees is the route above packed under a start token: the
token times 2^32 plus the route. The token is below 2^21 and the route below
2^32, so the packed number stays below 2^53, the largest range a JavaScript
JSON number holds exactly. Unpacking takes the quotient and remainder by 2^32
and gives back the token and route that went in.

Rust owner: crates/database/db/src/txn/owner/id.rs, `TxnId::start_token`,
`TxnId::new`, `TxnId::parse` and `Display for TxnId`.
-/

-- 2^32: one past the largest route.
def routeSpan : Nat := 4294967296

-- 2^21: one past the largest start token.
def tokenSpan : Nat := 2097152

-- 2^53: one past the largest whole number a JSON number holds exactly.
def jsonSafe : Nat := 9007199254740992

-- The wire number: the start token above the route.
def pack (token route : Nat) : Nat :=
  token * routeSpan + route

-- Promise: the remainder by 2^32 of a packed number is its route.
theorem route_pack (token route : Nat) (h : route < routeSpan) :
    pack token route % routeSpan = route := by
  -- Whole multiples of 2^32 leave no remainder, and the route is its own.
  unfold pack
  rw [Nat.add_comm, Nat.add_mul_mod_self_right]
  exact Nat.mod_eq_of_lt h

-- Promise: the quotient by 2^32 of a packed number is its start token.
theorem token_pack (token route : Nat) (h : route < routeSpan) :
    pack token route / routeSpan = token := by
  -- The route is smaller than one 2^32 and adds nothing to the quotient.
  unfold pack
  have pos : 0 < routeSpan := by unfold routeSpan; omega
  rw [Nat.add_comm, Nat.add_mul_div_right _ _ pos, Nat.div_eq_of_lt h, Nat.zero_add]

-- Promise: every packed number is below 2^53, so a JSON number reader keeps it exact.
theorem pack_json_safe (token route : Nat) (ht : token < tokenSpan) (hr : route < routeSpan) :
    pack token route < jsonSafe := by
  -- The largest token times 2^32, plus a route below 2^32, is still below 2^53.
  unfold pack routeSpan tokenSpan jsonSafe at *
  omega

/-!
## When a worker runs out of routes

A worker's routes stop at 2^32. Instead of refusing the next transaction, the
worker keeps counting its sequence and moves to the next start token. The
sequences that fit under one token are `perToken = 2^32 / workers`; a sequence
uses route `(sequence % perToken) * workers + worker` under the token
`sequence / perToken` steps after the first. Tokens cycle through 1 up to
2^21 - 1, never the legacy 0. This section proves the route still fits in 32
bits and still names its worker, that two sequences under the same token never
share a route, and that the token stays a nonzero number below 2^21.

Rust owner: crates/database/db/src/txn/owner/id.rs, `TxnId::new` and
`TxnId::advance`.
-/

-- How many sequences of one worker fit under one start token.
def perToken (workers : Nat) : Nat :=
  routeSpan / workers

-- The route a worker's `sequence`th transaction uses under its token.
def rolledRoute (seq worker workers : Nat) : Nat :=
  encode (seq % perToken workers) worker workers

-- The token `steps` tokens after `token`, cycling through 1 .. 2^21 - 1.
def advance (token steps : Nat) : Nat :=
  ((token - 1) % (tokenSpan - 1) + steps % (tokenSpan - 1)) % (tokenSpan - 1) + 1

-- Promise: a rolled route stays below 2^32, so it still packs under a token.
theorem rolled_route_lt (seq worker workers : Nat) (h : worker < workers)
    (hp : 0 < perToken workers) : rolledRoute seq worker workers < routeSpan := by
  -- The sequence part is at most perToken - 1, so the route is below
  -- perToken * workers, which is at most 2^32.
  unfold rolledRoute encode
  have hm : seq % perToken workers < perToken workers := Nat.mod_lt _ hp
  have hle : seq % perToken workers + 1 ≤ perToken workers := hm
  have hspan : perToken workers * workers ≤ routeSpan := by
    unfold perToken
    exact Nat.div_mul_le_self routeSpan workers
  have step : seq % perToken workers * workers + worker <
      (seq % perToken workers + 1) * workers := by
    rw [Nat.succ_mul]
    omega
  have grow : (seq % perToken workers + 1) * workers ≤ perToken workers * workers :=
    Nat.mul_le_mul_right workers hle
  omega

-- Promise: a rolled route still names the worker that drew it.
theorem rolled_owner (seq worker workers : Nat) (h : worker < workers) :
    owner (rolledRoute seq worker workers) workers = worker := by
  -- The rolled route is an ordinary encoded ID, so the first promise applies.
  unfold rolledRoute
  exact owner_encode _ worker workers h

-- Promise: two sequences of one worker under the same token never share a route.
theorem rolled_injective (s₁ s₂ worker workers : Nat) (h : worker < workers)
    (sameToken : s₁ / perToken workers = s₂ / perToken workers)
    (same : rolledRoute s₁ worker workers = rolledRoute s₂ worker workers) : s₁ = s₂ := by
  -- Equal routes give equal remainders; with equal quotients the division
  -- identity rebuilds the same sequence on both sides.
  unfold rolledRoute at same
  have rem := (encode_injective _ worker _ worker workers h h same).1
  rw [← Nat.div_add_mod s₁ (perToken workers), ← Nat.div_add_mod s₂ (perToken workers),
    sameToken, rem]

-- Promise: a rolled token is never the legacy token 0.
theorem advance_pos (token steps : Nat) : 0 < advance token steps := by
  -- It is some remainder plus one.
  unfold advance
  omega

-- Promise: a rolled token stays below 2^21, so the packed ID stays JSON-safe.
theorem advance_lt (token steps : Nat) : advance token steps < tokenSpan := by
  -- A remainder by 2^21 - 1 is at most 2^21 - 2; adding one stays below 2^21.
  unfold advance
  have pos : 0 < tokenSpan - 1 := by unfold tokenSpan; omega
  have := Nat.mod_lt (((token - 1) % (tokenSpan - 1) + steps % (tokenSpan - 1))) pos
  omega

end TxnId
