/-!
# CursorParity — cursor tokens, the backward boundary and the explain attributes

The story. A client pages through a query with opaque cursor tokens. A Rust
node and a Go Shinzo node must hand out and accept the same tokens, report the
same `hasNext`, and explain a cursor query the same way.

Who acts: the client, which sends `first`/`after` or `last`/`before`; the
cursor node, which builds the page; the index scan beneath it.

What must never happen:
* a token carries a member Go does not write, or loses one it does;
* a page taken `before` a row that is gone says there is a next page;
* a backward page shows its own boundary row;
* explain reports a page size under the wrong direction, or a cursor value
  without a token.

Rust anchors:
* tokens: `crates/cursor/src/lib.rs` `Cursor::encode`, `Cursor::decode`;
* the boundary: `crates/query/src/plan/cursor.rs`
  `populate_backward_buffer_fast`, `at_before_boundary`,
  `populate_backward_buffer_slow`, and the inclusive backward seek in
  `crates/query/src/planner/builder/cursor.rs` `configure_scan_for_cursor`;
* explain: `crates/query/src/plan/cursor.rs` `explain_inner`.

Go anchors (`shinzo-develop` 2a899b860): `internal/cursor/cursor.go`
`CursorPayload`; `internal/planner/cursor.go` `bufferBackwardPage`,
`beforeCursorBoundarySurvives`, `drainBackwardPage`, `simpleExplain`.
-/

namespace CursorParity

/-! ## 1. The token's members -/

/-- What a token says: a DocID and the order values (field name, value). -/
structure Payload where
  -- the row's DocID
  doc : String
  -- the row's order field values; empty when the token has none
  keys : List (String × Nat)
  deriving DecidableEq

/-- One JSON member of the token's object. -/
inductive Member where
  -- `"d": <docID>`
  | d (doc : String)
  -- `"k": {…}`
  | k (keys : List (String × Nat))
  -- `"o": <text>`, which older builds wrote and Go Shinzo does not
  | o (text : String)
  deriving DecidableEq

/-- The members Go Shinzo writes: `d`, then `k` only when there are keys. -/
def encode (p : Payload) : List Member :=
  -- `k` is omitted when empty (`omitempty`)
  if p.keys = [] then [.d p.doc] else [.d p.doc, .k p.keys]

/-- Read a token's members: the last `d` and `k` win, anything else is ignored. -/
def decode : List Member → Payload
  -- no members: no DocID, no keys
  | [] => ⟨"", []⟩
  -- a `d` sets the DocID unless a later one does
  | .d doc :: rest => let p := decode rest; ⟨if p.doc = "" then doc else p.doc, p.keys⟩
  -- a `k` sets the keys unless a later one does
  | .k keys :: rest => let p := decode rest; ⟨p.doc, if p.keys = [] then keys else p.keys⟩
  -- an unknown member is skipped
  | .o _ :: rest => decode rest

/-- A token never carries `o`. -/
theorem encode_has_no_o (p : Payload) (t : String) : Member.o t ∉ encode p := by
  -- both shapes are made of `d` and `k` only
  unfold encode; split <;> simp

/-- Decoding what was encoded gives back the payload. -/
theorem decode_encode (p : Payload) : decode (encode p) = p := by
  -- one case per shape of `encode`
  obtain ⟨doc, keys⟩ := p
  unfold encode
  split <;> simp_all [decode]

/-- An older token with an empty `o` after its members decodes the same. -/
theorem decode_legacy_o (p : Payload) : decode (encode p ++ [.o ""]) = p := by
  -- the trailing `o` adds nothing, so this is `decode_encode`
  obtain ⟨doc, keys⟩ := p
  unfold encode
  split <;> simp_all [decode]

/-- RED: the old encoder, which always wrote an empty `o`. -/
def encodeWithO (p : Payload) : List Member := encode p ++ [.o ""]

/-- RED: the old encoder's token is not Go Shinzo's. -/
theorem neg_old_encoder_differs : ∃ p, encodeWithO p ≠ encode p :=
  -- any payload shows it: the old one is one member longer
  ⟨⟨"doc-1", []⟩, by decide⟩

/-! ## 2. The backward boundary and `hasNext` -/

/-- An index row: its key (the order values and, unless the index is unique,
    the DocID) and its DocID. -/
abbrev Row := Nat × String

/-- The index holds its rows in strictly increasing key order: no two rows share a key. -/
abbrev Ordered (rows : List Row) : Prop := rows.Pairwise (fun a b => a.1 < b.1)

/-- The backward seek from the `before` key `k`, that key included: rows at or
    below `k`, nearest first. -/
def seekIncl (rows : List Row) (k : Nat) : List Row :=
  (rows.filter (fun r => decide (r.1 ≤ k))).reverse

/-- The rows the page may show: strictly below `k`, nearest first. -/
def below (rows : List Row) (k : Nat) : List Row :=
  (rows.filter (fun r => decide (r.1 < k))).reverse

/-- The cursor node: when the seek's first row sits at `k` it is the
    boundary's slot, dropped, and there is a next page only when it is the
    token's row `d`. -/
