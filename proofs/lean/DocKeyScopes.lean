/-!
The story: one write encrypts some fields of a document. Each field asks for a
key by its scope: `none` is the document's own key, `some f` is the dedicated
key of field `f`. The KMS hands out a fresh key each time it is asked, so the
write keeps a memo from scope to key and asks the KMS only for a scope it has
not seen. What must never happen: two fields under document encryption get
different document keys (Go keeps one key per document), or two different
scopes end up sharing one key.

Keys are modelled as the counter value of the KMS call that made them, so a
fresh key is a number no earlier key has.
Rust owner: crates/db/src/write/prepare.rs, the `generated` memo in the KMS
key preparation of a document write.
-/

namespace DocKeyScopes

-- A key scope: `none` for the document key, `some field` for a field key.
abbrev Scope := Option String

-- The memo: which key a scope already has, if any.
abbrev Memo := Scope → Option Nat

-- What the write carries between fields: the memo and the next fresh key.
structure St where
  memo : Memo
  next : Nat

-- One field's key: reuse the scope's key, or ask for a fresh one and remember it.
def step (st : St) (s : Scope) : Nat × St :=
  match st.memo s with
  | some k => (k, st)
  | none =>
    (st.next, { memo := fun t => if t = s then some st.next else st.memo t,
                next := st.next + 1 })

-- Every field of the write in order: the keys they got and the final memo.
def run : St → List Scope → List Nat × St
  | st, [] => ([], st)
  | st, s :: rest =>
    let (k, st') := step st s
    let (ks, final) := run st' rest
    (k :: ks, final)

-- The memo only names keys already handed out, and no two scopes share one.
def Inv (st : St) : Prop :=
  (∀ s k, st.memo s = some k → k < st.next) ∧
  (∀ s t k, st.memo s = some k → st.memo t = some k → s = t)

-- A later memo keeps every scope's key: keys are never replaced.
def Extends (a b : St) : Prop := ∀ s k, a.memo s = some k → b.memo s = some k

-- The empty memo holds the invariant.
theorem inv_empty (n : Nat) : Inv { memo := fun _ => none, next := n } := by
  -- Nothing is in the memo, so both clauses hold vacuously.
  constructor <;> intros <;> simp_all

-- One step keeps the invariant, keeps old keys, and records the key it returned.
theorem step_ok (st : St) (s : Scope) (h : Inv st) :
    Inv (step st s).2 ∧ Extends st (step st s).2 ∧ (step st s).2.memo s = some (step st s).1 := by
  -- Split on whether the scope already had a key.
  unfold step
  cases hm : st.memo s with
  | some k =>
    -- Reuse: the state is unchanged and the memo already names this key.
    simp only [hm]
    exact ⟨h, fun _ _ x => x, by simp [hm]⟩
  | none =>
    -- Fresh key: it is `next`, which no earlier key equals.
    simp only [hm]
    obtain ⟨hlt, hinj⟩ := h
    refine ⟨⟨?_, ?_⟩, ?_, ?_⟩
    · -- Every remembered key stays below the advanced counter.
      intro t k ht
      dsimp only at ht ⊢
      by_cases e : t = s
      · simp [e] at ht; omega
      · simp [e] at ht; have := hlt t k ht; omega
    · -- The fresh key belongs to one scope; old keys stay one-to-one.
      intro a b k ha hb
      dsimp only at ha hb
      by_cases ea : a = s <;> by_cases eb : b = s
      · rw [ea, eb]
      · simp [ea] at ha; simp [eb] at hb; have := hlt b k hb; omega
      · simp [ea] at ha; simp [eb] at hb; have := hlt a k ha; omega
      · simp [ea] at ha; simp [eb] at hb; exact hinj a b k ha hb
    · -- An old scope keeps its key: it is not the new scope, which had none.
      intro t k ht
      by_cases e : t = s
      · rw [e] at ht; simp [hm] at ht
      · simp [e, ht]
    · -- The new scope now names the fresh key.
      simp

-- A whole run keeps the invariant and only extends the memo.
theorem run_ok (st : St) (l : List Scope) (h : Inv st) :
    Inv (run st l).2 ∧ Extends st (run st l).2 := by
  -- Induct on the fields, carrying the state.
  induction l generalizing st with
  | nil => exact ⟨h, fun _ _ x => x⟩
  | cons s rest ih =>
    obtain ⟨h1, e1, _⟩ := step_ok st s h
    obtain ⟨h2, e2⟩ := ih (step st s).2 h1
    exact ⟨h2, fun t k x => e2 t k (e1 t k x)⟩

-- Each field's key is the key its scope has in the final memo.
theorem key_in_final (st : St) (l : List Scope) (h : Inv st) :
    ∀ i (hi : i < l.length) (hk : i < (run st l).1.length),
      (run st l).2.memo (l.get ⟨i, hi⟩) = some ((run st l).1.get ⟨i, hk⟩) := by
  -- Induct on the fields; the head's key survives the rest of the run.
  induction l generalizing st with
  | nil => intro i hi; simp at hi
  | cons s rest ih =>
    obtain ⟨h1, _, hs⟩ := step_ok st s h
    obtain ⟨_, e2⟩ := run_ok (step st s).2 rest h1
    intro i hi hk
    cases i with
    | zero => simp [run]; exact e2 s _ hs
    | succ j =>
      simp [run] at hk ⊢
      exact ih (step st s).2 h1 j (by simp at hi; omega) hk

-- Fields under the same scope get the same key: one document key per document.
theorem same_scope_same_key (l : List Scope) (i j : Nat)
    (hi : i < l.length) (hj : j < l.length)
    (ki : i < (run { memo := fun _ => none, next := 0 } l).1.length)
    (kj : j < (run { memo := fun _ => none, next := 0 } l).1.length)
    (same : l.get ⟨i, hi⟩ = l.get ⟨j, hj⟩) :
    (run { memo := fun _ => none, next := 0 } l).1.get ⟨i, ki⟩ =
    (run { memo := fun _ => none, next := 0 } l).1.get ⟨j, kj⟩ := by
  -- Both keys are what the final memo names for that one scope.
  have a := key_in_final _ l (inv_empty 0) i hi ki
  have b := key_in_final _ l (inv_empty 0) j hj kj
  rw [same] at a
  rw [a] at b
  exact Option.some.inj b

-- Fields under different scopes get different keys: a field key is never the document key.
theorem different_scope_different_key (l : List Scope) (i j : Nat)
    (hi : i < l.length) (hj : j < l.length)
    (ki : i < (run { memo := fun _ => none, next := 0 } l).1.length)
    (kj : j < (run { memo := fun _ => none, next := 0 } l).1.length)
    (diff : l.get ⟨i, hi⟩ ≠ l.get ⟨j, hj⟩) :
    (run { memo := fun _ => none, next := 0 } l).1.get ⟨i, ki⟩ ≠
    (run { memo := fun _ => none, next := 0 } l).1.get ⟨j, kj⟩ := by
  -- The final memo is one-to-one, so equal keys would mean equal scopes.
  intro eq
  have a := key_in_final _ l (inv_empty 0) i hi ki
  have b := key_in_final _ l (inv_empty 0) j hj kj
  rw [eq] at a
  exact diff ((run_ok _ l (inv_empty 0)).1.2 _ _ _ a b)

end DocKeyScopes
