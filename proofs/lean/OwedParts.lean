/-!
# OwedParts: the counters a node keeps of what it owes a remote ACP provider

The story, in plain words. A node owes a remote ACP provider relationship
changes (see `proofs/tla/AcpOutbox.tla`). It keeps counts of them: per
relationship (tuple), per document (object), per actor a delete names on a
document, and for the whole node. Two kinds of writer change these counts:

* a **stager**, a transaction that records a new change it owes;
* a **settler**, a pass that deletes the record of a change the provider
  now holds (or that was given up).

Each count is four counters. The first two are the **staged part**, which only
stagers add to; the last two are the **settled part**, which only settlers add
to. What is still owed is the staged count minus the settled count. Every
write is a blind merge that only adds: nobody reads, subtracts and writes back.

What must never happen:

1. two merges folded early give a different count than applied one by one
   (`fold_exact`, `fold_assoc`, `fold_comm`, `apply_fold`);
2. a settler's write changes what a reader of the staged part decided on, or
   a fold hides which part a write changed (`settle_touches_no_staged_part`,
   `touches_fold`), so a settle never aborts a stager or a decision;
3. what is owed goes below zero (`settled_le_staged`);
4. a reader that sees the staged part now and the settled part from earlier
   misses something still owed: it may only deny more (`late_settled_over_owes`).

The Rust this models: `crates/engine/storage/src/backends/regolith/merge/owed.rs`
(`full`, `fold`, `touches`, `delta`, `read`), used by
`crates/engine/acp/src/outbox/ops.rs` (`stage`, `settle`, `owed`).
-/

namespace OwedParts

-- A count is four natural numbers, read by index 0 to 3.
-- Indexes 0 and 1 are the staged part, 2 and 3 the settled part.
abbrev Counters := Fin 4 → Nat

-- The count nobody has written: every counter zero. A key that is absent
-- reads as this, so a store reads the same before and after compaction.
def zero : Counters := fun _ => 0

-- A merge operand is a count to add, in the same four counters.
abbrev Delta := Counters

-- Folding two operands (`partial_merge`): add them counter by counter.
def fold (a b : Delta) : Delta := fun i => a i + b i

-- Applying an operand to a value (`full_merge`, one operand at a time).
def apply (v : Counters) (d : Delta) : Counters := fun i => v i + d i

-- Applying a whole list of operands, oldest first.
def applyAll (v : Counters) : List Delta → Counters
  | [] => v
  | d :: ds => applyAll (apply v d) ds

-- Law 1a: folding two operands first and then applying gives exactly what
-- applying them one after the other gives.
theorem apply_fold (v : Counters) (a b : Delta) :
    apply v (fold a b) = apply (apply v a) b := by
  funext i
  simp [apply, fold, Nat.add_assoc]

-- Law 1b: folding is associative: any grouping of a run folds the same.
theorem fold_assoc (a b c : Delta) : fold (fold a b) c = fold a (fold b c) := by
  funext i
  simp [fold, Nat.add_assoc]

-- Law 1c: folding is commutative: two writers' operands in either order fold
-- the same, so no write order between a stager and a settler matters.
theorem fold_comm (a b : Delta) : fold a b = fold b a := by
  funext i
  simp [fold, Nat.add_comm]

-- Law 1d: the fold is exact: a run folded into one operand and then applied
-- is the run applied operand by operand.
theorem fold_exact (v : Counters) (a : Delta) (ds : List Delta) :
    applyAll v (a :: ds) = applyAll v [ds.foldl fold a] := by
  induction ds generalizing a v with
  | nil => rfl
  | cons d ds ih =>
    simp only [applyAll, List.foldl] at *
    rw [← ih v (fold a d), apply_fold]

-- The parts are the counter indexes. An operand touches a part when it adds
-- something to that counter: a reader that decided on some parts is refused
-- at commit only by an operand that touches one of them.
def touches (d : Delta) (parts : List (Fin 4)) : Prop := ∃ i ∈ parts, d i ≠ 0

-- Law 2a: a fold touches exactly what one of its operands touches, so the
-- answer survives regolith folding operands before the commit check.
theorem touches_fold (a b : Delta) (parts : List (Fin 4)) :
    touches (fold a b) parts ↔ touches a parts ∨ touches b parts := by
  unfold touches fold
  constructor
  · rintro ⟨i, hi, h⟩
    by_cases ha : a i = 0
    · exact Or.inr ⟨i, hi, by omega⟩
    · exact Or.inl ⟨i, hi, ha⟩
  · rintro (⟨i, hi, h⟩ | ⟨i, hi, h⟩)
    · exact ⟨i, hi, by omega⟩
    · exact ⟨i, hi, by omega⟩

-- The staged part: counters 0 and 1.
def staged : List (Fin 4) := [0, 1]

-- A settler's operand: it adds `c` settled changes and `d` settled deletes,
-- to counters 2 and 3 only.
def settle (c d : Nat) : Delta := fun i => if i = 2 then c else if i = 3 then d else 0

