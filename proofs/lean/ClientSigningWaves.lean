/-!
The story. A client signs the blocks of its own create over the HTTP signing
WebSocket. The server asks for signatures in numbered waves and keeps a
ledger for the session: how many waves it opened, which wave is open, which
blocks the client has signed, and whether it refused anything. A client may
answer a wave twice, answer a wave nobody asked for, or sign other blocks; a
server retry may ask again for a block already signed. What must never
happen: a block signed twice (its second signature would give the document a
second ID), an answer taken for a wave that is not the open one, or anything
signed once the session has refused.

Blocks are numbers here; a signature "over" a list of blocks stands for the
signatures that verify over exactly those blocks. The ledger is the Rust
`SigningWaves` in crates/surfaces/http/src/client_signing_waves.rs:
`openWave` is `SigningWaves::open`, `answer` is `SigningWaves::answer`.
-/

namespace ClientSigningWaves

-- The session's ledger.
structure Ledger where
  -- How many waves the server opened; the open wave has this number.
  issued : Nat
  -- The blocks of the open wave, or none when no wave is open.
  asked : Option (List Nat)
  -- Every block the client has signed in this session.
  signed : List Nat
  -- Whether the session refused an answer or a wave.
  refused : Bool

-- A wave may be asked only if each of its blocks appears exactly once in
-- the wave and the signed blocks together: never signed, never repeated.
def fresh (blocks signed : List Nat) : Bool :=
  -- Check every block of the wave against the wave plus the signed blocks.
  blocks.all fun b => (blocks ++ signed).count b == 1

-- Open the next wave, or refuse the session if it is refused or a block is not fresh.
def openWave (l : Ledger) (blocks : List Nat) : Ledger :=
  -- A refused session, or a block already signed or repeated, refuses.
  if l.refused || !fresh blocks l.signed then
    -- Nothing is open and the refusal is kept.
    { l with asked := none, refused := true }
  else
    -- The wave gets the next number and waits for its answer.
    { l with issued := l.issued + 1, asked := some blocks }

-- Take the client's answer naming wave `n` with signatures over `over`.
def answer (l : Ledger) (n : Nat) (over : List Nat) : Ledger :=
  -- Look at the open wave, if there is one.
  match l.asked with
  | some blocks =>
    -- Accept only on a live session, for the open wave, over its exact blocks.
    if !l.refused && n == l.issued && over == blocks then
      -- The wave's blocks become signed and the wave closes.
      { l with asked := none, signed := blocks ++ l.signed }
    else
      -- Anything else closes the wave and refuses the session.
      { l with asked := none, refused := true }
  -- An answer with no open wave refuses the session.
  | none => { l with refused := true }

-- A new session: nothing opened, nothing signed, nothing refused.
def empty : Ledger := { issued := 0, asked := none, signed := [], refused := false }

-- The ledger's invariant: every signed block is signed once, and the open
-- wave only holds blocks that are fresh against the signed ones.
def Inv (l : Ledger) : Prop :=
  -- No block is in the signed list twice.
  (∀ b, l.signed.count b ≤ 1) ∧
  -- Every block of the open wave appears once in the wave plus the signed list.
  (∀ blocks, l.asked = some blocks → ∀ b ∈ blocks, (blocks ++ l.signed).count b = 1)

-- A new session satisfies the invariant.
theorem inv_empty : Inv empty := by
  -- Nothing is signed and nothing is open.
  constructor
  · intro b; simp [empty]
  · intro blocks h; simp [empty] at h

-- Appending a fresh wave keeps every block signed at most once.
theorem once_after_fresh (blocks signed : List Nat)
    (once : ∀ b, signed.count b ≤ 1)
    (fresh_wave : ∀ b ∈ blocks, (blocks ++ signed).count b = 1) :
    ∀ b, (blocks ++ signed).count b ≤ 1 := by
  -- Take any block.
  intro b
  -- Either it is in the wave, where freshness says it appears once,
  by_cases h : b ∈ blocks
  · exact Nat.le_of_eq (fresh_wave b h)
  -- or it is not, so it appears only in the signed list.
  · have zero : blocks.count b = 0 := List.count_eq_zero.mpr h
    rw [List.count_append, zero, Nat.zero_add]
    exact once b

