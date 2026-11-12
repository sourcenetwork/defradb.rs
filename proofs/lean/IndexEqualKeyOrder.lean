/-!
# IndexEqualKeyOrder: equal index keys come back in short-ID order

The story, in plain words. A secondary index stores one entry per document:
the indexed value, then the document's short ID. A node hands out short IDs
in increasing order as it creates (or first merges) documents, so a later
document gets a larger ID. An index scan walks entries in key order: by
value, and for one value by short ID. An exact-match query (`_eq`) takes the
entries of one value in scan order, then skips `offset` of them and keeps at
most `limit`.

What must hold: the documents an exact match returns are in increasing
short-ID order, which is the order this node first stored them in: created
here, or first received from a peer, so two replicas can differ. Go keys its
index the same way (`getDocumentsIndexKey` in `internal/db/index.go`) and
returns the same order.

What must never happen: a reorder by public DocID. An encrypted document's
DocID is new on every run, so a DocID order (the #1602 tie-break this
replaces) puts the same documents in a different order from run to run
(`docIdOrder_can_reverse_creation`).

Rust anchors: `crates/db/src/read/index_order.rs` `apply_exact_match_page`
and `extend_unique`; the ExactMatch and InScan arms of `get_by_index_scan` in
`crates/db/src/read/doc.rs` and of `get_by_index_scan_inner` in
`read/lensed/fetcher/scan.rs` and `read/lensed/autocommit/scan.rs`;
`crates/db/src/docid/map.rs`
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

/-!
## InScan: several values, each in scan order, repeats dropped

An `_in` query scans each listed value in turn. `extend_unique` appends each
value's short IDs, in scan order, to what the earlier values gave, and skips a
short ID it has already listed (one document can match two values of a
composite prefix only once). The Rust code keeps a `seen` set beside the list;
here the list itself is the set, since both hold the same IDs.

What must hold: the result lists every matched document once, in the order of
its first encounter, and nothing a later value adds moves an earlier one.
-/

-- One value's short IDs appended to `all`: a listed ID is skipped, a new one
-- goes on the end.
def extendUnique : List Nat → List Nat → List Nat
  -- nothing left to append: the list stands;
  | all, [] => all
  -- the next ID is skipped if listed, and appended otherwise.
  | all, x :: rest =>
    if x ∈ all then extendUnique all rest else extendUnique (all ++ [x]) rest

-- An InScan: each value's scan, appended in the `_in` list's order.
def inScan (scans : List (List Nat)) : List Nat := scans.foldl extendUnique []

-- Promise: appending never moves what is already listed; it only adds at
-- the end. So an ID keeps the position it got on its first encounter.
theorem extendUnique_prefix (all es : List Nat) : all <+: extendUnique all es := by
  induction es generalizing all with
  -- nothing appended: the list is its own prefix;
  | nil => exact List.prefix_refl all
  | cons x rest ih =>
    unfold extendUnique
    split
    -- a skipped ID leaves the list as it was;
    · exact ih all
    -- a new ID extends it, and the rest extends that.
    · exact (List.prefix_append all [x]).trans (ih (all ++ [x]))

-- Promise: a repeat changes nothing.
theorem extendUnique_seen (all rest : List Nat) (x : Nat) (h : x ∈ all) :
    extendUnique all (x :: rest) = extendUnique all rest := by
  -- the listed branch is taken.
  simp [extendUnique, h]

-- Promise: the result keeps scan order: it is the listed IDs, then the scan,
-- with some entries left out and none reordered.
theorem extendUnique_sublist (all es : List Nat) :
    (extendUnique all es).Sublist (all ++ es) := by
  induction es generalizing all with
  -- nothing appended: the list itself;
  | nil => simp [extendUnique]
  | cons x rest ih =>
    unfold extendUnique
    split
    -- a skipped ID is one entry left out;
    · exact (ih all).trans (List.Sublist.append_left (List.sublist_cons_self x rest) all)
    -- a kept ID moves from the scan to the list, in the same place.
    · simpa using ih (all ++ [x])

