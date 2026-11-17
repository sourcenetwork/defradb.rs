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

Rust anchors: `crates/db/src/read/index_order.rs` `apply_exact_match_page`,
`extend_unique`, `resolve_bound` and `resolve_in_order`; the ExactMatch and InScan arms of `get_by_index_scan` in
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

/-!
## Resolving only the page

An ExactMatch scans its whole equal-key set (so `indexFetches` counts every
entry), but `resolve_in_order` maps short IDs to DocIDs only until
`offset + limit` of them are mapped (`resolve_bound`); IDs with no mapping
are skipped and do not count. The page cut from that shorter list must be
the page full resolution would give.
-/

-- Full resolution: every short ID with a mapping, in scan order.
def resolveAll (lookup : Nat → Option Nat) (ids : List Nat) : List Nat :=
  -- unmapped IDs drop out.
  ids.filterMap lookup

-- Bounded resolution: the first `bound` mapped IDs, none past them.
def resolveUpTo (lookup : Nat → Option Nat) (ids : List Nat) (bound : Nat) :
    List Nat :=
  -- stopping after `bound` mapped IDs keeps exactly the first `bound`.
  (resolveAll lookup ids).take bound

-- Promise: resolving only `offset + limit` mapped IDs gives the same page as
-- resolving them all, whatever IDs are unmapped.
theorem resolveUpTo_same_page (lookup : Nat → Option Nat) (ids : List Nat)
    (offset limit : Nat) :
    page offset (some limit) (resolveUpTo lookup ids (offset + limit)) =
      page offset (some limit) (resolveAll lookup ids) := by
  -- dropping `offset` from the first `offset + limit` leaves the first
  -- `limit` of the rest, and taking `limit` of that changes nothing.
  simp [page, resolveUpTo, List.drop_take, Nat.add_sub_cancel_left, List.take_take]

/-!
## A lookup on the leading field of a composite key

The story, in plain words. A composite index key holds the leading field's
value, then the later fields' values, then the document's short ID. A join or
a filter through a relation knows only the leading value: the related
document's ID. The lookup takes every entry whose key starts with that value.

What must hold: each entry's short ID is read from the end of its key, after
all of the index's fields, so the lookup returns exactly the short IDs of the
entries with that leading value, in key order. Go does the same with a prefix
iterator (`newPrefixBasedMatchIteratorFromConditions`).

What must never happen: reading the part right after the leading value as the
short ID. On a composite key that part is the next field's value
(`afterLead_reads_a_field`), so the old reading lost every row or failed.

Rust anchors: `crates/storage/src/index/eq_iterator.rs`
`ExactMatchIterator::new_leading`; `crates/storage/src/index/range_iterator.rs`
`RangeIterator::new_prefix`.
-/

-- One composite entry: the leading value, the later values, the short ID.
structure CEntry where
  -- the leading field's value (the related document's ID, for a join)
  lead : Nat
  -- the values of the index's later fields, in field order
  rest : List Nat
  -- the document's short ID, the key's last part
  shortId : Nat
deriving DecidableEq, Repr

-- The stored key: the leading value, the later values, then the short ID.
def CEntry.key (e : CEntry) : List Nat := e.lead :: e.rest ++ [e.shortId]

-- Read the short ID after skipping `fields` values of the key.
def readShortId (fields : Nat) (key : List Nat) : Option Nat :=
  -- what follows the skipped values, if anything does.
  (key.drop fields).head?

-- Promise: skipping all of the index's fields reads the entry's short ID.
theorem readShortId_after_all_fields (e : CEntry) :
    readShortId (e.rest.length + 1) e.key = some e.shortId := by
  -- the leading value and the later values drop away, leaving the short ID.
  simp [readShortId, CEntry.key, List.drop_append]

-- Promise (the bug): skipping only the leading value reads the next field's
-- value instead, whenever the index has a later field.
theorem afterLead_reads_a_field (e : CEntry) (h : e.rest ≠ []) :
    readShortId 1 e.key = e.rest.head? := by
  -- what follows the leading value is the later values, which are not empty.
  cases hr : e.rest with
  | nil => exact absurd hr h
  | cons r rs => simp [readShortId, CEntry.key, hr]

-- Promise, on one entry: book 9 by author 5 titled 7 reads as 7, not 9.
theorem afterLead_example :
    readShortId 1 (CEntry.key ⟨5, [7], 9⟩) ≠ some 9 := by
  -- the part after the leading value is the title.
  decide

-- The lookup: the entries with leading value `v`, in scan order, each read
-- after all of its fields.
def leadingLookup (scan : List CEntry) (v : Nat) : List Nat :=
  -- an entry with another leading value adds nothing.
  scan.filterMap (fun e => if e.lead = v then readShortId (e.rest.length + 1) e.key else none)

-- Promise: the lookup is the matching entries' short IDs, in scan order.
theorem leadingLookup_eq (scan : List CEntry) (v : Nat) :
    leadingLookup scan v = (scan.filter (fun e => e.lead = v)).map CEntry.shortId := by
  induction scan with
  -- an empty scan finds nothing;
  | nil => rfl
  -- each entry is either read correctly or skipped, as the filter does.
  | cons e rest ih =>
    by_cases he : e.lead = v
    · simp [leadingLookup, List.filterMap_cons, he, readShortId_after_all_fields] at ih ⊢
      exact ih
    · simp [leadingLookup, List.filterMap_cons, he] at ih ⊢
      exact ih