-- The Boolean freshness check gives the freshness property.
theorem fresh_spec (blocks signed : List Nat) (h : fresh blocks signed = true) :
    ∀ b ∈ blocks, (blocks ++ signed).count b = 1 := by
  -- Unfold the check: every block's count equals one.
  intro b hb
  simp [fresh, List.all_eq_true] at h
  rw [List.count_append]
  exact h b hb

-- Opening a wave keeps the invariant.
theorem inv_open (l : Ledger) (blocks : List Nat) (inv : Inv l) :
    Inv (openWave l blocks) := by
  -- Split on whether the wave is refused.
  unfold openWave
  split
  -- Refused: the signed list is unchanged and nothing is open.
  · exact ⟨inv.1, by intro bs h; simp at h⟩
  -- Opened: the signed list is unchanged and the wave passed the check.
  · rename_i h
    simp at h
    refine ⟨inv.1, ?_⟩
    intro bs hbs
    simp at hbs
    subst hbs
    exact fresh_spec _ _ (by simpa using h.2)

-- Answering keeps the invariant.
theorem inv_answer (l : Ledger) (n : Nat) (over : List Nat) (inv : Inv l) :
    Inv (answer l n over) := by
  -- Look at the open wave.
  unfold answer
  split
  · rename_i blocks hasked
    split
    -- Accepted: the open wave's fresh blocks join the signed list.
    · refine ⟨once_after_fresh blocks l.signed inv.1 (inv.2 blocks hasked), ?_⟩
      intro bs h; simp at h
    -- Refused: nothing is signed and nothing is open.
    · exact ⟨inv.1, by intro bs h; simp at h⟩
  -- No open wave: nothing changes but the refusal.
  · exact ⟨inv.1, by rename_i h; intro bs h'; simp_all⟩

-- Signed once: in every session the ledger reaches, no block is signed twice.
theorem signed_once (l : Ledger) (inv : Inv l) (b : Nat) : l.signed.count b ≤ 1 :=
  -- The invariant's first half says exactly this.
  inv.1 b

-- Open wave: an answer that signs anything named the open wave, on a live
-- session, with signatures over exactly the blocks asked.
theorem answer_signs_only_the_open_wave (l : Ledger) (n : Nat) (over : List Nat)
    (changed : (answer l n over).signed ≠ l.signed) :
    l.refused = false ∧ n = l.issued ∧ l.asked = some over := by
  -- Only the accepting branch changes the signed list.
  unfold answer at changed
  split at changed
  · rename_i blocks hasked
    split at changed
    · rename_i h
      simp at h
      exact ⟨h.1.1, h.1.2, by rw [hasked, h.2]⟩
    · simp at changed
  · simp at changed

-- Refusal is final: a refused session signs nothing and stays refused.
theorem refusal_final (l : Ledger) (n : Nat) (over blocks : List Nat)
    (refused : l.refused = true) :
    (answer l n over).signed = l.signed ∧ (answer l n over).refused = true ∧
    (openWave l blocks).asked = none ∧ (openWave l blocks).refused = true := by
  -- Every branch of both functions keeps the signed list or refuses.
  unfold answer openWave
  split <;> simp [refused]

-- RED: without the freshness check, a retry that asks for a signed block gets
-- it signed again, so block 0 is signed twice.
def openUnchecked (l : Ledger) (blocks : List Nat) : Ledger :=
  -- The planted bug: open any wave on a live session.
  if l.refused then { l with asked := none, refused := true }
  else { l with issued := l.issued + 1, asked := some blocks }

-- The bug's run: sign block 0, open it again, sign it again.
theorem red_unchecked_signs_twice :
    (answer (openUnchecked (answer (openUnchecked empty [0]) 1 [0]) [0]) 2 [0]).signed.count 0
      = 2 := by
  -- Evaluate the run.
  decide

-- GREEN counterpart: the checked ledger refuses the same retry.
theorem green_checked_refuses_retry :
    (openWave (answer (openWave empty [0]) 1 [0]) [0]).refused = true := by
  -- Evaluate the run.
  decide

end ClientSigningWaves
