import BackEdgesInPlace

/-!
# ResidentBackEdges: back-edges on an ids-only record write the same record

The story, in plain words. With resident codes a DiskANN node's record holds
only its neighbours' ids; each node's code lives under its own key. A merge
adds back-edges to many records. It used to look up the code of every
neighbour in the record (dropping a neighbour whose code is gone, because a
consolidation removed it), add the new edges to that list of ids and codes,
prune the list if it grew past the slack limit, and write back only the ids.
Every code it looked up was thrown away unless it pruned.

The code now asks only whether each neighbour still has a code, keeps the ids
that do, adds the new edges' ids, and writes the ids at once when the record
stays within the slack. Only a record that must be pruned has its codes
looked up, from the same ids, and is pruned as before.

A tiny example. Record [1, 3], node 3 consolidated away (no code), slack 2,
new edge (2, b). Before: codes of 1 and 3 looked up, 3 dropped, list
[(1, a)] → [(1, a), (2, b)], two edges, not pruned, ids [1, 2] written. Now:
1 kept, 3 dropped, [1] → [1, 2], two ids, written. The same record.

What must never happen: the ids written differ from the ids the old path
wrote (`resident_eq_decoded`), or the record's count disagrees with its ids
(`in_place_ids_eq_added`). Editing the ids in place without the filter, as the
records-with-codes path does, keeps a neighbour that was consolidated away
(`skipping_the_filter_keeps_a_consolidated_neighbour`).

The record's bytes are a count and then fixed-width ids, so the model works
on that list of ids; the count is carried beside it.

Rust anchors: `codec::add_ids` and `codec::encode_id_list`
(`crates/database/db/src/index/vector/engine/diskann/codec.rs`) and
`DiskAnn::add_resident_back_edges` and `DiskAnn::presence`
(`crates/database/db/src/index/vector/engine/diskann/graph.rs`). Checked by
`resident_records_are_the_records_with_codes_left_out` and
`a_back_edge_drops_a_neighbour_consolidated_away`
(`crates/database/db/tests/vector/diskann_resident_back_edges.rs`) and
`adding_ids_in_place_writes_what_adding_edges_and_dropping_codes_would`
(`crates/database/db/tests/vector/diskann_back_edges.rs`).
-/

namespace ResidentBackEdges

open BackEdgesInPlace

-- The ids-only path, one edge: skip an edge to the node itself, leave an id
-- already there alone, else append the new id.
def addId (target : Nat) (ids : List Nat) (i : Nat) : List Nat :=
  -- an edge back to the node itself is never added
  if i = target then ids
  -- an id already present stays where it is
  else if ids.any (fun x => x = i) then ids
  -- a new id goes on the end
  else ids ++ [i]

-- The ids-only path: every new edge's id in order.
def addIds (target : Nat) (ids : List Nat) (edges : List Nat) : List Nat :=
  edges.foldl (addId target) ids

-- Overwriting a code never changes which ids the entries carry.
theorem replaceFirst_ids (id code : Nat) :
    ∀ es : List Entry, (replaceFirst id code es).map Prod.fst = es.map Prod.fst
  -- the empty list stays empty
  | [] => rfl
  -- the replaced entry keeps its id, because it was found by that id
  | e :: rest => by
    unfold replaceFirst
    split
    · next h => simp [h]
    · simp [replaceFirst_ids id code rest]

-- One edge: adding it with codes and then forgetting the codes leaves the
-- ids that adding only its id would.
theorem addOne_ids (target : Nat) (es : List Entry) (e : Entry) :
    (addOne target es e).map Prod.fst = addId target (es.map Prod.fst) e.1 := by
  unfold addOne addId
  -- the same three cases on both sides; the membership tests agree because
  -- they look at the same ids
  have hany : (es.map Prod.fst).any (fun x => x = e.1) = es.any (fun x => x.1 = e.1) := by
    simp [List.any_map, Function.comp_def]
  split
  · rfl
  · rw [hany]
    split
    · exact replaceFirst_ids e.1 e.2 es
    · simp

-- Every edge: the ids of the decoded path are the ids-only path's ids.
theorem decoded_ids (target : Nat) :
    ∀ (edges : List Entry) (es : List Entry),
      (decoded target es edges).map Prod.fst = addIds target (es.map Prod.fst) (edges.map Prod.fst)
  -- no edges: both sides are the record's own ids
  | [], es => rfl
  -- one edge, then the rest from the record that edge leaves
  | e :: rest, es => by
    simp only [decoded, addIds, List.foldl_cons, List.map_cons] at *
    have ih := decoded_ids target rest (addOne target es e)
    simp only [decoded, addIds] at ih
    rw [ih, addOne_ids]

-- A neighbour with a code, paired with it; one without is left out. This is
-- what looking up every neighbour's code builds.
def withCodes (code : Nat → Option Nat) (ids : List Nat) : List Entry :=
  ids.filterMap (fun i => (code i).map (fun c => (i, c)))

-- The neighbours that still have a code, without their codes: what the new
-- path keeps by asking only whether each code is there.
def kept (code : Nat → Option Nat) (ids : List Nat) : List Nat :=
  ids.filter (fun i => (code i).isSome)

-- Asking whether each code is there keeps exactly the ids that looking the
-- codes up keeps.
theorem withCodes_ids (code : Nat → Option Nat) :
    ∀ ids : List Nat, (withCodes code ids).map Prod.fst = kept code ids
  -- no neighbours: nothing kept either way
  | [] => rfl
  -- one neighbour: kept by both or dropped by both, then the rest
  | i :: rest => by
    have ih := withCodes_ids code rest
    unfold withCodes kept at *
    cases h : code i <;> simp [List.filterMap_cons, List.filter_cons, h, ih]