-- Promise: every entry with the leading value is found, and nothing else is.
theorem leadingLookup_exact (scan : List CEntry) (v x : Nat) :
    x ∈ leadingLookup scan v ↔ ∃ e ∈ scan, e.lead = v ∧ e.shortId = x := by
  -- membership of a filtered map: some kept entry carries `x`.
  rw [leadingLookup_eq]
  simp [List.mem_map, List.mem_filter, and_assoc]

-- Promise: the lookup keeps the scan's key order and only leaves entries out.
theorem leadingLookup_keeps_key_order (scan : List CEntry) (v : Nat) :
    (leadingLookup scan v).Sublist (scan.map CEntry.shortId) := by
  -- a filter is a sublist, and mapping keeps that.
  rw [leadingLookup_eq]
  exact (List.filter_sublist scan).map CEntry.shortId

/-!
## A relation in an index is its ID field

The story, in plain words. A document stores a relation as its ID field: a
book keeps `_authorID`, and has no stored `author` value. An index declared on
`author` must therefore index `_authorID`, as Go does
(`checkExistingFieldsAndAdjustRelFieldNames`).

What must never happen: an index that reads `author` itself. Every book then
indexes a missing value, and a lookup by any author's ID finds nothing
(`raw_relation_index_finds_nothing`).

Rust anchors: `crates/schema/src/relation.rs` `CollectionVersion::index_field_name`;
`crates/query/src/sdl_parse/builder.rs` `build_collection`;
`crates/db/src/index/manager/mod.rs` `IndexManager::create_index_of_kind`.
-/

-- A field of a collection: its name, and whether it is a single relation.
structure Field where
  -- the field's name
  name : String
  -- true for a relation such as `author` (not a list of related documents)
  relation : Bool
deriving DecidableEq, Repr

-- The ID field a relation is stored under: `author` becomes `_authorID`.
def idFieldName (name : String) : String := "_" ++ name ++ "ID"

-- The field an index reads for `name`: a relation's ID field, else `name`.
def indexFieldName (fields : List Field) (name : String) : String :=
  -- only a declared single relation is renamed.
  if fields.any (fun f => f.name == name && f.relation) then idFieldName name else name

-- A stored document: the value under each stored field, if any.
abbrev Doc := String → Option Nat

-- The documents an index on `field` finds for value `v`.
def lookup (docs : List Doc) (field : String) (v : Nat) : List Doc :=
  -- a document matches when its stored value under `field` is `v`.
  docs.filter (fun d => d field == some v)

-- Promise: a relation is indexed by its ID field.
theorem relation_indexed_by_id (fields : List Field) (name : String)
    (h : fields.any (fun f => f.name == name && f.relation)) :
    indexFieldName fields name = idFieldName name := by
  -- the relation branch is taken.
  simp [indexFieldName, h]

-- Promise: a book that stores author 5 under `_authorID` is found by an index
-- on `author`, and an index that read `author` itself would find nothing.
theorem raw_relation_index_finds_nothing :
    let book : Doc := fun f => if f = "_authorID" then some 5 else none
    let fields := [Field.mk "author" true, Field.mk "_authorID" false]
    (lookup [book] (indexFieldName fields "author") 5).length = 1 ∧
      (lookup [book] "author" 5).length = 0 := by
  -- run the definitions on the one book.
  simp [lookup, indexFieldName, idFieldName]

/-!
## A join through a relation orders by that relation only

The story, in plain words. To answer `order: [{author: {name: ASC}}, {title:
ASC}]`, the planner may walk authors in name order and look each author's
books up in an index. The books of one author come back in index key order,
which may be by `pages`, not by `title`.

What must hold: the planner skips the final sort only when every order
condition goes through the relation; otherwise the later conditions are
sorted after the join (`join_orders_fully` in `assemble_type_join_one`).

What must never happen: trusting the join for `title` too. Rows in author
order can still be out of title order (`author_order_is_not_title_order`).

Rust anchor: `crates/query/src/planner/joins/join_one.rs` `assemble_type_join_one`.
-/

-- A joined row: the author's name and the book's title, as order keys.
structure Row where
  -- the author's name, the relation's order key
  author : Nat
  -- the book's title, a later order key
  title : Nat
deriving DecidableEq, Repr

-- Rows in order by author only.
def ByAuthor (rows : List Row) : Prop := rows.Pairwise (fun a b => a.author ≤ b.author)

-- Rows in order by author, then title.
def ByAuthorTitle (rows : List Row) : Prop :=
  -- an earlier row has a smaller author, or the same author and a title no larger.
  rows.Pairwise (fun a b => a.author < b.author ∨ (a.author = b.author ∧ a.title ≤ b.title))

-- Whether the join's order is the whole requested order: the conditions
-- through the relation are all the conditions.
def joinOrdersFully (relationConditions allConditions : Nat) : Bool :=
  -- the same count means no condition is left outside the relation.
  relationConditions == allConditions

-- Promise: the join is trusted only when nothing is left to sort.
theorem joinOrdersFully_iff (r a : Nat) : joinOrdersFully r a = true ↔ r = a := by
  -- the comparison is plain equality.
  simp [joinOrdersFully]

-- Promise (the bug): rows in author order need not be in title order, so a
-- query that also orders by title still needs its sort.
theorem author_order_is_not_title_order :
    ByAuthor [⟨1, 9⟩, ⟨1, 3⟩] ∧ ¬ ByAuthorTitle [⟨1, 9⟩, ⟨1, 3⟩] := by
  -- the same author, with the larger title first.
  simp [ByAuthor, ByAuthorTitle]

end IndexEqualKeyOrder
