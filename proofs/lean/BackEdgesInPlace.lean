/-!
# BackEdgesInPlace: adding edges to an encoded record writes the same record

The story, in plain words. A DiskANN node's record lists its out-edges: for
each neighbour, its id and its code. A merge adds back-edges to many records.
It used to decode the record into a list, add each new edge to the list
(replace the code of an edge already there, else append it, and skip an edge
back to the node itself), then encode the list again. Decoding copies every
neighbour's code, so the code now edits the encoded bytes instead: it finds an
edge already there by id and overwrites its code, or appends the new edge and
adds one to the record's count.

A tiny example. Record: [(1, a)], count 1. Add (2, b), then (1, c). Decoded:
[(1, a)] → [(1, a), (2, b)] → [(1, c), (2, b)]. In place: append (2, b),
count 2; find 1, overwrite its code, count stays 2. Same entries, same count.

What must never happen: the bytes written in place differ from the bytes the
decode-add-encode path writes, or the count disagrees with the entries
(`in_place_eq_decoded`). A search that only looks at the entries the record
had before this call appends an edge twice when the new edges name it twice
(`stale_search_duplicates_an_edge`).

The encoded record is its own code, a count, then fixed-width entries of id
and code, so its bytes are exactly a list of entries plus a count; the model
works on that list. A position found among fixed-width entries is the
position in the list.

Rust anchors: `codec::add_edges` and `codec::decode_record`
(`crates/db/src/index/vector/engine/diskann/codec.rs`), called from
`DiskAnn::add_back_edges` (`crates/db/src/index/vector/engine/diskann/graph.rs`)
when codes are not resident. Checked against the decoded loop by
`adding_edges_in_place_writes_what_decoding_and_encoding_would`
(`crates/db/tests/vector/diskann_back_edges.rs`).
-/

namespace BackEdgesInPlace

-- An entry is a neighbour's id and its code.
abbrev Entry := Nat × Nat

-- Overwrite the code of the first entry with this id; the rest stays as is.
def replaceFirst (id code : Nat) : List Entry → List Entry
  -- nothing to overwrite in an empty list
  | [] => []
  -- the first entry with the id takes the new code and the search stops
  | e :: rest => if e.1 = id then (id, code) :: rest else e :: replaceFirst id code rest

-- The decoded path, one edge: skip an edge to the node itself, overwrite an
-- edge already there, else append the new one.
def addOne (target : Nat) (es : List Entry) (e : Entry) : List Entry :=
  -- an edge back to the node itself is never added
  if e.1 = target then es
  -- an id already present only changes its code
  else if es.any (fun x => x.1 = e.1) then replaceFirst e.1 e.2 es
  -- a new id goes on the end
  else es ++ [e]

-- The decoded path: every new edge in order.
def decoded (target : Nat) (es : List Entry) (edges : List Entry) : List Entry :=
  edges.foldl (addOne target) es

-- The encoded record: its entries and the count stored in its header.
structure Bytes where
  -- the fixed-width entries, in order
  entries : List Entry
  -- the count field the header carries
  count : Nat
  deriving DecidableEq

-- The in-place path, one edge: the same three cases, with the count bumped
-- only when an entry is appended.
def stepInPlace (target : Nat) (b : Bytes) (e : Entry) : Bytes :=
  -- an edge back to the node itself leaves the bytes alone
  if e.1 = target then b
  -- an id already present gets its code overwritten; the count stays
  else if b.entries.any (fun x => x.1 = e.1) then { b with entries := replaceFirst e.1 e.2 b.entries }
  -- a new id is appended and the count goes up by one
  else { entries := b.entries ++ [e], count := b.count + 1 }

-- The in-place path: every new edge in order, on the bytes.
def inPlace (target : Nat) (b : Bytes) (edges : List Entry) : Bytes :=
  edges.foldl (stepInPlace target) b

-- Overwriting a code never changes how many entries there are.
theorem replaceFirst_length (id code : Nat) :
    ∀ es : List Entry, (replaceFirst id code es).length = es.length
  -- the empty list stays empty
  | [] => rfl
  -- either this entry is replaced or the rest is, one for one
  | e :: rest => by
    unfold replaceFirst
    split <;> simp [replaceFirst_length id code rest]

-- One edge: the in-place step leaves the entries the decoded step would, and
-- a count that still matches them.
theorem step_matches (target : Nat) (b : Bytes) (e : Entry)
    (h : b.count = b.entries.length) :
    (stepInPlace target b e).entries = addOne target b.entries e ∧
      (stepInPlace target b e).count = (stepInPlace target b e).entries.length := by
  unfold stepInPlace addOne
  -- the same three cases on both sides
  split
  · exact ⟨rfl, h⟩
  · split
    · simp [replaceFirst_length, h]
    · simp [h]

-- Every edge: editing the bytes in place leaves the entries decoding, adding
-- and encoding would, with a count that matches them, from any record whose
-- count matches its entries.
theorem in_place_eq_decoded (target : Nat) :
    ∀ (edges : List Entry) (b : Bytes), b.count = b.entries.length →
      (inPlace target b edges).entries = decoded target b.entries edges ∧
        (inPlace target b edges).count = (decoded target b.entries edges).length
  -- no edges: nothing changes on either side
  | [], b, h => ⟨rfl, h⟩
  -- one edge, then the rest from the record that edge leaves
  | e :: rest, b, h => by
    have ⟨h1, h2⟩ := step_matches target b e h
    have ih := in_place_eq_decoded target rest (stepInPlace target b e) h2
    simp only [inPlace, decoded, List.foldl_cons] at *
    rw [h1] at ih
    exact ih

-- The planted bug: searching only the entries the record had before this
-- call, so an id appended earlier in the same call is not found again.
def stepStale (target : Nat) (orig : List Entry) (b : Bytes) (e : Entry) : Bytes :=
  -- an edge back to the node itself leaves the bytes alone
  if e.1 = target then b
  -- only the original entries are searched
  else if orig.any (fun x => x.1 = e.1) then { b with entries := replaceFirst e.1 e.2 b.entries }
  -- everything else is appended, even an id this call already appended
  else { entries := b.entries ++ [e], count := b.count + 1 }

-- The buggy path: every new edge, searching the original entries only.
def stale (target : Nat) (b : Bytes) (edges : List Entry) : Bytes :=
  edges.foldl (stepStale target b.entries) b

-- Adding the same new neighbour twice to an empty record: the stale search
-- lists it twice, where the decoded path lists it once with its last code.
theorem stale_search_duplicates_an_edge :
    (stale 0 ⟨[], 0⟩ [(1, 5), (1, 6)]).entries ≠ decoded 0 [] [(1, 5), (1, 6)] := by
  decide

end BackEdgesInPlace
