/-!
# OwedLive: the index of the sequences still owed on one relationship

The story, in plain words. A node keeps, for each relationship (tuple) it
owes a remote ACP provider changes on, one key per change still owed: the
tuple's prefix followed by the change's sequence number as eight big-endian
bytes (`crates/engine/acp/src/outbox/keys.rs`, `live_key`). Staging a change
adds its key; settling it deletes it. A settling pass reads the tuple's keys
in the store's order (bytes compared one by one, the first difference
decides) and takes the first as the oldest change still owed and the last as
the latest (`crates/engine/acp/src/outbox/ops.rs`, `tuple_state`).

What must never happen:

1. two sequences encode to keys whose byte order differs from their numeric
   order, or an encoding does not read back as its number (`order_iff`,
   `decode_encode`);
2. after a settle, the first key read is not the smallest sequence still
   owed, or the last not the largest (`oldest_after_settle`,
   `latest_after_settle`);
3. an index with nothing owed names an oldest or a latest (`empty_index`).

With sequences 10 and 20 owed, settling 10 leaves 20 as the oldest
(`ten_and_twenty`).
-/

namespace OwedLive

-- A byte is a digit below 256. Bytes are compared as numbers.
def Base : Nat := 256

-- How the store orders two byte strings: the first differing byte decides,
-- and a string that ends first is smaller.
def lexLt : List Nat → List Nat → Prop
  | [], [] => False
  | [], _ :: _ => True
  | _ :: _, [] => False
  | a :: as, b :: bs => a < b ∨ (a = b ∧ lexLt as bs)

-- The number a big-endian byte string stands for.
def val : List Nat → Nat
  | [] => 0
  | d :: ds => d * Base ^ ds.length + val ds

-- Every byte of the string is below 256.
def Bytes (l : List Nat) : Prop := ∀ d ∈ l, d < Base

-- The `k` lowest bytes of `n`, most significant first: `to_be_bytes` for
-- `k = 8`.
def encode : Nat → Nat → List Nat
  | 0, _ => []
  | k + 1, n => (n / Base ^ k) % Base :: encode k n

theorem encode_length (k n : Nat) : (encode k n).length = k := by
  induction k with
  | zero => rfl
  | succ k ih => simp [encode, ih]

theorem encode_bytes (k n : Nat) : Bytes (encode k n) := by
  induction k with
  | zero => intro d h; simp [encode] at h
  | succ k ih =>
    intro d h
    simp only [encode, List.mem_cons] at h
    rcases h with rfl | h
    · exact Nat.mod_lt _ (by decide)
    · exact ih d h

-- A string of bytes stands for less than 256 to its length.
theorem val_lt (l : List Nat) (h : Bytes l) : val l < Base ^ l.length := by
  induction l with
  | nil => simp [val]
  | cons d ds ih =>
    have hd : d < Base := h d (by simp)
    have hs : val ds < Base ^ ds.length := ih (fun x hx => h x (by simp [hx]))
    have step : (d + 1) * Base ^ ds.length ≤ Base * Base ^ ds.length :=
      Nat.mul_le_mul_right _ hd
    simp only [val, List.length_cons, Nat.pow_succ]
    have : d * Base ^ ds.length + val ds < (d + 1) * Base ^ ds.length := by
      rw [Nat.add_mul, Nat.one_mul]; omega
    calc d * Base ^ ds.length + val ds < (d + 1) * Base ^ ds.length := this
      _ ≤ Base * Base ^ ds.length := step
      _ = Base ^ ds.length * Base := Nat.mul_comm _ _

-- The digits of `n` below `Base ^ k` read back as `n` modulo `Base ^ k`.
theorem val_encode (k n : Nat) : val (encode k n) = n % Base ^ k := by
  induction k with
  | zero => simp [encode, val, Nat.mod_one]
  | succ k ih =>
    simp only [encode, val, encode_length, ih]
    -- n % B^(k+1) = (n / B^k % B) * B^k + n % B^k
    have h := Nat.mod_mul_left_div_self n (Base ^ k) Base
    rw [Nat.pow_succ, Nat.mod_mul, Nat.mul_comm (n / Base ^ k % Base), Nat.add_comm]

-- Round trip: a sequence below 2^64 encodes to eight bytes that read back
-- as it.
theorem decode_encode (n : Nat) (h : n < Base ^ 8) : val (encode 8 n) = n := by
  rw [val_encode, Nat.mod_eq_of_lt h]

