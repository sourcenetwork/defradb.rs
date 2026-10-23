/-!
# DiskAnnCodes: resident DiskANN codes in one flat array

The story, in plain words. A DiskANN index with resident codes loads every
node's PQ code into memory once. The codes sit back to back in one byte array,
and a small table maps each node id to its slot: the code's position divided by
the code length. Reading a code means jumping to `slot * len` and taking `len`
bytes. Every code of one index has the same length, set by the first code
inserted.

What can happen: codes are inserted one at a time, and a code of the wrong
length, or more slots than a `u32` holds, can show up from a malformed store.

What must never happen:
* a read slices past the end of the array (`get_in_bounds`);
* a read returns another node's bytes (`get_returns_inserted`);
* a code of another length slips in and shifts every later slot
  (`insert_refuses_other_length`).

Rust anchors: `Codes::insert` and `Codes::get` in
`crates/db/src/index/vector/engine/diskann/cache.rs`. The id map there is a
`RapidHashMap<NodeId, u32>`, modeled here as a function from id to slot; bytes
are modeled as natural numbers. `started` stands for `!index.is_empty()`: the
map only ever grows, so it is empty exactly until the first insert succeeds.
-/

namespace DiskAnnCodes

-- The flat code table: the id-to-slot map, the bytes, and the code length.
structure Codes where
  -- For each node id, the slot its code sits in, if it has one.
  index : Nat → Option Nat
  -- Every code, back to back.
  bytes : List Nat
  -- The length every code must have.
  len : Nat
  -- Whether any code has been inserted (Rust: the map is not empty).
  started : Bool

-- The table before anything is loaded: no slots, no bytes.
def empty : Codes :=
  -- Nobody has a slot, there are no bytes, and the length is not set yet.
  { index := fun _ => none, bytes := [], len := 0, started := false }

-- The largest slot count a `u32` holds (Rust: `u32::try_from`).
def slotLimit : Nat := 2 ^ 32

-- Insert one code. `none` is the Rust error: a wrong length or a slot past `u32`.
def insert (c : Codes) (id : Nat) (code : List Nat) : Option Codes :=
  -- The first code sets the length; later codes must match it.
  let len := if c.started then c.len else code.length
  -- A code of another length is refused.
  if code.length ≠ len then none
  else
    -- The new code's slot: how many codes of this length the array holds.
    let slot := c.bytes.length / max len 1
    -- A slot that does not fit a `u32` is refused.
    if slotLimit ≤ slot then none
    -- Otherwise the id points at the new slot and the code goes on the end.
    else some { index := fun i => if i = id then some slot else c.index i,
                bytes := c.bytes ++ code, len := len, started := true }

-- Read one code: jump to `slot * len` and take `len` bytes.
-- (Rust slices `&bytes[slot * len..][..len]`, which panics when out of bounds;
-- `get_in_bounds` shows the slice never is.)
def get (c : Codes) (id : Nat) : Option (List Nat) :=
  -- No slot means no code; otherwise the `len` bytes starting at the slot.
  match c.index id with
  | none => none
  | some s => some ((c.bytes.drop (s * c.len)).take c.len)

-- Promise: a code whose length differs from the table's is refused.
theorem insert_refuses_other_length (c : Codes) (id : Nat) (code : List Nat)
    -- Given a table that already holds a code,
    (hs : c.started = true)
    -- and a code of another length,
    (hl : code.length ≠ c.len) :
    -- inserting it fails.
    insert c id code = none := by
  -- The length is the table's, and the length check takes the failure branch.
  simp [insert, hs, hl]

/-! ## The slice is always in bounds -/