-- Looking the codes up again from the kept ids finds the same entries as
-- looking them up from every id: a dropped id had no code to find.
theorem withCodes_kept (code : Nat → Option Nat) :
    ∀ ids : List Nat, withCodes code (kept code ids) = withCodes code ids
  -- no neighbours: nothing found either way
  | [] => rfl
  -- one neighbour: found by both or by neither, then the rest
  | i :: rest => by
    have ih := withCodes_kept code rest
    unfold withCodes kept at *
    cases h : code i <;> simp [List.filterMap_cons, List.filter_cons, h, ih]

-- The old path: look up every neighbour's code, add the edges, prune past
-- the slack, and write the ids.
def oldWrite (code : Nat → Option Nat) (prune : List Entry → List Entry)
    (slack target : Nat) (ids : List Nat) (edges : List Entry) : List Nat :=
  -- every neighbour with its code, one without left out
  let added := decoded target (withCodes code ids) edges
  -- pruned only past the slack, then only the ids are written
  (if slack < added.length then prune added else added).map Prod.fst

-- The new path: keep the ids that have a code, add the edges' ids, and write
-- them when within the slack; past it, look the codes up and prune as before.
def newWrite (code : Nat → Option Nat) (prune : List Entry → List Entry)
    (slack target : Nat) (ids : List Nat) (edges : List Entry) : List Nat :=
  -- the ids alone, no code looked up
  let added := addIds target (kept code ids) (edges.map Prod.fst)
  -- within the slack these are the record
  if added.length ≤ slack then added
  -- past it, the codes of the kept ids are looked up and the list pruned
  else (prune (decoded target (withCodes code (kept code ids)) edges)).map Prod.fst

-- For any prune and any set of codes still present, the new path writes the
-- ids the old path wrote.
theorem resident_eq_decoded (code : Nat → Option Nat) (prune : List Entry → List Entry)
    (slack target : Nat) (ids : List Nat) (edges : List Entry) :
    newWrite code prune slack target ids edges = oldWrite code prune slack target ids edges := by
  unfold newWrite oldWrite
  -- the ids the new path adds are the old path's entries without their codes
  have hids : addIds target (kept code ids) (edges.map Prod.fst)
      = (decoded target (withCodes code ids) edges).map Prod.fst := by
    rw [decoded_ids, withCodes_ids]
  -- so both paths count the same edges against the slack
  have hlen : (addIds target (kept code ids) (edges.map Prod.fst)).length
      = (decoded target (withCodes code ids) edges).length := by
    rw [hids, List.length_map]
  rw [withCodes_kept]
  by_cases h : slack < (decoded target (withCodes code ids) edges).length
  · -- past the slack both prune the same entries
    have : ¬ (addIds target (kept code ids) (edges.map Prod.fst)).length ≤ slack := by omega
    simp [h, this]
  · -- within it both write the same ids, unpruned
    have : (addIds target (kept code ids) (edges.map Prod.fst)).length ≤ slack := by omega
    simp [h, this, hids]

-- The encoded ids-only record: its ids and the count stored in its header.
structure IdBytes where
  -- the fixed-width ids, in order
  ids : List Nat
  -- the count field the header carries
  count : Nat

-- The in-place edit, one edge: the same three cases, with the count bumped
-- only when an id is appended.
def stepIds (target : Nat) (b : IdBytes) (i : Nat) : IdBytes :=
  -- an edge back to the node itself leaves the bytes alone
  if i = target then b
  -- an id already present leaves the bytes alone
  else if b.ids.any (fun x => x = i) then b
  -- a new id is appended and the count goes up by one
  else { ids := b.ids ++ [i], count := b.count + 1 }

-- The in-place edit: every new edge's id in order, on the bytes.
def inPlaceIds (target : Nat) (b : IdBytes) (edges : List Nat) : IdBytes :=
  edges.foldl (stepIds target) b

-- Editing the ids in place leaves the ids the list path adds, with a count
-- that matches them, from any record whose count matches its ids.
theorem in_place_ids_eq_added (target : Nat) :
    ∀ (edges : List Nat) (b : IdBytes), b.count = b.ids.length →
      (inPlaceIds target b edges).ids = addIds target b.ids edges ∧
        (inPlaceIds target b edges).count = (inPlaceIds target b edges).ids.length
  -- no edges: nothing changes on either side
  | [], b, h => ⟨rfl, h⟩
  -- one edge, then the rest from the record that edge leaves
  | i :: rest, b, h => by
    -- the step leaves the ids `addId` would, with a matching count
    have hstep : (stepIds target b i).ids = addId target b.ids i ∧
        (stepIds target b i).count = (stepIds target b i).ids.length := by
      unfold stepIds addId
      split
      · exact ⟨rfl, h⟩
      · split
        · exact ⟨rfl, h⟩
        · simp [h]
    have ih := in_place_ids_eq_added target rest (stepIds target b i) hstep.2
    simp only [inPlaceIds, addIds, List.foldl_cons] at *
    rw [hstep.1] at ih
    exact ih

-- The planted bug: editing the ids in place without asking whether each
-- neighbour still has a code, as the records-with-codes path edits its bytes.
def unfilteredWrite (target : Nat) (ids : List Nat) (edges : List Entry) : List Nat :=
  addIds target ids (edges.map Prod.fst)

-- Node 1 was consolidated away and has no code. Adding a back-edge from 2 to
-- node 0's record [1]: the unfiltered edit keeps 1, where the old path drops
-- it and writes only 2.
theorem skipping_the_filter_keeps_a_consolidated_neighbour :
    unfilteredWrite 0 [1] [(2, 5)]
      ≠ oldWrite (fun i => if i = 1 then none else some 0) id 8 0 [1] [(2, 5)] := by
  decide

end ResidentBackEdges
