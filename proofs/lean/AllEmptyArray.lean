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
so the scan also reads the null entry.

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

end AllEmptyArray
