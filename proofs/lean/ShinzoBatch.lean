/-!
# ShinzoBatch: the inline CAR store keeps its byte count, and a split batch keeps its documents

The story, in plain words. A node speaking Go Shinzo's wire keeps CARs
that came inline with updates, each under a key (its head and the peer that
offered it) with a size in bytes. It keeps a running byte count instead of
adding the sizes up every time, and refuses a CAR that would take that count
past its budget. A kept CAR leaves the store when it is used or expires.

When it publishes its own batch of documents, a batch whose estimated size
is over the message limit is cut in half, and each half again, until every
part fits or holds one document.

What must never happen:
* the running byte count drifts from the sizes really kept, or goes over the
  budget (`offer_valid`, `take_valid`);
* a used CAR stays in the store (`take_removes`);
* a split loses, repeats or reorders a document (`split_keeps_documents`);
* a split part is over the limit while it still holds two documents or more
  (`split_parts_fit`).

Rust anchors: `InlineCars::offer`, `InlineCars::take`, `InlineCars::expire`
and `encode_batch` in `crates/p2p/src/sync/shinzo_batch.rs`.
-/

namespace ShinzoBatch

-- A kept CAR: its key and its size in bytes. A key stands for the pair
-- (head, offering peer); only equality of keys matters here.
abbrev Car := Nat × Nat

-- The sum of the sizes in a list of kept CARs.
def total (cs : List Car) : Nat := (cs.map Prod.snd).sum

-- The CARs not kept under key `k`.
def others (k : Nat) (cs : List Car) : List Car := cs.filter (fun c => c.1 != k)

-- The CARs kept under key `k` (at most one in practice; the proofs do not
-- need that).
def under (k : Nat) (cs : List Car) : List Car := cs.filter (fun c => c.1 == k)

-- The store: the kept CARs and the running byte count.
structure Store where
  -- The kept CARs.
  cars : List Car
  -- The running byte count, updated step by step as the code does.
  bytes : Nat

-- A store is valid under budget `b` when its count is exactly the sizes it
-- keeps and is within the budget.
def Valid (b : Nat) (st : Store) : Prop := st.bytes = total st.cars ∧ st.bytes ≤ b

-- Offer a CAR of size `s` under key `k`, as `InlineCars::offer` does: refuse
-- it if it does not fit on top of the count; otherwise add its size, keep it
-- in place of any CAR under the same key, and take that CAR's size off.
def offer (b : Nat) (st : Store) (k s : Nat) : Store :=
  if st.bytes + s ≤ b then
    { cars := (k, s) :: others k st.cars, bytes := st.bytes + s - total (under k st.cars) }
  else st

-- Remove the CAR under key `k`, as `take` (on use) and `expire` (on age) do,
-- taking its size off the count.
def remove (st : Store) (k : Nat) : Store :=
  { cars := others k st.cars, bytes := st.bytes - total (under k st.cars) }

-- Splitting a list by any test adds its sizes up in two parts.
theorem total_split (k : Nat) :
    ∀ cs : List Car, total cs = total (others k cs) + total (under k cs) := by
  intro cs
  induction cs with
  | nil => simp [total, others, under]
  | cons c cs ih =>
    -- Each CAR lands in exactly one of the two parts.
    by_cases h : c.1 = k
    · have hne : (c.1 != k) = false := by simp [h]
      have heq : (c.1 == k) = true := by simp [h]
      simp only [total, others, under, List.filter_cons, hne, heq] at ih ⊢
      simp at ih ⊢
      omega
    · have hne : (c.1 != k) = true := by simp [h]
      have heq : (c.1 == k) = false := by simp [h]
      simp only [total, others, under, List.filter_cons, hne, heq] at ih ⊢
      simp at ih ⊢
      omega

-- Offering a CAR keeps a valid store valid: the count is still exact and
-- still within the budget.
theorem offer_valid (b : Nat) (st : Store) (k s : Nat) (hv : Valid b st) :
    Valid b (offer b st k s) := by
  obtain ⟨hexact, hbudget⟩ := hv
  unfold offer
  split
  · -- Kept: the old CAR's bytes come off, the new one's go on.
    rename_i hfit
    have hsplit := total_split k st.cars
    constructor
    · simp [total] at hsplit ⊢
      simp [total] at hexact
      omega
    · show st.bytes + s - total (under k st.cars) ≤ b
      omega
  · -- Refused: nothing changes.
    exact ⟨hexact, hbudget⟩

-- Removing a CAR, used or expired, keeps a valid store valid.
theorem take_valid (b : Nat) (st : Store) (k : Nat) (hv : Valid b st) :
    Valid b (remove st k) := by
  obtain ⟨hexact, hbudget⟩ := hv
  have hsplit := total_split k st.cars
  constructor
  · show st.bytes - total (under k st.cars) = total (others k st.cars)
    omega
  · show st.bytes - total (under k st.cars) ≤ b
    omega

-- After a CAR is used, nothing is kept under its key any more, so it is
-- never used again.
theorem take_removes (st : Store) (k : Nat) : under k (remove st k).cars = [] := by
  simp [remove, under, others, List.filter_filter]

-- Splitting a batch, as `encode_batch` does: while the estimate `est` of a
-- part is over `limit` and it holds two documents or more, cut it in half.
def split (est : List α → Nat) (limit : Nat) (l : List α) : List (List α) :=
  if _h : limit < est l ∧ 1 < l.length then
    split est limit (l.take (l.length / 2)) ++ split est limit (l.drop (l.length / 2))
  else [l]
termination_by l.length
decreasing_by
  all_goals simp_wf
  all_goals omega

-- Gluing the parts back together gives the batch: no document is lost,
-- repeated or moved.
theorem split_keeps_documents (est : List α → Nat) (limit : Nat) :
    ∀ l : List α, (split est limit l).flatten = l := by
  intro l
  induction l using (measure List.length).wf.induction with
  | h l ih =>
    rw [split]
    split
    · rename_i h
      have h1 : (l.take (l.length / 2)).length < l.length := by simp; omega
      have h2 : (l.drop (l.length / 2)).length < l.length := by simp; omega
      simp [List.flatten_append, ih _ h1, ih _ h2]
    · simp

-- Every part fits the limit, or holds a single document (or none).
theorem split_parts_fit (est : List α → Nat) (limit : Nat) :
    ∀ l : List α, ∀ p ∈ split est limit l, est p ≤ limit ∨ p.length ≤ 1 := by
  intro l
  induction l using (measure List.length).wf.induction with
  | h l ih =>
    intro p hp
    rw [split] at hp
    split at hp
    · rename_i h
      have h1 : (l.take (l.length / 2)).length < l.length := by simp; omega
      have h2 : (l.drop (l.length / 2)).length < l.length := by simp; omega
      rcases List.mem_append.mp hp with hp | hp
      · exact ih _ h1 p hp
      · exact ih _ h2 p hp
    · rename_i h
      simp at hp
      subst hp
      omega

end ShinzoBatch