def page (rows : List Row) (k : Nat) (d : String) : List Row × Bool :=
  match seekIncl rows k with
  -- the first row is at the boundary key: drop it, `hasNext` if it is `d`
  | r :: rs => if r.1 = k then (rs, decide (r.2 = d)) else (r :: rs, false)
  -- nothing at or below the boundary
  | [] => ([], false)

/-- Rows above a key are dropped by any filter that keeps only rows at or below it. -/
theorem filter_above (rows : List Row) (k : Nat) (p : Row → Bool)
    (h : ∀ r ∈ rows, k < r.1) (hp : ∀ r : Row, k < r.1 → p r = false) :
    rows.filter p = [] := by
  -- every row fails `p`
  rw [List.filter_eq_nil_iff]
  intro r hr
  simp [hp r (h r hr)]

/-- Splitting the inclusive seek: the rows below `k`, then at most the one row at `k`. -/
theorem split_at_key : ∀ (rows : List Row) (k : Nat), Ordered rows →
    rows.filter (fun r => decide (r.1 ≤ k))
      = rows.filter (fun r => decide (r.1 < k)) ++ rows.filter (fun r => decide (r.1 = k))
  -- no rows: both sides empty
  | [], _, _ => rfl
  -- a row `r` then `rs`, all of `rs` above `r`
  | r :: rs, k, h => by
    obtain ⟨hr, hrs⟩ := List.pairwise_cons.mp h
    rcases Nat.lt_trichotomy r.1 k with hlt | heq | hgt
    -- `r` below `k`: it leads both sides, the rest splits the same way
    · simp [List.filter_cons, hlt, Nat.le_of_lt hlt, Nat.ne_of_lt hlt, split_at_key rs k hrs]
    -- `r` at `k`: everything after it is above `k`
    · have hab : ∀ x ∈ rs, k < x.1 := fun x hx => heq ▸ hr x hx
      have e1 := filter_above rs k (fun r => decide (r.1 ≤ k)) hab (by intro x hx; simp; omega)
      have e2 := filter_above rs k (fun r => decide (r.1 < k)) hab (by intro x hx; simp; omega)
      have e3 := filter_above rs k (fun r => decide (r.1 = k)) hab (by intro x hx; simp; omega)
      simp [List.filter_cons, heq, e1, e2, e3]
    -- `r` above `k`: so is everything after it, both sides empty
    · have hab : ∀ x ∈ r :: rs, k < x.1 := by
        intro x hx
        rcases List.mem_cons.mp hx with rfl | hx
        · exact hgt
        · exact Nat.lt_trans hgt (hr x hx)
      rw [filter_above _ k _ hab (by intro x hx; simp; omega),
        filter_above _ k _ hab (by intro x hx; simp; omega),
        filter_above _ k _ hab (by intro x hx; simp; omega)]
      rfl

/-- At most one row sits at a key. -/
theorem at_key_le_one : ∀ (rows : List Row) (k : Nat), Ordered rows →
    rows.filter (fun r => decide (r.1 = k)) = [] ∨
      ∃ r, r.1 = k ∧ r ∈ rows ∧ rows.filter (fun r => decide (r.1 = k)) = [r]
  -- no rows: none at `k`
  | [], _, _ => Or.inl rfl
  | r :: rs, k, h => by
    obtain ⟨hr, hrs⟩ := List.pairwise_cons.mp h
    by_cases heq : r.1 = k
    -- `r` at `k`: the rest is above it, so `r` is the only one
    · have hab : ∀ x ∈ rs, k < x.1 := fun x hx => heq ▸ hr x hx
      refine Or.inr ⟨r, heq, List.mem_cons_self _ _, ?_⟩
      rw [List.filter_cons, if_pos (by simp [heq]),
        filter_above rs k _ hab (by intro x hx; simp; omega)]
    -- `r` not at `k`: whatever the rest has
    · rw [List.filter_cons, if_neg (by simp [heq])]
      rcases at_key_le_one rs k hrs with h0 | ⟨x, hx, hm, h1⟩
      · exact Or.inl h0
      · exact Or.inr ⟨x, hx, List.mem_cons_of_mem _ hm, h1⟩

/-- The page shows exactly the rows below the boundary, nearest first: the
    inclusive seek costs nothing in what is shown. -/
theorem page_rows (rows : List Row) (k : Nat) (d : String) (h : Ordered rows) :
    (page rows k d).1 = below rows k := by
  unfold page seekIncl below
  rw [split_at_key rows k h, List.reverse_append]
  rcases at_key_le_one rows k h with h0 | ⟨r, hk, _, h1⟩
  -- nothing at `k`: the first row, if any, is below `k`, so it is kept
  · rw [h0, List.reverse_nil, List.nil_append]
    cases hb : (rows.filter (fun r => decide (r.1 < k))).reverse with
    | nil => rfl
    | cons x xs =>
      have hx : x ∈ rows.filter (fun r => decide (r.1 < k)) := by
        rw [← List.mem_reverse, hb]; exact List.mem_cons_self _ _
      have : x.1 < k := by simpa using (List.mem_filter.mp hx).2
      simp [Nat.ne_of_lt this]
  -- the row at `k` comes first and is dropped
  · rw [h1]; simp [hk]