-- A stager's operand: it adds `c` staged changes and `d` staged deletes, to
-- counters 0 and 1 only.
def stage (c d : Nat) : Delta := fun i => if i = 0 then c else if i = 1 then d else 0

-- Law 2b: a settler's operand touches no staged part, so a reader of the
-- staged part alone (a stager, a deny decision) is never refused for it.
theorem settle_touches_no_staged_part (c d : Nat) : ¬ touches (settle c d) staged := by
  rintro ⟨i, hi, h⟩
  simp [staged] at hi
  rcases hi with rfl | rfl <;> simp [settle] at h

-- Law 2c: a stager's operand that adds anything does touch the staged part:
-- two stagers of one key, both reading it, are ordered by the commit check,
-- and a staged delete refuses a decision that read the staged part.
theorem stage_touches_staged (c d : Nat) (h : c ≠ 0 ∨ d ≠ 0) : touches (stage c d) staged := by
  rcases h with h | h
  · exact ⟨0, by simp [staged], by simp [stage, h]⟩
  · exact ⟨1, by simp [staged], by simp [stage, h]⟩

-- What is owed: staged minus settled, for changes (counters 0 and 2) and
-- for deletes (counters 1 and 3). Nat subtraction stops at zero.
def owedChanges (v : Counters) : Nat := v 0 - v 2
def owedDeletes (v : Counters) : Nat := v 1 - v 3

-- One write a node makes to a count: a stager adds one change, which may be
-- a delete; a settler settles one change, and only while one is owed.
inductive Write
  | stage (delete : Bool)
  | settle (delete : Bool)

-- The write rule, as `ops::stage` and `ops::settle` keep it: a settle names
-- a change that is still owed, so there is one to settle, and a delete only
-- when a delete is owed.
def allowed (v : Counters) : Write → Prop
  | .stage _ => True
  | .settle false => owedChanges v > 0
  | .settle true => owedChanges v > 0 ∧ owedDeletes v > 0

-- The operand each write merges.
def delta : Write → Delta
  | .stage del => stage 1 (if del then 1 else 0)
  | .settle del => settle 1 (if del then 1 else 0)

-- The invariant: no settled counter is above its staged one.
def Sound (v : Counters) : Prop := v 2 ≤ v 0 ∧ v 3 ≤ v 1

-- The zero count, an absent key, is sound.
theorem zero_sound : Sound zero := by simp [Sound, zero]

-- A run of writes each allowed in the count it is applied to.
def AllowedRun : Counters → List Write → Prop
  | _, [] => True
  | v, w :: ws => allowed v w ∧ AllowedRun (apply v (delta w)) ws

-- Law 3: under the write rule the settled counters never pass the staged
-- ones: what is owed is never negative, so `staged - settled` never
-- truncates a real count.
theorem settled_le_staged (v : Counters) (w : Write)
    (hs : Sound v) (ha : allowed v w) : Sound (apply v (delta w)) := by
  obtain ⟨h2, h3⟩ := hs
  cases w with
  | stage del =>
    cases del <;> simp [Sound, apply, delta, stage] <;> omega
  | settle del =>
    cases del <;> simp [allowed, owedChanges, owedDeletes] at ha <;>
      simp [Sound, apply, delta, settle] <;> omega

-- Law 3, over any run of allowed writes from a sound count, the zero one
-- included.
theorem run_sound (ws : List Write) (v : Counters) (hs : Sound v)
    (hr : AllowedRun v ws) : Sound (applyAll v (ws.map delta)) := by
  induction ws generalizing v with
  | nil => simpa [applyAll] using hs
  | cons w ws ih =>
    simp only [List.map, applyAll]
    exact ih _ (settled_le_staged v w hs hr.1) hr.2

-- Law 4: a reader that sees the staged counters as they are and the settled
-- ones as they were (a settle it has not seen yet only adds to them) finds
-- at least as much owed as there is. A denial that rests on an owed delete
-- is therefore never missed; a late settlement only denies more.
theorem late_settled_over_owes (staged settledNow settledSeen : Nat)
    (late : settledSeen ≤ settledNow) :
    staged - settledNow ≤ staged - settledSeen := by
  omega

-- Law 4, as the decision is made: something owed now is owed as seen.
theorem late_settled_never_misses (staged settledNow settledSeen : Nat)
    (late : settledSeen ≤ settledNow) (owedNow : staged - settledNow > 0) :
    staged - settledSeen > 0 := by
  omega

-- A count with nothing owed reads as the absent key does: whatever was
-- staged has been settled, so a fully settled key that compaction has not
-- dropped answers "nothing owed" exactly as a dropped one would.
theorem fully_settled_owes_nothing (v : Counters) (hc : v 2 = v 0) (hd : v 3 = v 1) :
    owedChanges v = owedChanges zero ∧ owedDeletes v = owedDeletes zero := by
  simp [owedChanges, owedDeletes, zero, hc, hd]

end OwedParts
