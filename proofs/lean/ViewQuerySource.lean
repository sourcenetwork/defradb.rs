/-!
# A view's query source keeps the transform it was given

The story, in plain words. A view is a collection whose rows come from a
query. When a caller adds a view it may name a lens transform by its CID, and
the node accepts the CID only if it is not empty and the node holds that lens.
The accepted CID goes on the view's query source, and the query source is
written into the stored definition as JSON: an object with a `Query` member and,
only when there is a transform, a `Transform` member holding the CID as a
string. When the node reads the definition back, `Transform` missing or `null`
means no transform.

Who acts: the caller, who names a CID or none; the node, which checks it,
stores the definition and reads it back.

What must never happen:
* a query source, with or without a transform, reads back as something else;
* a stored transform differs from the CID the caller named, is empty, or names
  a lens the node did not hold when it accepted it.

Rust owner: crates/schema/src/source.rs `QuerySource` (its serde derive:
`Transform` is `skip_serializing_if = "Option::is_none"` and `default`;
`Query` reads through `deserialize_query_select`) and `QuerySource::with_transform`;
crates/schema/src/collection.rs `CollectionVersion::query` (`Query`, skipped when
none, `default`); crates/defra-node/src/db_impls.rs `DbSchemaOps::add_view`,
which checks the CID; the definition store writes `serde_json::to_vec` of the
`CollectionVersion` and reads it with `serde_json::from_slice`
(crates/db/src/collection/ops/version.rs `get_all_collection_versions_with_txn`).
-/

namespace ViewQuerySource

/-! ## 1. JSON, as serde_json writes and reads it -/

/-- A JSON value: the shapes serde_json reads and writes. -/
inductive Json where
  -- `null`
  | null
  -- `true` or `false`
  | bool (b : Bool)
  -- a number
  | num (n : Int)
  -- a string
  | str (s : String)
  -- an array, in order
  | arr (items : List Json)
  -- an object: its members, in the order they were written
  | obj (members : List (String × Json))