/-- There is a next page exactly when the token's row is still at its key. -/
theorem page_has_next (rows : List Row) (k : Nat) (d : String) (h : Ordered rows) :
    (page rows k d).2 = true ↔ (k, d) ∈ rows := by
  unfold page seekIncl
  rw [split_at_key rows k h, List.reverse_append]
  rcases at_key_le_one rows k h with h0 | ⟨r, hk, hm, h1⟩
  -- nothing at `k`: no next page, and `(k, d)` is not a row
  · rw [h0, List.reverse_nil, List.nil_append]
    have hn : (k, d) ∉ rows := fun hm =>
      by
        -- `(k, d)` would be a row at `k`, but there is none
        have : (k, d) ∈ rows.filter (fun r : Row => decide (r.1 = k)) :=
          List.mem_filter.mpr ⟨hm, by simp⟩
        rw [h0] at this; simp at this
    cases hb : (rows.filter (fun r => decide (r.1 < k))).reverse with
    | nil => simpa using hn
    | cons x xs =>
      have hx : x ∈ rows.filter (fun r => decide (r.1 < k)) := by
        rw [← List.mem_reverse, hb]; exact List.mem_cons_self _ _
      have : x.1 < k := by simpa using (List.mem_filter.mp hx).2
      simpa [Nat.ne_of_lt this] using hn
  -- the row `r` at `k`: next page iff it is `d`, and `(k, d)` is a row iff `r` is it
  · rw [h1]
    simp only [List.reverse_cons, List.reverse_nil, List.nil_append, List.singleton_append,
      if_pos hk, decide_eq_true_eq]
    constructor
    · intro hd; rw [← hk, ← hd]; exact hm
    · intro hkd
      -- `(k, d)` is a row at `k`, and `r` is the only one
      have : (k, d) ∈ rows.filter (fun r : Row => decide (r.1 = k)) :=
        List.mem_filter.mpr ⟨hkd, by simp⟩
      rw [h1] at this
      have := List.mem_singleton.mp this
      rw [← this]

/-- RED: the old rule, a next page whenever a `before` token is given. -/
def oldHasNext (_rows : List Row) (_k : Nat) (_d : String) : Bool := true

/-- RED: the old rule says there is a next page before a deleted row. -/
theorem neg_old_has_next_wrong :
    ¬ ∀ rows k d, Ordered rows → (oldHasNext rows k d = true ↔ (k, d) ∈ rows) := by
  -- with no rows at all, the old rule still says `true`
  intro hall
  have := (hall [] 45 "eve" List.Pairwise.nil).mp rfl
  simp at this

/-! ## 3. The explain attributes -/

/-- Which way the page runs. -/
inductive Direction where
  | forward
  | backward
  deriving DecidableEq

/-- `first` and `last` as explain shows them: the page size under its own
    direction, nothing under the other. -/
def firstLast (dir : Direction) (limit : Option Nat) : Option Nat × Option Nat :=
  match dir with
  -- `first: n` pages forward
  | .forward => (limit, none)
  -- `last: n` pages backward
  | .backward => (none, limit)

/-- `cursorValue`: the `after` token's payload, else the `before` one's, else nothing. -/
def cursorValue (after before : Option Payload) : Option Payload := after.or before

/-- The page size shows under exactly one of `first` and `last`. -/
theorem first_last_one_side (dir : Direction) (n : Nat) :
    (firstLast dir (some n)).1.isSome ≠ (firstLast dir (some n)).2.isSome := by
  -- one case per direction
  cases dir <;> simp [firstLast]

/-- A cursor value shows exactly when a token was given. -/
theorem cursor_value_iff_token (after before : Option Payload) :
    (cursorValue after before).isSome ↔ (after.isSome ∨ before.isSome) := by
  -- one case per token
  cases after <;> cases before <;> simp [cursorValue]

/-- RED: the old explain, which showed `pageSize` and not under `first`/`last`. -/
def oldFirstLast (_dir : Direction) (_limit : Option Nat) : Option Nat × Option Nat := (none, none)

/-- RED: the old explain shows the page size under neither attribute. -/
theorem neg_old_explain_hides_size :
    ¬ ∀ dir n, (oldFirstLast dir (some n)).1.isSome ≠ (oldFirstLast dir (some n)).2.isSome := by
  -- both sides are `none`, so they are equal
  intro hall
  exact hall .forward 5 rfl

end CursorParity

#print axioms CursorParity.encode_has_no_o
#print axioms CursorParity.decode_encode
#print axioms CursorParity.decode_legacy_o
#print axioms CursorParity.neg_old_encoder_differs
#print axioms CursorParity.page_rows
#print axioms CursorParity.page_has_next
#print axioms CursorParity.neg_old_has_next_wrong
#print axioms CursorParity.first_last_one_side
#print axioms CursorParity.cursor_value_iff_token
#print axioms CursorParity.neg_old_explain_hides_size
