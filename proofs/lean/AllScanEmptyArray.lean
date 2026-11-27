/-!
# AllScanEmptyArray: an index scan on a JSON array answers exactly what it promises

The story, in plain words. A JSON index stores one entry per scalar in the
value, each with its path. A null at a path is stored as a null entry. An empty
array has no element to store, so the index used to write nothing for it, as if
the path were missing. (A typed array field already keeps its empty arrays under
its null entry, and `AllEmptyArray` covers how its scan finds them.) An array
element that is itself an object or an array is not a scalar: it has no entry at
the element path.

`{custom: {tags: {_all: {_eq: 1}}}}` asks for documents whose array at `tags`
has every element equal to 1. An empty array has no element that breaks that,
so it matches (Go's `allSlice` over nothing is true); a null array does not.
The index scan for `_all` looks up the entries whose value passes the
condition, so it never saw the empty arrays.

The fix gives an empty array an entry of its own at its path, the empty
marker, and the `_all` scan also looks the marker up. A scan of a positive
condition (`_eq`, `_in`, a range, `_like`, `_ilike`) skips the marker: no
element of an empty array passes `_any`, `_eq`, a range or a `_like`. A
negative condition (`_ne`, `_nin`, `_nlike`, `_nilike`) is different in two
ways. Bare, it passes an empty array (`[] _ne 5` is true). At the top level its
scan reads every entry, markers included (`neg_scan_finds_empty`); at a path
inside the value an array or object has its entries deeper than the path, so
the query reads every document. Under `_all` or `_any` it passes an element that is an object
or an array, which has no entry, so the index cannot answer it and the planner
sends the filter to a full scan. The same goes for an array operator nested in
another (`_all{_all}`).

Model limit: one array level. An element is a scalar (`some v`) or not
(`none`); a nested array operator is out of the model because the planner never
sends it to the index.

Who acts: the writer, which turns a document's array into entries, and a scan,
which turns entries back into the documents it hands to the filter.

What must never happen:
- with a positive predicate, a document that matches `_all` is missing from the
  `_all` scan (`all_scan_complete`);
- a document the `_all` scan finds has neither a passing element nor an empty
  array, so the scan did more than it needed (`all_scan_sound`);
- a value scan hands back an empty array (`value_scan_skips_empty`), or `_any`
  with a positive predicate misses one of its matches (`any_scan_complete`);
- a bare negative scan misses an empty array, which it passes
  (`neg_scan_finds_empty`);
- a null array matches `_all` (`nil_not_all`);
- two empty arrays clash in a unique index (`empty_arrays_never_clash`);
- with `_all` on two paths of one JSON index, the scan reads one path and the
  marker lookup another, so a match whose scanned array is `[]` is lost
  (`same_path_scan_complete`);
- a full scan reads one path but tests its values with the `_like` pattern of
  another path's condition, so a match is lost (`pinned_scan_complete`);
- a range, `_eq` or `_in` scan on one path tests its values with a `_like`
  pattern of another path, so a match is lost (`narrowed_scan_complete`);
- a full scan reads the path of a bare `_nin`, which a document without that
  path passes, so the document is lost (`chosen_scan_complete`);
- bounds from two `_any` clauses are joined into one range, which no single
  element of a match may fall in (`separate_any_complete`).

The RED side: with the old writer, which stored nothing for `[]`, the same
scan misses an empty array that matches (`neg_no_entry_misses_empty`). Routing
`_all{_ne}` to the index misses `[{"a": 1}]`, whose only element has no entry
(`neg_all_negative_misses_inner`). A bare `_ne` scan that skips the marker
misses `[]` (`neg_skip_marker_misses_empty`). A scan that reads one path but
looks the marker up at another misses `{"a": [1], "b": []}`
(`neg_split_paths_miss_empty`). A scan that reads the last condition's path but
tests the first condition's pattern misses `{"a": "xa", "b": 5}` under
`{a: {_like: "x%"}, b: {_ne: 25}}` (`neg_mixed_scan_misses`). A range scan on
`a` that tests `b`'s pattern on `a`'s values misses `{"a": 15, "b": "xa"}`
under `{a: {_gt: 10}, b: {_like: "x%"}}` (`neg_unguarded_scan_misses`). A scan
pinned to `_nin` on `a` misses `{"b": "xa"}` under
`{a: {_nin: [25]}, b: {_like: "x%"}}` (`neg_nin_pin_misses`). Joining
`{_any: {_gt: 10}}` and `{_any: {_lt: 20}}` from two clauses misses `[5, 30]`
(`neg_joined_any_misses`).

Rust anchors:
- the writer: `extract_json_leaves` (`crates/building-blocks/document/src/normal.rs`);
- the `_all` scan: `filter_to_index_scan`
  (`crates/engine/query/src/planner/index_selection/filter_to_scan.rs`), with
  `all_path` and `with_json_empty_arrays`
  (`crates/engine/query/src/planner/index_selection/empty_array.rs`);
- the bare negative scan: `filter_to_index_scan`, where a top-level one is a
  `PrefixScan` that reads every entry, markers included, and `is_full_scan`
  (`crates/engine/query/src/planner/index_selection/conditions.rs`), which
  keeps a bare negative at a path from driving a scan;
- the full-scan fallback for a negative op under `_all` or `_any`:
  `should_fallback_to_full_scan` (`conditions.rs`), and for a nested array
  operator `FieldCondition::nested_array_op` (`types.rs`);
- the skip: `RangeIterator::next` (`crates/engine/storage/src/index/range_iterator.rs`),
  which drops the marker from every bounded scan;
- the pinned path: `filter_to_index_scan`, where the first `_like`-family
  condition (or a top-level `_ne` or `_nin`) gives the scan both its path and its value filter, and
  a narrowing scan keeps the value filter only when it names the scan's path;
- the choice of that condition: `full_scan_at` in `filter_to_index_scan`, with
  `is_full_scan` and `accepts_missing` (`conditions.rs`), which skip a bare `_nin` while
  another condition can pin the scan, and `in_range`, which joins bounds only
  within one `_any` clause (`FieldCondition::clause`);
- the unique exemption: `skips_uniqueness` (`crates/engine/storage/src/index/mod.rs`).
-/

namespace AllScanEmptyArray

-- One index entry of the array at a JSON path. `V` is the scalar type.
inductive Entry (V : Type) where
  -- a null: the value at the path is null
  | null
  -- the empty marker: the array has no elements
  | empty
  -- one scalar element of the array
  | val (v : V)
  deriving DecidableEq

-- The array of a document: `none` for a null array, `some xs` for the list `xs`.
-- An element is `some v` for a scalar and `none` for an object or an array.
abbrev Arr (V : Type) := Option (List (Option V))

-- What the writer stores for an array.
def entries {V : Type} : Arr V → List (Entry V)
  -- a null array: one null entry
  | none => [.null]
  -- an empty array: one empty marker
  | some [] => [.empty]
  -- otherwise one entry per scalar element; a non-scalar element has none
  | some (x :: xs) => (x :: xs).filterMap fun e => e.map .val

-- What the old writer stored: nothing at all for an empty array.
def oldEntries {V : Type} : Arr V → List (Entry V)
  -- a null array: one null entry
  | none => [.null]
  -- an empty array: no entry, like a missing path
  | some [] => []
  -- otherwise one entry per scalar element
  | some (x :: xs) => (x :: xs).filterMap fun e => e.map .val

-- `_all p`: a null array fails, an empty one passes, otherwise every element passes.
def allMatch {V : Type} (p : Option V → Bool) : Arr V → Bool
  -- a null array never matches
  | none => false
  -- every element, which is none at all for `[]`
  | some xs => xs.all p

-- `_any p`: some element passes.
def anyMatch {V : Type} (p : Option V → Bool) : Arr V → Bool
  -- a null array never matches
  | none => false
  -- at least one element passes
  | some xs => xs.any p

-- A value scan over `p`: the document has a scalar entry whose value passes.
-- The null entry and the empty marker are never part of it.
def valueScan {V : Type} (p : Option V → Bool) (es : List (Entry V)) : Bool :=
  -- look for a passing scalar entry
  es.any fun e => match e with
    -- a scalar: keep it when it passes
    | .val v => p (some v)
    -- a null or the empty marker: skipped
    | _ => false

-- Whether an entry is the empty marker.
def isEmpty {V : Type} : Entry V → Bool
  -- the marker
  | .empty => true
  -- a null or an element
  | _ => false

-- The `_all` scan: the value scan, plus a lookup of the empty marker.
def allScan {V : Type} (p : Option V → Bool) (es : List (Entry V)) : Bool :=
  -- a scalar that passes, or the marker
  valueScan p es || es.any isEmpty

-- The scan of a bare negative condition: it reads the marker like `_all` does.
def negScan {V : Type} (p : Option V → Bool) (es : List (Entry V)) : Bool :=
  -- a scalar that passes, or the marker
  allScan p es

-- A bare `_ne n` on a list of numbers: nothing in the list equals `n`.
def bareNe (n : Nat) (xs : List Nat) : Bool :=
  -- an empty list passes
  !xs.contains n

-- A value scan finds a document exactly when some scalar element passes.
theorem value_scan_iff {V : Type} (p : Option V → Bool) (xs : List (Option V)) :
    -- the scan over the writer's entries finds a passing scalar element
    valueScan p (entries (some xs)) = true ↔ ∃ v, some v ∈ xs ∧ p (some v) = true := by
  -- split on whether the list is empty
  cases xs with
  -- the only entry is the marker, which the scan skips
  | nil => simp [entries, valueScan]
  -- a scalar element has an entry, a non-scalar one has none
  | cons x xs =>
    simp only [entries, valueScan, List.any_eq_true, List.mem_filterMap]
    constructor
    · rintro ⟨e, ⟨a, ha, hae⟩, hp⟩
      cases a with
      | none => simp at hae
      | some v =>
        simp at hae
        subst hae
        exact ⟨v, ha, hp⟩
    · rintro ⟨v, hv, hp⟩
      exact ⟨.val v, ⟨some v, hv, rfl⟩, hp⟩

/-- Every document that matches `_all` with a positive predicate, one that never
    passes a non-scalar element, is found by the `_all` scan. -/
theorem all_scan_complete {V : Type} (p : Option V → Bool) (a : Arr V)
    -- the predicate is positive: a non-scalar element fails it
    (hpos : p none = false)
    -- the document matches `_all`
    (h : allMatch p a = true) :
    -- then the scan finds it
    allScan p (entries a) = true := by
  -- look at the array
  cases a with
  -- a null array does not match, so this case cannot happen
  | none => simp [allMatch] at h
  | some xs =>
    cases xs with
    -- an empty array: found by the marker lookup
    | nil => simp [allScan, entries, isEmpty]
    -- a non-empty array: its first element passes, and it is a scalar
    | cons x xs =>
      simp only [allMatch, List.all_cons, Bool.and_eq_true] at h
      cases x with
      -- a non-scalar passing a positive predicate cannot happen
      | none => simp [hpos] at h
      -- a scalar: the value scan finds it
      | some v =>
        have : valueScan p (entries (some (some v :: xs))) = true :=
          (value_scan_iff p _).mpr ⟨v, by simp, h.1⟩
        simp [allScan, this]

/-- What the `_all` scan finds has a passing element or is an empty array. -/
theorem all_scan_sound {V : Type} (p : Option V → Bool) (a : Arr V)
    -- the scan found the document
    (h : allScan p (entries a) = true) :
    -- then it has a passing element, or it is the empty array
    anyMatch p a = true ∨ a = some [] := by
  -- look at the array
  cases a with
  -- a null array has only its null entry, which no part of the scan takes
  | none => simp [allScan, valueScan, entries, isEmpty] at h
  | some xs =>
    cases xs with
    -- the empty array
    | nil => exact Or.inr rfl
    -- a non-empty array has no marker, so a scalar passed
    | cons x xs =>
      left
      simp only [allScan, Bool.or_eq_true] at h
      rcases h with h | h
      · -- the value scan found a passing scalar element
        obtain ⟨v, hv, hpv⟩ := (value_scan_iff p _).mp h
        simp only [anyMatch, List.any_eq_true]
        exact ⟨some v, hv, hpv⟩
      · -- element entries are never the marker
        simp only [entries, List.any_eq_true, List.mem_filterMap] at h
        obtain ⟨e, ⟨a, _, hae⟩, he⟩ := h
        cases a with
        | none => simp at hae
        | some v =>
          simp at hae
          subst hae
          simp [isEmpty] at he

/-- A value scan, the scan of every condition that does not read the marker,
    never hands back an empty array. -/
theorem value_scan_skips_empty {V : Type} (p : Option V → Bool) :
    -- the only entry of `[]` is the marker, which the scan skips
    valueScan p (entries (some ([] : List (Option V)))) = false := by
  -- one entry, the marker
  rfl

/-- A bare negative scan reads the marker, so it finds every empty array. -/
theorem neg_scan_finds_empty {V : Type} (p : Option V → Bool) :
    -- the marker is the only entry of `[]`, and the scan looks it up
    negScan p (entries (some ([] : List (Option V)))) = true := by
  -- one entry, the marker
  rfl

/-- `_any` with a positive predicate loses nothing: every document it matches is
    found by the value scan. -/
theorem any_scan_complete {V : Type} (p : Option V → Bool) (a : Arr V)
    -- the predicate is positive: a non-scalar element fails it
    (hpos : p none = false)
    -- the document matches `_any`
    (h : anyMatch p a = true) :
    -- the value scan finds it
    valueScan p (entries a) = true := by
  -- look at the array
  cases a with
  -- a null array does not match `_any`
  | none => simp [anyMatch] at h
  | some xs =>
    -- the passing element
    simp only [anyMatch, List.any_eq_true] at h
    obtain ⟨y, hy, hpy⟩ := h
    cases y with
    -- a non-scalar passing a positive predicate cannot happen
    | none => rw [hpos] at hpy; exact absurd hpy (by simp)
    -- a scalar has its entry
    | some v => exact (value_scan_iff p xs).mpr ⟨v, hy, hpy⟩

/-- A null array never matches `_all`. -/
theorem nil_not_all {V : Type} (p : Option V → Bool) :
    -- Go's `allSlice` over a nil slice is false
    allMatch p (none : Arr V) = false := by
  -- by the definition
  rfl

/-- An empty array always matches `_all`. -/
theorem empty_all {V : Type} (p : Option V → Bool) :
    -- nothing breaks "every element passes"
    allMatch p (some ([] : List (Option V))) = true := by
  -- by the definition
  rfl

-- Two documents clash in a unique index when they share an entry that is a real
-- value. Null entries and the empty marker never clash, as `skips_uniqueness` says.
def clash {V : Type} [DecidableEq V] (a b : List (Entry V)) : Bool :=
  -- a shared scalar entry
  a.any fun e => match e with
    -- a scalar shared by both
    | .val v => b.contains (.val v)
    -- a null or the marker: never a clash
    | _ => false

/-- Two empty arrays can live in the same unique index. -/
theorem empty_arrays_never_clash {V : Type} [DecidableEq V] :
    -- both have only the marker, which never clashes
    clash (entries (some ([] : List (Option V)))) (entries (some ([] : List (Option V)))) = false := by
  -- one entry each, the marker
  rfl

/-- RED: with the old writer, an empty array has no entry, so the `_all` scan
    misses it even though it matches. -/
theorem neg_no_entry_misses_empty :
    -- `[]` matches `_all (= 1)`
    allMatch (fun n : Option Nat => n == some 1) (some []) = true
    -- but the scan over the old entries does not find it
    ∧ allScan (fun n : Option Nat => n == some 1) (oldEntries (some [])) = false := by
  -- both sides are literal; Lean computes them
  decide

/-- RED: sending `_all{_ne: 5}` to the index misses `[{"a": 1}]`. Its one element
    is an object, which has no entry, yet it is not 5, so the document matches.
    The scan finds no entry and no marker, so the document is lost. -/
theorem neg_all_negative_misses_inner :
    -- `[{"a": 1}]` matches `_all (≠ 5)`: the non-scalar element passes
    allMatch (fun n : Option Nat => n != some 5) (some [none]) = true
    -- but the `_all` scan over its entries does not find it
    ∧ allScan (fun n : Option Nat => n != some 5) (entries (some [none])) = false := by
  -- both sides are literal; Lean computes them
  decide

/-- RED: a bare `_ne: 5` scan that skips the marker misses `[]`. `[] _ne 5` is
    true, but the only entry of `[]` is the marker, so the scan hands back
    nothing and the document is lost. -/
theorem neg_skip_marker_misses_empty :
    -- `[]` passes the bare `_ne 5`
    bareNe 5 [] = true
    -- but the scan that skips the marker does not find it
    ∧ valueScan (fun n : Option Nat => n != some 5) (entries (some [])) = false := by
  -- both sides are literal; Lean computes them
  decide

-- A document with arrays at two paths of one JSON field, `a` and `b`.
structure TwoPaths (V : Type) where
  -- the array at path `a`
  a : Arr V
  -- the array at path `b`
  b : Arr V

-- `{a: {_all: p}, b: {_all: q}}`: both arrays pass their `_all`.
def bothAll {V : Type} (p q : Option V → Bool) (d : TwoPaths V) : Bool :=
  -- each path checked on its own
  allMatch p d.a && allMatch q d.b

-- The shipped scan: it reads path `a` and looks the marker up at `a` too.
def samePathScan {V : Type} (p : Option V → Bool) (d : TwoPaths V) : Bool :=
  -- the `_all` scan of one path, as `allScan` above
  allScan p (entries d.a)

-- The broken scan: it reads path `b` but looks the marker up at path `a`.
def splitPathScan {V : Type} (q : Option V → Bool) (d : TwoPaths V) : Bool :=
  -- elements of `b` that pass `b`'s condition, or the marker of `a`
  valueScan q (entries d.b) || (entries d.a).any isEmpty

/-- When the scan and the marker lookup share a path, every document matching
    `_all` on both paths is found; the filter checks the other path. -/
theorem same_path_scan_complete {V : Type} (p q : Option V → Bool) (d : TwoPaths V)
    -- the predicate on `a` is positive
    (hpos : p none = false)
    -- the document matches both conditions
    (h : bothAll p q d = true) :
    -- the scan finds it
    samePathScan p d = true := by
  -- the condition on `a` holds on its own
  simp only [bothAll, Bool.and_eq_true] at h
  -- and one path is the single-path case
  exact all_scan_complete p d.a hpos h.1

/-- RED: scanning `b` while looking the marker up at `a` misses
    `{"a": [1], "b": []}`, which matches `_all (≠ 25)` on both paths. -/
theorem neg_split_paths_miss_empty :
    -- the document matches
    bothAll (fun n : Option Nat => n != some 25) (fun n : Option Nat => n != some 25)
      ⟨some [some 1], some []⟩ = true
    -- but the split scan does not find it
    ∧ splitPathScan (fun n : Option Nat => n != some 25) ⟨some [some 1], some []⟩ = false := by
  -- both sides are literal; Lean computes them
  decide

-- One condition of a full index scan: the JSON path it reads and the test its
-- values must pass, a `_like` pattern or a `_ne`.
structure Cond (P V : Type) where
  -- the path the condition names
  path : P
  -- what a value at that path must pass
  test : V → Bool

-- A document passes every condition, each at its own path.
def passesAll {P V : Type} (doc : P → V) (cs : List (Cond P V)) : Bool :=
  -- each condition checks the value at its own path
  cs.all (fun c => c.test (doc c.path))

-- The shipped scan: the first condition gives both the path and the test.
def pinnedScan {P V : Type} (doc : P → V) : List (Cond P V) → Bool
  -- no condition: the scan keeps everything
  | [] => true
  -- the first condition's test on the first condition's path
  | c :: _ => c.test (doc c.path)

-- The broken scan: the path of the second condition, the test of the first.
def mixedScan {P V : Type} (doc : P → V) : List (Cond P V) → Bool
  -- two conditions: the test of one at the path of the other
  | c :: d :: _ => c.test (doc d.path)
  -- fewer: the same as the shipped scan
  | cs => pinnedScan doc cs

/-- When the scan's path and its value test come from one condition, every
    document passing all conditions is kept; the filter checks the others. -/
theorem pinned_scan_complete {P V : Type} (doc : P → V) (cs : List (Cond P V))
    -- the document passes every condition
    (h : passesAll doc cs = true) :
    -- the scan keeps it
    pinnedScan doc cs = true := by
  -- split on whether there is a first condition
  cases cs with
  -- none: the scan keeps everything
  | nil => rfl
  -- one: it is among the conditions the document passes
  | cons c rest =>
    -- the first condition holds on its own
    simp only [passesAll, List.all_cons, Bool.and_eq_true] at h
    -- and the scan is that condition
    exact h.1

-- The values in the RED example: a string at `a`, a number at `b`.
inductive Val where
  -- a string value, as its characters
  | str (s : List Char)
  -- a number value
  | num (n : Nat)
  deriving DecidableEq

-- `_like: "x%"`: a string starting with `x`.
def likeX : Val → Bool
  -- a string passes when it starts with `x`
  | .str ('x' :: _) => true
  -- any other string, and a number, does not match
  | _ => false

-- `_ne: 25`: anything but the number 25.
def ne25 : Val → Bool
  -- 25 fails
  | .num 25 => false
  -- everything else passes
  | _ => true

-- `{"a": "xa", "b": 5}`, with `true` standing for path `a`.
def doc : Bool → Val
  -- path `a` holds "xa"
  | true => .str ['x', 'a']
  -- path `b` holds 5
  | false => .num 5

/-- RED: reading path `b` but testing `a`'s pattern misses `{"a": "xa", "b": 5}`,
    which passes `{a: {_like: "x%"}, b: {_ne: 25}}`. -/
theorem neg_mixed_scan_misses :
    -- the document passes both conditions
    passesAll doc [⟨true, likeX⟩, ⟨false, ne25⟩] = true
    -- but the mixed scan drops it
    ∧ mixedScan doc [⟨true, likeX⟩, ⟨false, ne25⟩] = false := by
  -- both sides are literal; Lean computes them
  decide

-- The scan when another condition narrows it: the narrowing condition gives
-- the path, and a `_like` test joins it only when it names that same path.
def narrowedScan {P V : Type} [DecidableEq P] (doc : P → V)
    (scan : Cond P V) (like : Option (Cond P V)) : Bool :=
  -- the narrowing condition reads its own path
  scan.test (doc scan.path) &&
    match like with
    -- no pattern: nothing more to test
    | none => true
    -- a pattern joins the scan only at the scan's own path
    | some l => if l.path = scan.path then l.test (doc scan.path) else true

-- The broken scan: the pattern is tested on the scanned path whatever it names.
def unguardedScan {P V : Type} (doc : P → V)
    (scan : Cond P V) (like : Option (Cond P V)) : Bool :=
  -- the narrowing condition reads its own path
  scan.test (doc scan.path) &&
    match like with
    -- no pattern: nothing more to test
    | none => true
    -- the pattern reads the scanned path even when it names another
    | some l => l.test (doc scan.path)

/-- A range, `_eq` or `_in` scan keeps every document passing all conditions,
    because a `_like` from another path is left to the filter. -/
theorem narrowed_scan_complete {P V : Type} [DecidableEq P] (doc : P → V)
    (cs : List (Cond P V)) (scan : Cond P V) (like : Option (Cond P V))
    -- the document passes every condition
    (h : passesAll doc cs = true)
    -- the narrowing condition is one of them
    (hs : scan ∈ cs)
    -- and so is the pattern, if any
    (hl : ∀ l, like = some l → l ∈ cs) :
    -- the scan keeps it
    narrowedScan doc scan like = true := by
  -- every condition holds at its own path
  have all : ∀ c ∈ cs, c.test (doc c.path) = true := by
    -- read `passesAll` as a statement about each member
    simpa [passesAll, List.all_eq_true] using h
  -- unfold the scan into its two tests
  unfold narrowedScan
  -- the narrowing test holds; split on the pattern
  cases like with
  -- no pattern: only the narrowing test is left
  | none => simp [all scan hs]
  -- a pattern: split on whether its path is the scan's
  | some l =>
    -- the pattern is one of the conditions too
    have hl' := all l (hl l rfl)
    -- the same path: the pattern holds there; another: it is skipped
    by_cases hp : l.path = scan.path
    · -- read the pattern's own path as the scanned one
      rw [hp] at hl'
      -- both tests now hold at the scanned path
      simp [all scan hs, hp, hl']
    · -- the pattern is left to the filter
      simp [all scan hs, hp]

-- `_gt: 10`: a number above 10.
def gt10 : Val → Bool
  -- a number passes when it is above 10
  | .num n => decide (10 < n)
  -- a string does not
  | _ => false

-- `{"a": 15, "b": "xa"}`, with `true` standing for path `a`.
def doc2 : Bool → Val
  -- path `a` holds 15
  | true => .num 15
  -- path `b` holds "xa"
  | false => .str ['x', 'a']

/-- RED: a range scan on `a` that tests `b`'s pattern on `a`'s values misses
    `{"a": 15, "b": "xa"}`, which passes `{a: {_gt: 10}, b: {_like: "x%"}}`. -/
theorem neg_unguarded_scan_misses :
    -- the document passes both conditions
    passesAll doc2 [⟨true, gt10⟩, ⟨false, likeX⟩] = true
    -- but the unguarded scan drops it
    ∧ unguardedScan doc2 ⟨true, gt10⟩ (some ⟨false, likeX⟩) = false := by
  -- both sides are literal; Lean computes them
  decide

-- A document whose paths may be missing: `none` at a path it does not have.
-- A full scan of one path sees only documents with an entry there.
def entryScan {P V : Type} (doc : P → Option V) (c : Cond P (Option V)) : Bool :=
  -- the document needs a value at the path, and the value must pass
  match doc c.path with
  -- no entry at the path: the scan never sees the document
  | none => false
  -- an entry: kept when it passes the condition
  | some v => c.test (some v)

-- Whether a document missing the condition's path fails it, as `_ne` and the
-- `_like` family do and a bare `_nin` does not.
def needsEntry {P V : Type} (c : Cond P (Option V)) : Bool :=
  -- a missing path is a `none` value
  !c.test none

-- The shipped choice: the first condition a missing path fails, else the first.
def chooseScan {P V : Type} (cs : List (Cond P (Option V))) : Option (Cond P (Option V)) :=
  -- prefer a condition that only documents with the path pass
  (cs.find? needsEntry).orElse fun _ => cs.head?

/-- When some condition fails a missing path, the chosen full scan keeps every
    document that passes all conditions. -/
theorem chosen_scan_complete {P V : Type} (doc : P → Option V)
    (cs : List (Cond P (Option V)))
    -- the document passes every condition
    (h : passesAll doc cs = true)
    -- and one condition fails a missing path
    (hn : ∃ c ∈ cs, needsEntry c = true) :
    -- the chosen scan exists and keeps it
    ∃ c, chooseScan cs = some c ∧ entryScan doc c = true := by
  -- the search for such a condition succeeds
  obtain ⟨c0, hc0, hn0⟩ := hn
  have hfind : (cs.find? needsEntry).isSome = true := by
    -- `find?` finds something when a member passes
    rw [List.find?_isSome]
    exact ⟨c0, hc0, hn0⟩
  -- name what it finds
  obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hfind
  -- that is the chosen scan
  refine ⟨c, by simp [chooseScan, hc, Option.orElse], ?_⟩
  -- it is a member that fails a missing path
  have hmem : c ∈ cs := List.mem_of_find?_eq_some hc
  have hneed : needsEntry c = true := List.find?_some hc
  -- the document passes it
  have hpass : c.test (doc c.path) = true := by
    -- read `passesAll` as a statement about each member
    have := (List.all_eq_true.mp (by simpa [passesAll] using h)) c hmem
    simpa using this
  -- split on whether the document has the path
  unfold entryScan
  cases hd : doc c.path with
  -- missing: it would fail the condition, so this cannot happen
  | none =>
    rw [hd] at hpass
    simp [needsEntry, hpass] at hneed
  -- present: the scan keeps it because it passes
  | some v =>
    rw [hd] at hpass
    exact hpass

-- `_nin: [25]` on an optional value: a missing path passes.
def nin25 : Option Val → Bool
  -- 25 fails
  | some (.num 25) => false
  -- anything else, a missing path included, passes
  | _ => true

-- `_like: "x%"` on an optional value: a missing path fails.
def likeXOpt : Option Val → Bool
  -- a value: the plain pattern
  | some v => likeX v
  -- a missing path never matches a pattern
  | none => false

-- `{"b": "xa"}`, with `true` standing for path `a`.
def docNoA : Bool → Option Val
  -- no `a`
  | true => none
  -- `b` holds "xa"
  | false => some (.str ['x', 'a'])

/-- RED: pinning the scan to the first condition, a `_nin` on `a`, misses
    `{"b": "xa"}`, which passes `{a: {_nin: [25]}, b: {_like: "x%"}}`. -/
theorem neg_nin_pin_misses :
    -- the document passes both conditions
    passesAll docNoA [⟨true, nin25⟩, ⟨false, likeXOpt⟩] = true
    -- but a scan of `a` never sees it
    ∧ entryScan docNoA ⟨true, nin25⟩ = false := by
  -- both sides are literal; Lean computes them
  decide

/-- Two `_any` clauses each need some element: the scan bounded by one clause
    alone keeps every document passing both. -/
theorem separate_any_complete {V : Type} (p q : Option V → Bool) (a : Arr V)
    -- the first predicate is positive
    (hpos : p none = false)
    -- the document passes both clauses
    (hp : anyMatch p a = true) (_hq : anyMatch q a = true) :
    -- the scan of the first clause finds it
    valueScan p (entries a) = true :=
  -- one clause is the single `_any` case
  any_scan_complete p a hpos hp

-- `_gt k`: a scalar number above `k`.
def gtN (k : Nat) : Option Nat → Bool
  -- a scalar passes when it is above `k`
  | some n => decide (k < n)
  -- a non-scalar does not
  | none => false

-- `_lt k`: a scalar number below `k`.
def ltN (k : Nat) : Option Nat → Bool
  -- a scalar passes when it is below `k`
  | some n => decide (n < k)
  -- a non-scalar does not
  | none => false

/-- RED: joining the bounds of two `_any` clauses into one range misses
    `[5, 30]`, which has an element above 10 and another below 20. -/
theorem neg_joined_any_misses :
    -- the array passes both clauses
    anyMatch (gtN 10) (some [some 5, some 30]) = true
    ∧ anyMatch (ltN 20) (some [some 5, some 30]) = true
    -- but no single element lies in the joined range
    ∧ valueScan (fun n => gtN 10 n && ltN 20 n)
        (entries (some [some 5, some 30])) = false := by
  -- all sides are literal; Lean computes them
  decide

end AllScanEmptyArray

#print axioms AllScanEmptyArray.all_scan_complete
#print axioms AllScanEmptyArray.all_scan_sound
#print axioms AllScanEmptyArray.value_scan_iff
#print axioms AllScanEmptyArray.value_scan_skips_empty
#print axioms AllScanEmptyArray.neg_scan_finds_empty
#print axioms AllScanEmptyArray.any_scan_complete
#print axioms AllScanEmptyArray.nil_not_all
#print axioms AllScanEmptyArray.empty_all
#print axioms AllScanEmptyArray.empty_arrays_never_clash
#print axioms AllScanEmptyArray.neg_no_entry_misses_empty
#print axioms AllScanEmptyArray.neg_all_negative_misses_inner
#print axioms AllScanEmptyArray.neg_skip_marker_misses_empty
#print axioms AllScanEmptyArray.same_path_scan_complete
#print axioms AllScanEmptyArray.neg_split_paths_miss_empty
#print axioms AllScanEmptyArray.pinned_scan_complete
#print axioms AllScanEmptyArray.neg_mixed_scan_misses
#print axioms AllScanEmptyArray.narrowed_scan_complete
#print axioms AllScanEmptyArray.neg_unguarded_scan_misses
#print axioms AllScanEmptyArray.chosen_scan_complete
#print axioms AllScanEmptyArray.neg_nin_pin_misses
#print axioms AllScanEmptyArray.separate_any_complete
#print axioms AllScanEmptyArray.neg_joined_any_misses