/-- The value of member `k` of an object: the first member with that name,
or nothing if there is none. -/
def member (k : String) : List (String × Json) → Option Json
  -- no members left: `k` is not there
  | [] => none
  -- this member is `k`: its value; otherwise look further on
  | (k', v) :: rest => if k' = k then some v else member k rest

/-- Looking past members that are not `k` changes nothing: if the first list
has no `k`, member `k` of both lists is member `k` of the second. -/
theorem member_append (k : String) (xs ys : List (String × Json))
    (h : member k xs = none) : member k (xs ++ ys) = member k ys := by
  -- walk the first list, one member at a time
  induction xs with
  -- nothing in front: the lists are the same
  | nil => rfl
  -- a member in front: it is not `k`, so it is skipped
  | cons x rest ih =>
    obtain ⟨k', v⟩ := x
    -- split on whether the front member is `k`
    by_cases hk : k' = k
    -- if it were `k`, the first list would have a `k`
    · simp [member, hk] at h
    -- it is not: both sides skip it
    · simp [member, hk] at h ⊢; exact ih h

/-! ## 2. The query source and its encoding -/

/-- A query source: the view's query (as JSON) and the transform CID, if any. -/
structure QuerySource where
  -- the base query, a Go `request.Select` as JSON
  query : Json
  -- the lens transform's CID, or none
  transform : Option String

/-- The JSON serde writes for a query source: `Query`, then `Transform` only
when there is one (`skip_serializing_if = "Option::is_none"`). -/
def encode (qs : QuerySource) : Json :=
  .obj (("Query", qs.query) ::
    -- no transform: the member is left out
    match qs.transform with
    | none => []
    -- a transform: its CID, as a string
    | some c => [("Transform", .str c)])

/-- How serde reads `Transform: Option<String>` with `#[serde(default)]`:
missing or `null` is no transform, a string is that CID, and anything else
fails to decode. -/
def decodeTransform : Option Json → Option (Option String)
  -- the member is missing: the default, no transform
  | none => some none
  -- `null`: no transform
  | some .null => some none
  -- a string: that CID
  | some (.str c) => some (some c)
  -- any other value: a decode error
  | some _ => none

/-- How serde reads a query source back. `norm` is what
`deserialize_query_select` does to the query: it rewrites it in Go's
`request.Select` form, or fails. -/
def decode (norm : Json → Option Json) : Json → Option QuerySource
  -- an object: read `Query` (required) and `Transform`
  | .obj ms =>
    match member "Query" ms with
    -- no `Query`: a decode error
    | none => none
    | some q =>
      match norm q, decodeTransform (member "Transform" ms) with
      -- both read: the query source
      | some q', some t => some ⟨q', t⟩
      -- either failed: a decode error
      | _, _ => none
  -- anything but an object: a decode error
  | _ => none

/-- The two member names differ, so looking up one never finds the other. -/
theorem query_ne_transform : ("Query" = "Transform") = False := by
  -- string literals compare by their characters
  exact eq_false (by decide)

/-- Reading back what was written gives the same transform, whatever the
query: the transform never changes on the way through. -/
theorem transform_round_trips (norm : Json → Option Json) (qs qs' : QuerySource)
    (h : decode norm (encode qs) = some qs') : qs'.transform = qs.transform := by
  obtain ⟨q, t⟩ := qs
  -- the two shapes: with and without a transform
  cases t <;> cases hn : norm q <;>
    simp [encode, decode, member, decodeTransform, query_ne_transform, hn] at h <;>
    (subst h; rfl)

/-- A query source in Go's form, with or without a transform, reads back as
exactly itself. -/
theorem round_trip (norm : Json → Option Json) (qs : QuerySource)
    (hq : norm qs.query = some qs.query) : decode norm (encode qs) = some qs := by
  obtain ⟨q, t⟩ := qs
  -- the two shapes: with and without a transform
  cases t <;> simp at hq <;>
    simp [encode, decode, member, decodeTransform, query_ne_transform, hq]

/-- A missing `Transform` and a `null` one both read as no transform. -/
theorem missing_or_null_is_none (norm : Json → Option Json) (q q' : Json)
    (hq : norm q = some q') :
    decode norm (.obj [("Query", q)]) = some ⟨q', none⟩ ∧
    decode norm (.obj [("Query", q), ("Transform", .null)]) = some ⟨q', none⟩ := by
  -- both are the default: no transform
  simp [decode, member, decodeTransform, query_ne_transform, hq]

/-! ## 3. The definition the query source is stored in -/

/-- The JSON of a stored definition: its other members, then `Query` only when
the collection is a view (`skip_serializing_if = "Option::is_none"`). -/
def encodeDefinition (others : List (String × Json)) (q : Option QuerySource) : Json :=
  .obj (others ++
    -- not a view: the member is left out
    match q with
    | none => []
    -- a view: its query source
    | some qs => [("Query", encode qs)])

/-- How serde reads a definition's `Query: Option<QuerySource>` with
`#[serde(default)]`: missing or `null` is none, an object is a query source. -/
def decodeDefinitionQuery (norm : Json → Option Json) : Json → Option (Option QuerySource)
  | .obj ms =>
    match member "Query" ms with
    -- missing: not a view
    | none => some none
    -- `null`: not a view
    | some .null => some none
    -- anything else is read as a query source
    | some j => (decode norm j).map some
  -- anything but an object: a decode error
  | _ => none

/-- A definition's query source, in Go's form, reads back as exactly itself,
given its other members have no `Query` of their own. -/
theorem definition_round_trip (norm : Json → Option Json)
    (others : List (String × Json)) (qs : QuerySource)
    (ho : member "Query" others = none) (hq : norm qs.query = some qs.query) :
    decodeDefinitionQuery norm (encodeDefinition others (some qs)) = some (some qs) := by
  -- skip the other members, then read the query source
  simp only [decodeDefinitionQuery, encodeDefinition, member_append _ _ _ ho]
  -- the query source as written, and what reading it gives back
  have h := round_trip norm qs hq
  simp only [encode] at h ⊢
  simp [member, h]

/-- A definition with no view query reads back as no view query. -/
theorem definition_without_query (norm : Json → Option Json)
    (others : List (String × Json)) (ho : member "Query" others = none) :
    decodeDefinitionQuery norm (encodeDefinition others none) = some none := by
  -- nothing after the other members, and they hold no `Query`
  simp [decodeDefinitionQuery, encodeDefinition, ho]

/-! ## 4. What add_view accepts, and what it stores -/

/-- Why add_view refuses a transform. -/
inductive Refusal where
  -- `ViewTransformCidEmpty`: the CID is empty
  | empty
  -- `ViewTransformMissing`: the node holds no lens with this CID
  | missing (cid : String)

/-- add_view's transform check: no transform passes as none; an empty CID is
refused; a CID the node holds passes; any other is refused. `holds` is the
lens store's `has_transform`. -/
def accept (holds : String → Bool) : Option String → Except Refusal (Option String)
  -- no transform asked for: none
  | none => .ok none
  | some c =>
    -- the empty CID: refused
    if c = "" then .error .empty
    -- a lens the node holds: accepted as it is
    else if holds c then .ok (some c)
    -- anything else: refused, naming the CID
    else .error (.missing c)

/-- The query source add_view builds: `QuerySource::new`, then
`with_transform` when a CID was accepted. -/
def build (query : Json) : Option String → QuerySource
  -- no transform: the bare query source
  | none => ⟨query, none⟩
  -- `with_transform(cid)`: the CID set on it
  | some c => { (⟨query, none⟩ : QuerySource) with transform := some c }

/-- What add_view accepts is exactly what the caller asked for, and an
accepted CID is not empty and names a lens the node held. -/
theorem accepted_is_asked (holds : String → Bool) (t r : Option String)
    (h : accept holds t = .ok r) :
    r = t ∧ ∀ c, r = some c → c ≠ "" ∧ holds c = true := by
  cases t with
  -- no transform: accepted as none
  | none => simp [accept] at h; subst h; simp
  | some c =>
    -- split on the two checks; the refusals cannot be `ok`
    by_cases he : c = "" <;> by_cases hh : holds c = true <;>
      simp [accept, he, hh] at h <;> (subst h; simp [he, hh])

/-- The headline: a view added with transform `t`, stored and read back, names
exactly `t`, and if `t` is a CID it is not empty and names a lens the node held
when add_view accepted it. -/
theorem stored_transform_is_accepted (norm : Json → Option Json)
    (holds : String → Bool) (others : List (String × Json))
    (query : Json) (t r : Option String) (qs' : QuerySource)
    (ho : member "Query" others = none) (hq : norm query = some query)
    (ha : accept holds t = .ok r)
    (hd : decodeDefinitionQuery norm (encodeDefinition others (some (build query r)))
      = some (some qs')) :
    qs'.transform = t ∧ ∀ c, qs'.transform = some c → c ≠ "" ∧ holds c = true := by
  -- what was accepted is what was asked, and it is a held, non-empty CID
  obtain ⟨hr, hok⟩ := accepted_is_asked holds t r ha
  -- the built query source's query is the Go-form query
  have hb : norm (build query r).query = some (build query r).query := by
    cases r <;> simpa [build] using hq
  -- the definition reads back as the built query source
  rw [definition_round_trip norm others _ ho hb] at hd
  -- so the read transform is the built one, which is the accepted one
  have ht : qs'.transform = r := by
    cases r <;> simp [build] at hd <;> (subst hd; rfl)
  exact ⟨ht.trans hr, fun c hc => hok c (ht ▸ hc)⟩

end ViewQuerySource
