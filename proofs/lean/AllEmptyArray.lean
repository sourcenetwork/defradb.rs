/-!
# AllEmptyArray: `_all` on an indexed array finds empty arrays

The story. A document holds an array, an absent array (null), or nothing
else. An array index writes one entry per distinct element, and one null
entry when the array is empty or null, so every document has an entry. A
query asks for the documents whose every element passes a test (`_all`).
The planner turns that test into an index scan, which hands back candidate
documents; the whole filter then checks each candidate again.

What can happen: an empty array passes every `_all` test, because it has no
element that fails. Its only entry is the null entry.

What must never happen: a document that passes the `_all` test is missing
from the candidates. A scan over the elements alone misses the empty array,
so the scan also reads the null entry, unless it reads it already (a range
that runs through the null end of the index) or the filter also holds an
`_any`, which no empty or null array passes.

Rust owner: crates/engine/query/src/planner/index_selection/empty_array.rs
`with_empty_arrays`; the entries come from
crates/database/db/src/index/manager/value_extraction.rs
`expand_value_for_indexing`.
-/
namespace AllEmptyArray

-- An element is a value or a null element; `none` is null.
abbrev Elem := Option Int

-- The index entries of one document's array: its elements, or the single null
-- entry when there are none. A null array is given as `none` and is the null
-- entry too.
def entries : Option (List Elem) → List Elem
  -- A null array gets the null entry.
  | none => [none]
  -- An empty array gets the null entry.
  | some [] => [none]
  -- Any other array gets its elements.
  | some (x :: xs) => x :: xs

-- `_all p`: the document holds an array and every element passes `p`. An empty
-- array passes, a null array does not.
def allMatch (p : Elem → Bool) : Option (List Elem) → Bool
  -- A null array is not an array whose elements all pass.
  | none => false
  -- Every element of the array passes.
  | some xs => xs.all p

-- A scan reads the entries that pass `q` and returns a document when one of its
-- entries is read.
def found (q : Elem → Bool) (doc : Option (List Elem)) : Bool :=
  (entries doc).any q

-- The scan the planner builds for `_all p`: the element scan `q`, plus the null
-- entry.
def widened (q : Elem → Bool) (e : Elem) : Bool :=
  -- An entry is read when the element scan reads it or it is the null entry.
  q e || e == none

-- Promise: when the element scan reads every element that passes `p`, every
-- document that passes `_all p` is a candidate of the widened scan.
theorem widened_finds_all (p q : Elem → Bool) (hq : ∀ e, p e = true → q e = true)
    (doc : Option (List Elem)) (h : allMatch p doc = true) :
    found (widened q) doc = true := by
  -- Look at each shape the document can have.
  match doc, h with
  -- A null array never passes, so there is nothing to show.
  | none, h => simp [allMatch] at h
  -- An empty array is found through its null entry.
  | some [], _ => simp [found, entries, widened]
  -- A non-empty array is found through its first element, which passes `p`.
  | some (x :: xs), h =>
    -- The first element passes `p`, because every element does.
    have hx : p x = true := by
      simp [allMatch, List.all_cons] at h
      exact h.1
    -- So the element scan reads it.
    simp [found, entries, widened, hq x hx]

-- Promise, the RED case: an element scan that never reads the null entry misses
-- the empty array, which passes every `_all` test.
theorem element_scan_misses_empty (p q : Elem → Bool) (hnull : q none = false) :
    allMatch p (some []) = true ∧ found q (some []) = false := by
  -- The empty array passes `_all p`, and its only entry is the null entry.
  simp [allMatch, found, entries, hnull]

-- Promise: the widened scan only adds the null entry, so a document it finds
-- that the element scan does not holds an empty or null array, or a null element.
theorem widened_adds_only_null (q : Elem → Bool) (doc : Option (List Elem))
    (h : found (widened q) doc = true) (hmiss : found q doc = false) :
    none ∈ entries doc := by
  -- Some entry of the document is read by the widened scan.
  simp only [found, List.any_eq_true] at h
  obtain ⟨e, he, hread⟩ := h
  -- The element scan reads no entry of the document, so not this one either.
  have hq : q e = false := by
    simp only [found, List.any_eq_false] at hmiss
    simpa using hmiss e he
  -- So it is the null entry.
  simp [widened, hq] at hread
  subst hread
  exact he

-- `_any r`: the document holds an array and some element passes `r`. Neither an
-- empty nor a null array passes.
def anyMatch (r : Elem → Bool) : Option (List Elem) → Bool
  -- A null array has no element that passes.
  | none => false
  -- Some element of the array passes.
  | some xs => xs.any r

-- Promise: when the scan already reads the null entry, widening it changes no
-- entry it reads, so the planner leaves it as it is.
theorem widened_same_when_null_read (q : Elem → Bool) (hnull : q none = true) (e : Elem) :
    widened q e = q e := by
  -- The null entry is read either way; any other entry is read exactly when `q` reads it.
  cases e <;> simp [widened, hnull]

-- Promise: with an `_any r` beside the `_all p`, the scan built from the `_any`
-- finds every document that passes both, with no null entry added.
theorem any_scan_finds_all_and_any (p r q : Elem → Bool)
    (hq : ∀ e, r e = true → q e = true) (doc : Option (List Elem))
    (_hall : allMatch p doc = true) (hany : anyMatch r doc = true) :
    found q doc = true := by
  -- Look at each shape the document can have.
  match doc, hany with
  -- A null array passes no `_any`, so there is nothing to show.
  | none, hany => simp [anyMatch] at hany
  -- An empty array passes no `_any` either.
  | some [], hany => simp [anyMatch] at hany
  -- A non-empty array has an element passing `r`, and the scan reads it.
  | some (x :: xs), hany =>
    -- Take the element that passes `r`.
    simp only [anyMatch, List.any_eq_true] at hany
    obtain ⟨e, he, hr⟩ := hany
    -- It is one of the document's entries, and the scan reads it.
    simp only [found, entries, List.any_eq_true]
    exact ⟨e, he, hq e hr⟩

end AllEmptyArray