-- Promise: a document is listed at most once.
theorem extendUnique_nodup (all es : List Nat) (h : all.Nodup) :
    (extendUnique all es).Nodup := by
  induction es generalizing all with
  -- nothing appended: the list was already repeat-free;
  | nil => simpa [extendUnique] using h
  | cons x rest ih =>
    unfold extendUnique
    split
    -- a skipped ID adds nothing;
    · exact ih all h
    -- a new ID is not yet listed, so appending it keeps the list repeat-free.
    · rename_i hx
      apply ih
      rw [List.Nodup, List.pairwise_append]
      refine ⟨h, List.pairwise_singleton _ x, ?_⟩
      -- every listed ID differs from `x`, which was not listed.
      intro a ha b hb
      simp only [List.mem_singleton] at hb
      subst hb
      intro hab
      subst hab
      exact hx ha

-- Promise: every scanned document is listed, and nothing else is.
theorem mem_extendUnique (all es : List Nat) (y : Nat) :
    y ∈ extendUnique all es ↔ y ∈ all ∨ y ∈ es := by
  induction es generalizing all with
  -- nothing appended: membership is the list's;
  | nil => simp [extendUnique]
  | cons x rest ih =>
    unfold extendUnique
    split
    -- a skipped ID was already listed, so it is still covered;
    · rename_i hx
      rw [ih all]
      constructor
      · rintro (h | h)
        · exact Or.inl h
        · exact Or.inr (List.mem_cons_of_mem x h)
      · rintro (h | h)
        · exact Or.inl h
        · rcases List.mem_cons.mp h with rfl | h
          · exact Or.inl hx
          · exact Or.inr h
    -- a new ID moves from the scan to the list.
    · rw [ih (all ++ [x])]
      simp [or_assoc]

-- The same four promises over a whole InScan, from any starting list.
theorem foldl_extendUnique (acc : List Nat) (scans : List (List Nat)) :
    acc <+: scans.foldl extendUnique acc ∧
    (scans.foldl extendUnique acc).Sublist (acc ++ scans.flatten) ∧
    (acc.Nodup → (scans.foldl extendUnique acc).Nodup) ∧
    (∀ y, y ∈ scans.foldl extendUnique acc ↔ y ∈ acc ∨ y ∈ scans.flatten) := by
  induction scans generalizing acc with
  -- no values: the starting list;
  | nil => simp
  | cons s rest ih =>
    -- one value's scan, then the rest from there.
    obtain ⟨hp, hs, hn, hm⟩ := ih (extendUnique acc s)
    refine ⟨(extendUnique_prefix acc s).trans hp, ?_, ?_, ?_⟩
    -- scan order: the first value's sublist, then the rest's;
    · have := List.Sublist.append_right (extendUnique_sublist acc s) rest.flatten
      simpa [List.append_assoc] using hs.trans this
    -- no repeats: the first value keeps the list repeat-free, the rest too;
    · exact fun h => hn (extendUnique_nodup acc s h)
    -- coverage: what the first value listed, plus what the rest did.
    · intro y
      rw [List.foldl_cons, hm y, mem_extendUnique]
      simp [List.mem_flatten, or_assoc]

-- Promise: an InScan lists each matched document once, in scan order across
-- the `_in` values, each at its first encounter.
theorem inScan_scan_order_without_repeats (scans : List (List Nat)) :
    (inScan scans).Sublist scans.flatten ∧ (inScan scans).Nodup ∧
    (∀ y, y ∈ inScan scans ↔ y ∈ scans.flatten) := by
  -- the general promises, from an empty start.
  obtain ⟨_, hs, hn, hm⟩ := foldl_extendUnique [] scans
  exact ⟨by simpa using hs, hn List.nodup_nil, fun y => by simpa using hm y⟩

-- Promise, on the Rust unit test's input: values scanning [3, 1] and [1, 2]
-- give [3, 1, 2], the repeated 1 kept where it first appeared.
theorem inScan_example : inScan [[3, 1], [1, 2]] = [3, 1, 2] := by
  -- run the definition.
  decide

end IndexEqualKeyOrder