-- Order: two byte strings of one length compare by bytes as their numbers
-- compare.
theorem lex_iff_val (l₁ l₂ : List Nat) (hlen : l₁.length = l₂.length)
    (h₁ : Bytes l₁) (h₂ : Bytes l₂) : lexLt l₁ l₂ ↔ val l₁ < val l₂ := by
  induction l₁ generalizing l₂ with
  | nil =>
    cases l₂ with
    | nil => simp [lexLt, val]
    | cons _ _ => simp at hlen
  | cons a as ih =>
    cases l₂ with
    | nil => simp at hlen
    | cons b bs =>
      simp only [List.length_cons, Nat.add_right_cancel_iff] at hlen
      have ha : a < Base := h₁ a (by simp)
      have hb : b < Base := h₂ b (by simp)
      have has : Bytes as := fun x hx => h₁ x (by simp [hx])
      have hbs : Bytes bs := fun x hx => h₂ x (by simp [hx])
      have vas := val_lt as has
      have vbs := val_lt bs hbs
      rw [hlen] at vas
      have ih' := ih bs hlen has hbs
      simp only [lexLt, val, hlen]
      generalize Base ^ bs.length = P at vas vbs
      constructor
      · rintro (hab | ⟨rfl, hl⟩)
        · have : (a + 1) * P ≤ b * P := Nat.mul_le_mul_right _ hab
          rw [Nat.add_mul, Nat.one_mul] at this
          omega
        · have := ih'.mp hl; omega
      · intro hv
        rcases Nat.lt_trichotomy a b with hab | rfl | hab
        · exact Or.inl hab
        · exact Or.inr ⟨rfl, ih'.mpr (by omega)⟩
        · have : (b + 1) * P ≤ a * P := Nat.mul_le_mul_right _ hab
          rw [Nat.add_mul, Nat.one_mul] at this
          omega

-- Law 1: the keys of two sequences below 2^64 sort as the sequences do.
theorem order_iff (a b : Nat) (ha : a < Base ^ 8) (hb : b < Base ^ 8) :
    lexLt (encode 8 a) (encode 8 b) ↔ a < b := by
  rw [lex_iff_val _ _ (by simp [encode_length]) (encode_bytes 8 a) (encode_bytes 8 b),
    decode_encode a ha, decode_encode b hb]

-- The index as the store hands it out: the sequences still owed, in key
-- order, which by Law 1 is ascending numeric order without repeats.
def Index (l : List Nat) : Prop := l.Pairwise (· < ·)

-- The oldest change still owed: the first key read.
def oldest (l : List Nat) : Option Nat := l.head?

-- The latest change still owed: the last key read.
def latest (l : List Nat) : Option Nat := l.getLast?

-- Law 3: an index with nothing owed names neither.
theorem empty_index : oldest [] = none ∧ latest [] = none := ⟨rfl, rfl⟩

-- In an index the first sequence is the smallest.
theorem oldest_is_min (l : List Nat) (hl : Index l) (a : Nat) (ha : oldest l = some a) :
    a ∈ l ∧ ∀ x ∈ l, a ≤ x := by
  cases l with
  | nil => simp [oldest] at ha
  | cons y ys =>
    simp [oldest] at ha
    subst ha
    refine ⟨by simp, ?_⟩
    intro x hx
    simp only [List.mem_cons] at hx
    rcases hx with rfl | hx
    · exact Nat.le_refl _
    · exact Nat.le_of_lt ((List.pairwise_cons.mp hl).1 x hx)

-- In an index the last sequence is the largest.
theorem latest_is_max (l : List Nat) (hl : Index l) (b : Nat) (hb : latest l = some b) :
    b ∈ l ∧ ∀ x ∈ l, x ≤ b := by
  induction l with
  | nil => simp [latest] at hb
  | cons y ys ih =>
    have hys : Index ys := (List.pairwise_cons.mp hl).2
    have hy : ∀ x ∈ ys, y < x := (List.pairwise_cons.mp hl).1
    cases ys with
    | nil =>
      simp [latest] at hb
      subst hb
      exact ⟨by simp, fun x hx => by simp at hx; omega⟩
    | cons z zs =>
      have hb' : latest (z :: zs) = some b := by
        simpa [latest, List.getLast?_cons_cons] using hb
      obtain ⟨hmem, hmax⟩ := ih hys hb'
      refine ⟨List.mem_cons_of_mem _ hmem, ?_⟩
      intro x hx
      simp only [List.mem_cons] at hx
      rcases hx with rfl | hx
      · exact Nat.le_of_lt (Nat.lt_of_lt_of_le (hy z (by simp)) (hmax z (by simp)))
      · exact hmax x (by simpa using hx)

-- Settling deletes one key: what remains is still an index.
theorem settle_index (l : List Nat) (hl : Index l) (s : Nat) : Index (l.erase s) :=
  hl.sublist (List.erase_sublist _ _)

-- Law 2: after settling any sequence, the first key read is the smallest
-- sequence still owed.
theorem oldest_after_settle (l : List Nat) (hl : Index l) (s a : Nat)
    (ha : oldest (l.erase s) = some a) : a ∈ l.erase s ∧ ∀ x ∈ l.erase s, a ≤ x :=
  oldest_is_min _ (settle_index l hl s) a ha

-- Law 2: after settling any sequence, the last key read is the largest
-- sequence still owed.
theorem latest_after_settle (l : List Nat) (hl : Index l) (s b : Nat)
    (hb : latest (l.erase s) = some b) : b ∈ l.erase s ∧ ∀ x ∈ l.erase s, x ≤ b :=
  latest_is_max _ (settle_index l hl s) b hb

-- The example: with 10 and 20 owed, settling 10 leaves 20 as the oldest,
-- and settling 20 as well leaves nothing.
theorem ten_and_twenty :
    oldest ([10, 20].erase 10) = some 20 ∧ latest ([10, 20].erase 10) = some 20 ∧
    oldest (([10, 20].erase 10).erase 20) = none := by
  decide

end OwedLive
