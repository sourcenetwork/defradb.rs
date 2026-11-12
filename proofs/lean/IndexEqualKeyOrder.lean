/-!
# IndexEqualKeyOrder: equal index keys come back in creation order

The story, in plain words. A secondary index stores one entry per document:
the indexed value, then the document's short ID. A node hands out short IDs
in increasing order as it creates (or first merges) documents, so a later
document gets a larger ID. An index scan walks entries in key order: by
value, and for one value by short ID. An exact-match query (`_eq`) takes the
entries of one value in scan order, then skips `offset` of them and keeps at
most `limit`.

What must hold: the documents an exact match returns are in increasing
short-ID order, which is the order this node created them in. Go keys its
index the same way (`getDocumentsIndexKey` in `internal/db/index.go`) and
returns the same order.

What must never happen: a reorder by public DocID. An encrypted document's
DocID is new on every run, so a DocID order (the #1602 tie-break this
replaces) puts the same documents in a different order from run to run
(`docIdOrder_can_reverse_creation`).

Rust anchors: `crates/db/src/read/index_order.rs` `apply_exact_match_page`
and `extend_unique`; the ExactMatch and InScan arms of `get_by_index_scan` in
`crates/db/src/read/doc.rs`, `read/lensed/fetcher/scan.rs` and
`read/lensed/autocommit/scan.rs`; `crates/db/src/docid/map.rs`
`DocShortIdAllocator::next`; `crates/storage/src/keys/datastore/index_key.rs`
`IndexDataStoreKey` (the short ID is the key's last, order-preserving part).
-/
namespace IndexEqualKeyOrder

-- One index entry: the indexed value and the document's short ID.
structure Entry where
  -- the indexed field's value (a number stands for any ordered value)
  value : Nat
  -- the document's node-local short ID, the key's suffix
  shortId : Nat
deriving DecidableEq, Repr

-- Key order: by value first, then by short ID.
def keyLt (a b : Entry) : Prop :=
  -- a smaller value comes first,
  a.value < b.value ∨
  -- and for the same value, the smaller short ID.
  (a.value = b.value ∧ a.shortId < b.shortId)

-- A scan returns entries in key order: each one before every later one.
def Sorted (scan : List Entry) : Prop := scan.Pairwise keyLt

-- A page of a list: skip `offset`, then keep at most `limit`.
def page (offset : Nat) (limit : Option Nat) (l : List Nat) : List Nat :=
  -- no limit keeps the rest;
  match limit with
  | none => l.drop offset
  -- a limit keeps that many of the rest.
  | some n => (l.drop offset).take n

-- An exact match on `v`: the entries with that value, in scan order, as
-- short IDs, then paged.
def exactMatch (scan : List Entry) (v : Nat) (offset : Nat) (limit : Option Nat) :
    List Nat :=
  page offset limit ((scan.filter (fun e => e.value == v)).map Entry.shortId)

-- Promise: paging never reorders, it only cuts a slice.
theorem page_sublist (offset : Nat) (limit : Option Nat) (l : List Nat) :
    (page offset limit l).Sublist l := by
  -- either way the page is a slice of `l`.
  cases limit with
  | none => exact List.drop_sublist offset l
  | some n => exact (List.take_sublist n _).trans (List.drop_sublist offset l)

-- Promise: within one value, key order is short-ID order.
theorem keyLt_same_value {a b : Entry} (h : keyLt a b) (ha : a.value = b.value) :
    a.shortId < b.shortId := by
  -- a smaller value is impossible when the values are equal,
  rcases h with hv | ⟨_, hs⟩
  · exact absurd ha (Nat.ne_of_lt hv)
  -- so the short IDs decide.
  · exact hs

-- Promise: an exact match returns short IDs in increasing order.
theorem exactMatch_ascending (scan : List Entry) (h : Sorted scan) (v : Nat)
    (offset : Nat) (limit : Option Nat) :
    (exactMatch scan v offset limit).Pairwise (· < ·) := by
  -- the matching entries are still in key order,
  have hf : (scan.filter (fun e => e.value == v)).Pairwise keyLt :=
    List.Pairwise.filter _ h
  -- and all have value `v`, so their short IDs increase,
  have hm : ((scan.filter (fun e => e.value == v)).map Entry.shortId).Pairwise (· < ·) := by
    rw [List.pairwise_map]
    apply List.Pairwise.imp_of_mem _ hf
    intro a b ha hb hab
    -- both came through the filter, so both have value `v`;
    have hav : a.value = v := by simpa using (List.mem_filter.mp ha).2
    have hbv : b.value = v := by simpa using (List.mem_filter.mp hb).2
    exact keyLt_same_value hab (hav.trans hbv.symm)
  -- and a page of an increasing list still increases.
  exact hm.sublist (page_sublist offset limit _)

-- The allocator: hand out `next`, and move on to `next + 1`.
def allocate (next : Nat) : Nat × Nat := (next, next + 1)

-- Promise: a document created after another gets a larger short ID.
theorem later_create_larger (next : Nat) :
    (allocate next).1 < (allocate (allocate next).2).1 := by
  -- the second ID is the first plus one.
  simp [allocate]

-- Promise: an exact match on two documents with one value lists the one
-- created first, first.
theorem exactMatch_creation_order (next v : Nat) :
    let first := (allocate next).1
    let second := (allocate (allocate next).2).1
    exactMatch [⟨v, first⟩, ⟨v, second⟩] v 0 none = [first, second] := by
  -- both entries match, and the scan holds them in key order.
  simp [exactMatch, page, allocate]

-- A DocID order: sort by an ID that has nothing to do with creation.
def docIdFirst (a b : Nat × Nat) : List Nat :=
  -- each pair is (short ID, DocID); the smaller DocID comes first.
  if a.2 ≤ b.2 then [a.1, b.1] else [b.1, a.1]

-- Promise (why #1602's tie-break goes): a DocID order can put the document
-- created second first. Here the first document's DocID happens to sort last,
-- as an encrypted document's new DocID does on some runs.
theorem docIdOrder_can_reverse_creation :
    docIdFirst (1, 0xb31d) (2, 0x88ac) ≠ exactMatch [⟨21, 1⟩, ⟨21, 2⟩] 21 0 none := by
  -- the DocID order is [2, 1]; the index order is [1, 2].
  decide

end IndexEqualKeyOrder
