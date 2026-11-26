/-!
# AllScanEmptyArray: an `_all` index scan finds the empty arrays

The story, in plain words. A JSON index stores one entry per scalar in the
value, each with its path. A null at a path is stored as a null entry. An empty
array has no element to store, so the index used to write nothing for it, as if
the path were missing. (A typed array field already keeps its empty arrays under
its null entry, and `AllEmptyArray` covers how its scan finds them.)

`{custom: {tags: {_all: {_eq: 1}}}}` asks for documents whose array at `tags`
has every element equal to 1. An empty array has no element that breaks that,
so it matches (Go's `allSlice` over nothing is true); a null array does not.
The index scan for `_all` looks up the entries whose value passes the
condition, so it never saw the empty arrays.

The fix gives an empty array an entry of its own at its path, the empty
marker, and the `_all` scan also looks the marker up. Every other scan skips
the marker: no element of an empty array passes `_any`, `_eq`, a range or a
`_like`.

Who acts: the writer, which turns a document's array into entries, and a scan,
which turns entries back into the documents it hands to the filter.

What must never happen:
- a document that matches `_all` is missing from the `_all` scan (`all_scan_complete`);
- a document the `_all` scan finds has neither a passing element nor an empty
  array, so the scan did more than it needed (`all_scan_sound`);
- a scan other than `_all` hands back an empty array (`value_scan_skips_empty`),
  or `_any` misses one of its matches (`any_scan_complete`);
- a null array matches `_all` (`nil_not_all`);
- two empty arrays clash in a unique index (`empty_arrays_never_clash`);
- with `_all` on two paths of one JSON index, the scan reads one path and the
  marker lookup another, so a match whose scanned array is `[]` is lost
  (`same_path_scan_complete`);
- a full scan reads one path but tests its values with the `_like` pattern of
  another path's condition, so a match is lost (`pinned_scan_complete`);
- a range, `_eq` or `_in` scan on one path tests its values with a `_like`
  pattern of another path, so a match is lost (`narrowed_scan_complete`).
- a full scan reads the path of a bare `_nin`, which a document without that
  path passes, so the document is lost (`chosen_scan_complete`);
- bounds from two `_any` clauses are joined into one range, which no single
  element of a match may fall in (`separate_any_complete`).

The RED side: with the old writer, which stored nothing for `[]`, the same
scan misses an empty array that matches (`neg_no_entry_misses_empty`). A scan
that reads one path but looks the marker up at another misses
`{"a": [1], "b": []}` (`neg_split_paths_miss_empty`). A scan that reads the
last condition's path but tests the first condition's pattern misses
`{"a": "xa", "b": 5}` under `{a: {_like: "x%"}, b: {_ne: 25}}`
(`neg_mixed_scan_misses`). A range scan on `a` that tests `b`'s pattern on
`a`'s values misses `{"a": 15, "b": "xa"}` under
`{a: {_gt: 10}, b: {_like: "x%"}}` (`neg_unguarded_scan_misses`). A scan pinned to `_nin` on `a` misses `{"b": "xa"}` under
`{a: {_nin: [25]}, b: {_like: "x%"}}` (`neg_nin_pin_misses`). Joining
`{_any: {_gt: 10}}` and `{_any: {_lt: 20}}` from two clauses misses `[5, 30]`
(`neg_joined_any_misses`).

Rust anchors:
- the writer: `extract_json_leaves` (`crates/building-blocks/document/src/normal.rs`);
- the `_all` scan: `filter_to_index_scan`, `all_path` and `empty_array_scan`
  (`crates/engine/query/src/planner/index_selection/filter_to_scan.rs`);
- the skip: `RangeIterator::next` (`crates/engine/storage/src/index/range_iterator.rs`),
  which drops the marker from every bounded scan, and `full_field_range`, which
  makes the `_ne` and `_like` scans bounded ones;
- the pinned path: `filter_to_index_scan`, where the first `_ne`, `_nin` or
  `_like` condition gives the scan both its path and its value filter, and
  a narrowing scan keeps the value filter only when it names the scan's path;
- the choice of that condition: `full_scan_at`, `is_full_scan` and
  `accepts_missing` in `filter_to_index_scan`, which skip a bare `_nin` while
  another condition can pin the scan, and `in_range`, which joins bounds only
  within one `_any` clause (`FieldCondition::clause`);
- the unique exemption: `skips_uniqueness` (`crates/engine/storage/src/index/mod.rs`).
-/

namespace AllScanEmptyArray

-- One index entry of the array at a JSON path. `V` is the element type.
inductive Entry (V : Type) where
  -- a null: the value at the path is null
  | null
  -- the empty marker: the array has no elements
  | empty
  -- one element of the array
  | val (v : V)
  deriving DecidableEq

-- The array of a document: `none` for a null array, `some xs` for the list `xs`.
abbrev Arr (V : Type) := Option (List V)

-- What the writer stores for an array.
def entries {V : Type} : Arr V → List (Entry V)
  -- a null array: one null entry
  | none => [.null]
  -- an empty array: one empty marker
  | some [] => [.empty]
  -- otherwise one entry per element
  | some (x :: xs) => (x :: xs).map .val

-- What the old writer stored: nothing at all for an empty array.
def oldEntries {V : Type} : Arr V → List (Entry V)
  -- a null array: one null entry
  | none => [.null]
  -- an empty array: no entry, like a missing path
  | some [] => []
  -- otherwise one entry per element
  | some (x :: xs) => (x :: xs).map .val

-- `_all p`: a null array fails, an empty one passes, otherwise every element passes.
def allMatch {V : Type} (p : V → Bool) : Arr V → Bool
  -- a null array never matches
  | none => false
  -- every element, which is none at all for `[]`
  | some xs => xs.all p

-- `_any p`: some element passes.
def anyMatch {V : Type} (p : V → Bool) : Arr V → Bool
  -- a null array never matches
  | none => false
  -- at least one element passes
  | some xs => xs.any p

-- A value scan over `p`: the document has an element entry whose value passes.
-- The null entry and the empty marker are never part of it.
def valueScan {V : Type} (p : V → Bool) (es : List (Entry V)) : Bool :=
  -- look for a passing element entry
  es.any fun e => match e with
    -- an element: keep it when it passes
    | .val v => p v
    -- a null or the empty marker: skipped
    | _ => false

-- Whether an entry is the empty marker.
def isEmpty {V : Type} : Entry V → Bool
  -- the marker
  | .empty => true
  -- a null or an element
  | _ => false

-- The `_all` scan: the value scan, plus a lookup of the empty marker.
def allScan {V : Type} (p : V → Bool) (es : List (Entry V)) : Bool :=
  -- an element that passes, or the marker
  valueScan p es || es.any isEmpty

/-- Every document that matches `_all` is found by the `_all` scan. -/
theorem all_scan_complete {V : Type} (p : V → Bool) (a : Arr V)
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
    -- a non-empty array: its first element passes, so the value scan finds it
    | cons x xs =>
      simp only [allMatch, List.all_cons, Bool.and_eq_true] at h
      simp [allScan, valueScan, entries, h.1]

/-- What the `_all` scan finds has a passing element or is an empty array. -/
theorem all_scan_sound {V : Type} (p : V → Bool) (a : Arr V)
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
    -- a non-empty array has no marker, so an element passed
    | cons x xs =>
      left
      simp only [allScan, entries, Bool.or_eq_true] at h
      rcases h with h | h
      · -- the value scan found a passing element
        simp only [valueScan, List.any_map, List.any_eq_true] at h
        obtain ⟨y, hy, hpy⟩ := h
        simp only [anyMatch, List.any_eq_true]
        exact ⟨y, hy, by simpa using hpy⟩
      · -- element entries are never the marker
        simp [List.any_map, isEmpty] at h

/-- A value scan, the scan of every condition but `_all`, never hands back an
    empty array. -/
theorem value_scan_skips_empty {V : Type} (p : V → Bool) :
    -- the only entry of `[]` is the marker, which the scan skips
    valueScan p (entries (some ([] : List V))) = false := by
  -- one entry, the marker
  rfl

/-- `_any` loses nothing: every document it matches is found by the value scan. -/
theorem any_scan_complete {V : Type} (p : V → Bool) (a : Arr V)
    -- the document matches `_any`
    (h : anyMatch p a = true) :
    -- the value scan finds it
    valueScan p (entries a) = true := by
  -- look at the array
  cases a with
  -- a null array does not match `_any`
  | none => simp [anyMatch] at h
  | some xs =>
    cases xs with
    -- an empty array does not match `_any`
    | nil => simp [anyMatch] at h
    -- otherwise the passing element has its entry
    | cons x xs =>
      simp only [anyMatch, List.any_eq_true] at h
      obtain ⟨y, hy, hpy⟩ := h
      simp only [valueScan, entries, List.any_map, List.any_eq_true]
      exact ⟨y, hy, by simpa using hpy⟩

/-- A null array never matches `_all`. -/
theorem nil_not_all {V : Type} (p : V → Bool) :
    -- Go's `allSlice` over a nil slice is false
    allMatch p (none : Arr V) = false := by
  -- by the definition
  rfl

/-- An empty array always matches `_all`. -/
theorem empty_all {V : Type} (p : V → Bool) :
    -- nothing breaks "every element passes"
    allMatch p (some ([] : List V)) = true := by
  -- by the definition
  rfl

-- Two documents clash in a unique index when they share an entry that is a real
-- value. Null entries and the empty marker never clash, as `skips_uniqueness` says.
def clash {V : Type} [DecidableEq V] (a b : List (Entry V)) : Bool :=
  -- a shared element entry
  a.any fun e => match e with
    -- an element shared by both
    | .val v => b.contains (.val v)
    -- a null or the marker: never a clash
    | _ => false

/-- Two empty arrays can live in the same unique index. -/
theorem empty_arrays_never_clash {V : Type} [DecidableEq V] :
    -- both have only the marker, which never clashes
    clash (entries (some ([] : List V))) (entries (some ([] : List V))) = false := by
  -- one entry each, the marker
  rfl

/-- RED: with the old writer, an empty array has no entry, so the `_all` scan
    misses it even though it matches. -/
theorem neg_no_entry_misses_empty :
    -- `[]` matches `_all (= 1)`
    allMatch (fun n : Nat => n == 1) (some []) = true
    -- but the scan over the old entries does not find it
    ∧ allScan (fun n : Nat => n == 1) (oldEntries (some [])) = false := by
  -- both sides are literal; Lean computes them
  decide

-- A document with arrays at two paths of one JSON field, `a` and `b`.
structure TwoPaths (V : Type) where
  -- the array at path `a`
  a : Arr V
  -- the array at path `b`
  b : Arr V

-- `{a: {_all: p}, b: {_all: q}}`: both arrays pass their `_all`.
def bothAll {V : Type} (p q : V → Bool) (d : TwoPaths V) : Bool :=
  -- each path checked on its own
  allMatch p d.a && allMatch q d.b

-- The shipped scan: it reads path `a` and looks the marker up at `a` too.
def samePathScan {V : Type} (p : V → Bool) (d : TwoPaths V) : Bool :=
  -- the `_all` scan of one path, as `allScan` above
  allScan p (entries d.a)

-- The broken scan: it reads path `b` but looks the marker up at path `a`.
def splitPathScan {V : Type} (q : V → Bool) (d : TwoPaths V) : Bool :=
  -- elements of `b` that pass `b`'s condition, or the marker of `a`
  valueScan q (entries d.b) || (entries d.a).any isEmpty

/-- When the scan and the marker lookup share a path, every document matching
    `_all` on both paths is found; the filter checks the other path. -/
theorem same_path_scan_complete {V : Type} (p q : V → Bool) (d : TwoPaths V)
    -- the document matches both conditions
    (h : bothAll p q d = true) :
    -- the scan finds it
    samePathScan p d = true := by
  -- the condition on `a` holds on its own
  simp only [bothAll, Bool.and_eq_true] at h
  -- and one path is the single-path case
  exact all_scan_complete p d.a h.1

/-- RED: scanning `b` while looking the marker up at `a` misses
    `{"a": [1], "b": []}`, which matches `_all (≠ 25)` on both paths. -/
theorem neg_split_paths_miss_empty :
    -- the document matches
    bothAll (fun n : Nat => n != 25) (fun n : Nat => n != 25) ⟨some [1], some []⟩ = true
    -- but the split scan does not find it
    ∧ splitPathScan (fun n : Nat => n != 25) ⟨some [1], some []⟩ = false := by
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
theorem separate_any_complete {V : Type} (p q : V → Bool) (a : Arr V)
    -- the document passes both clauses
    (hp : anyMatch p a = true) (_hq : anyMatch q a = true) :
    -- the scan of the first clause finds it
    valueScan p (entries a) = true :=
  -- one clause is the single `_any` case
  any_scan_complete p a hp

/-- RED: joining the bounds of two `_any` clauses into one range misses
    `[5, 30]`, which has an element above 10 and another below 20. -/
theorem neg_joined_any_misses :
    -- the array passes both clauses
    anyMatch (fun n : Nat => decide (10 < n)) (some [5, 30]) = true
    ∧ anyMatch (fun n : Nat => decide (n < 20)) (some [5, 30]) = true
    -- but no single element lies in the joined range
    ∧ valueScan (fun n : Nat => decide (10 < n) && decide (n < 20))
        (entries (some [5, 30])) = false := by
  -- all sides are literal; Lean computes them
  decide

end AllScanEmptyArray

#print axioms AllScanEmptyArray.all_scan_complete
#print axioms AllScanEmptyArray.all_scan_sound
#print axioms AllScanEmptyArray.value_scan_skips_empty
#print axioms AllScanEmptyArray.any_scan_complete
#print axioms AllScanEmptyArray.nil_not_all
#print axioms AllScanEmptyArray.empty_all
#print axioms AllScanEmptyArray.empty_arrays_never_clash
#print axioms AllScanEmptyArray.neg_no_entry_misses_empty
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