-- Every state the table can reach from `empty` by successful inserts.
inductive Reachable : Codes → Prop
  -- The empty table is reachable.
  | empty : Reachable empty
  -- A successful insert into a reachable table gives a reachable table.
  | step (c c' : Codes) (id : Nat) (code : List Nat) :
      Reachable c → insert c id code = some c' → Reachable c'

-- What stays true of every reachable table: before the first insert it is
-- empty, and the array is always `n` whole codes with every slot below `n`.
def Inv (c : Codes) : Prop :=
  -- Not started means no bytes and no slots;
  (c.started = false → c.bytes = [] ∧ ∀ i, c.index i = none) ∧
  -- and some count `n` of codes fills the array exactly, every slot below it.
  ∃ n, c.bytes.length = n * c.len ∧ ∀ i s, c.index i = some s → s < n

-- A count of `n` whole codes divided by the code length is at most `n`.
theorem slot_le (n len : Nat) : n * len / max len 1 ≤ n := by
  -- Dividing by `max len 1` is undoing a multiplication by at least `len`;
  apply Nat.div_le_of_le_mul
  -- and `n * len ≤ max len 1 * n` because `len ≤ max len 1`.
  rw [Nat.mul_comm (max len 1) n]
  -- Multiply both sides of `len ≤ max len 1` by `n`.
  exact Nat.mul_le_mul_left n (Nat.le_max_left len 1)

-- Promise: the empty table satisfies the invariant.
theorem inv_empty : Inv empty := by
  -- No bytes, no slots, and zero codes fill zero bytes.
  refine ⟨fun _ => ⟨rfl, fun _ => rfl⟩, 0, by simp [empty], ?_⟩
  -- No id has a slot, so there is nothing below zero to check.
  intro i s h
  -- The empty map answers `none`, never `some s`.
  simp [empty] at h

-- Promise: a successful insert keeps the invariant.
theorem inv_insert (c c' : Codes) (id : Nat) (code : List Nat)
    -- Given a table that satisfies the invariant
    (hc : Inv c)
    -- and an insert into it that succeeded,
    (h : insert c id code = some c') :
    -- the new table satisfies it too.
    Inv c' := by
  -- Name the parts of the old invariant.
  obtain ⟨hfresh, n, hlen, hslots⟩ := hc
  -- Unfold the insert and look at each of its checks.
  unfold insert at h
  -- Split on whether the table had started.
  cases hst : c.started
  -- Not started: the table was empty, so the new code is the only one.
  · obtain ⟨hb, hi⟩ := hfresh hst
    -- With no bytes the slot is zero; the length is the code's own.
    simp only [hst, hb] at h
    -- Neither check fails; take the success branch.
    simp at h
    -- The new table is exactly the one built by the insert.
    obtain ⟨_, h⟩ := h
    subst h
    -- It has started, one code fills it, and the only slot is zero.
    refine ⟨by simp, 1, by simp, ?_⟩
    -- Look up any id in the new map.
    intro i s hs
    -- The inserted id has slot zero; every other id had none.
    by_cases hid : i = id
    · simp [hid] at hs; omega
    · simp [hid, hi i] at hs
  -- Started: the length is the table's and the new code goes at slot `n` or below.
  · -- The length check passed, or the insert would have failed.
    by_cases hl : code.length = c.len
    · -- The `u32` check passed, or the insert would have failed.
      by_cases hlim : slotLimit ≤ c.bytes.length / max c.len 1
      · simp [hst, hl, hlim] at h
      · -- Take the success branch and name the new table.
        simp [hst, hl, hlim] at h
        -- The new table is exactly the one built by the insert.
        subst h
        -- It has started; `n + 1` codes now fill it.
        refine ⟨by simp, n + 1, ?_, ?_⟩
        · -- The array grew by one code of the table's length.
          simp [hlen, hl, Nat.succ_mul]
        · -- Look up any id in the new map.
          intro i s hs
          -- The new slot is at most `n`, since the array held `n` codes.
          have hle := slot_le n c.len
          -- The inserted id has the new slot; every other id kept its old one.
          by_cases hid : i = id
          · simp [hid, hlen] at hs; omega
          · simp [hid] at hs; have := hslots i s hs; omega
    · -- A code of another length was refused, so this case cannot happen.
      simp [hst, hl] at h

-- Promise: every reachable table satisfies the invariant.
theorem inv_reachable (c : Codes) (h : Reachable c) : Inv c := by
  -- Walk the inserts that built the table, from `empty` forward.
  induction h with
  -- The empty table satisfies it;
  | empty => exact inv_empty
  -- and each successful insert keeps it.
  | step c c' id code _ hins ih => exact inv_insert c c' id code ih hins

-- Promise: in any reachable table, every slot's code lies inside the array,
-- so the Rust slice `&bytes[slot * len..][..len]` never panics.
theorem get_in_bounds (c : Codes) (id s : Nat)
    -- Given a table built by successful inserts
    (hc : Reachable c)
    -- and an id that has slot `s`,
    (hs : c.index id = some s) :
    -- the slot's last byte is inside the array.
    s * c.len + c.len ≤ c.bytes.length := by
  -- The array holds `n` whole codes and `s` is below `n`.
  obtain ⟨_, n, hlen, hslots⟩ := inv_reachable c hc
  -- So `s + 1` codes fit inside `n` codes.
  have hsn := hslots id s hs
  -- Rewrite the array length as `n` codes and compare code counts.
  rw [hlen, ← Nat.succ_mul]
  -- `(s + 1) * len ≤ n * len` because `s + 1 ≤ n`.
  exact Nat.mul_le_mul_right c.len hsn

/-! ## A read returns the code that was inserted -/

-- Insert a list of (id, code) pairs in order; `none` if any insert fails.
def insertAll : Codes → List (Nat × List Nat) → Option Codes
  -- Nothing left to insert: the table as it is.
  | c, [] => some c
  -- Insert the first pair, then the rest into the result.
  | c, (id, code) :: rest =>
    match insert c id code with
    | none => none
    | some c' => insertAll c' rest

-- Taking `n` bytes from position `a` reads the same before and after more
-- bytes are appended, as long as those `n` bytes were already there.
theorem read_unchanged_by_append (xs ys : List Nat) (a n : Nat)
    -- Given that the `n` bytes from `a` lie inside `xs`,
    (h : a + n ≤ xs.length) :
    -- appending `ys` does not change them.
    ((xs ++ ys).drop a).take n = (xs.drop a).take n := by
  -- Dropping `a` from the joined list drops it from `xs` alone,
  rw [List.drop_append_of_le_length (by omega)]
  -- and the first `n` bytes of what is left all come from `xs`.
  exact List.take_append_of_le_length (by simp; omega)

-- What holds after inserting `done` (codes of length `L`, distinct ids) from
-- `empty`: the array is exactly those codes, every slot is below their count,
-- and each of them reads back.
def Holds (c : Codes) (done : List (Nat × List Nat)) (L : Nat) : Prop :=
  -- Nothing done means the table is still empty;
  (done = [] → c.started = false ∧ c.bytes = [] ∧ ∀ i, c.index i = none) ∧
  -- something done means it has started with length `L`;
  (done ≠ [] → c.started = true ∧ c.len = L) ∧
  -- the array is exactly `done.length` codes of length `L`;
  c.bytes.length = done.length * L ∧
  -- every slot is below that count;
  (∀ i s, c.index i = some s → s < done.length) ∧
  -- and every inserted code reads back.
  (∀ p ∈ done, get c p.1 = some p.2)

-- One step: inserting a fresh id with a code of length `L` keeps `Holds`.
theorem holds_insert (c c' : Codes) (done : List (Nat × List Nat)) (L id : Nat)
    (code : List Nat)
    -- Given that `Holds` is true of the table and what was inserted,
    (hc : Holds c done L)
    -- the new id is not among those already inserted,
    (hfresh : ∀ p ∈ done, p.1 ≠ id)
    -- the new code has length `L`,
    (hl : code.length = L)
    -- and the insert succeeded,
    (h : insert c id code = some c') :
    -- `Holds` is true with the new pair on the end.
    Holds c' (done ++ [(id, code)]) L := by
  -- Name the parts of `Holds`.
  obtain ⟨hnil, hcons, hlen, hslots, hget⟩ := hc
  -- Whether anything was inserted before decides where the length comes from.
  by_cases hd : done = []
  · -- Nothing before: the table is empty and this is its first code.
    subst hd
    -- The old table had not started and had no bytes or slots.
    obtain ⟨hst, hb, hi⟩ := hnil rfl
    -- Unfold the insert; the slot is zero and both checks pass.
    simp [insert, hst, hb] at h
    -- The new table is exactly the one built by the insert.
    obtain ⟨_, h⟩ := h
    subst h
    -- Check each part of `Holds` for the one-code table.
    refine ⟨by simp, fun _ => ⟨rfl, hl⟩, by simp [hl], ?_, ?_⟩
    · -- The only slot is zero, below one code.
      intro i s hs
      -- The inserted id has slot zero; every other id has none.
      by_cases hid : i = id
      · simp [hid] at hs; simp; omega
      · simp [hid, hi i] at hs
    · -- The only inserted code reads back: it is the whole array.
      intro p hp
      -- The only pair is the one just inserted.
      simp at hp; subst hp
      -- Slot zero, so the read takes the first `L` bytes, which are the code.
      simp [get, ← hl]
  · -- Something before: the table has started with length `L`.
    obtain ⟨hst, hcl⟩ := hcons hd
    -- The slot of the new code: the array's code count.
    have hslot : c.bytes.length / max c.len 1 = done.length ∨ L = 0 := by
      -- When `L` is zero there is nothing to show;
      by_cases hL : L = 0
      · exact Or.inr hL
      · -- otherwise dividing `done.length * L` by `L` gives `done.length`.
        left
        rw [hlen, hcl, Nat.max_eq_left (by omega)]
        exact Nat.mul_div_cancel _ (by omega)
    -- The slot is at most the code count.
    have hle : c.bytes.length / max c.len 1 ≤ done.length := by
      rw [hlen, hcl]; exact slot_le _ _
    -- The `u32` check passed, or the insert would have failed.
    by_cases hlim : slotLimit ≤ c.bytes.length / max L 1
    · simp [insert, hst, hcl, hl, hlim] at h
    · -- Unfold the insert; both checks pass, so the new table is the one it builds.
      simp [insert, hst, hcl, hl, hlim] at h
      subst h
      -- Check each part of `Holds` for the grown table.
      refine ⟨by simp, fun _ => ⟨rfl, rfl⟩, ?_, ?_, ?_⟩
      · -- The array grew by one code of length `L`.
        simp [hlen, hl, Nat.succ_mul]
      · -- Every slot is below the new count.
        intro i s hs
        -- The new id has the new slot; every other id kept its old one.
        by_cases hid : i = id
        · simp [hid] at hs; rw [hcl] at hle; simp; omega
        · simp [hid] at hs; have := hslots i s hs; simp; omega
      · -- Every inserted code reads back.
        intro p hp
        -- It is either an old pair or the new one.
        rcases List.mem_append.mp hp with hold | hnew
        · -- An old pair: its id is not the new id, so its slot is unchanged.
          have hne := hfresh p hold
          -- Its old read gave its code.
          have hg := hget p hold
          -- Name its slot.
          cases hsp : c.index p.1 with
          | none => simp [get, hsp] at hg
          | some s =>
            -- The old read was the `L` bytes at slot `s`.
            simp [get, hsp, hcl] at hg
            -- Its bytes were already inside the old array.
            have hin : s * L + L ≤ c.bytes.length := by
              have := hslots p.1 s hsp
              rw [hlen, ← Nat.succ_mul]
              exact Nat.mul_le_mul_right L this
            -- So appending the new code leaves its read unchanged.
            simp [get, hne, hsp, read_unchanged_by_append _ _ _ _ hin, hg]
        · -- The new pair: it reads from the new slot, which holds the new code.
          simp at hnew; subst hnew
          -- Either the slot is the old code count, or every code is empty.
          rcases hslot with hs | hL
          · -- The read starts right after the old codes, which is the new code.
            simp only [get, if_true]
            rw [hcl] at hs
            rw [hs, ← hlen, ← hl]
            simp
          · -- Every code is empty, so any read is the empty code.
            have : code = [] := List.eq_nil_of_length_eq_zero (by omega)
            simp [get, hL, this]

-- Promise: inserting codes of one length under distinct ids, each `get`
-- returns the code that was inserted under that id.
theorem get_returns_inserted (entries : List (Nat × List Nat)) (L : Nat) (c : Codes)
    -- Given that no id appears twice,
    (hdistinct : entries.Pairwise (fun a b => a.1 ≠ b.1))
    -- every code has length `L`,
    (hlen : ∀ p ∈ entries, p.2.length = L)
    -- and inserting them all from `empty` succeeded with table `c`,
    (h : insertAll empty entries = some c) :
    -- every pair reads back: `get` returns its code.
    ∀ p ∈ entries, get c p.1 = some p.2 := by
  -- Generalize: from any table that `Holds` for what was done, inserting the
  -- rest gives a table that `Holds` for everything.
  suffices key : ∀ (rest done : List (Nat × List Nat)) (c0 : Codes),
      Holds c0 done L →
      (done ++ rest).Pairwise (fun a b => a.1 ≠ b.1) →
      (∀ p ∈ rest, p.2.length = L) →
      insertAll c0 rest = some c →
      Holds c (done ++ rest) L by
    -- Start from `empty` with nothing done, then read the last part of `Holds`.
    have hh := key entries [] empty
      ⟨fun _ => ⟨rfl, rfl, fun _ => rfl⟩, fun hne => absurd rfl hne, by simp [empty],
        fun _ _ hs => by simp [empty] at hs, fun _ hp => by simp at hp⟩
      (by simpa using hdistinct) hlen h
    -- The read-back part of `Holds` for all entries is the promise.
    simpa using hh.2.2.2.2
  -- Walk the remaining pairs one insert at a time.
  intro rest
  induction rest with
  | nil =>
    -- Nothing left: the table is the one we started from.
    intro done c0 hc _ _ hins
    simp [insertAll] at hins
    subst hins
    simpa using hc
  | cons x rest ih =>
    -- Insert the first remaining pair, then the rest.
    intro done c0 hc hd hl hins
    -- Split the pair into its id and code.
    obtain ⟨id, code⟩ := x
    -- The first insert either failed (impossible here) or gave a table `c1`.
    cases h1 : insert c0 id code with
    | none => simp [insertAll, h1] at hins
    | some c1 =>
      -- The rest was inserted into `c1`.
      simp only [insertAll, h1] at hins
      -- The new id differs from every id already inserted.
      have hfresh : ∀ p ∈ done, p.1 ≠ id := by
        intro p hp
        exact (List.pairwise_append.mp hd).2.2 p hp (id, code) (by simp)
      -- `Holds` survives the first insert.
      have h1' := holds_insert c0 c1 done L id code hc hfresh
        (hl (id, code) (by simp)) h1
      -- The rest then finishes the job, with the pair moved to `done`.
      have := ih (done ++ [(id, code)]) c1 h1'
        (by simpa using hd) (fun p hp => hl p (by simp [hp])) hins
      simpa using this

end DiskAnnCodes
