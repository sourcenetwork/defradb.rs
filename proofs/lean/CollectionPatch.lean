/-!
# Splitting a patch by collection, and kinds that name their own collection

The story, in plain words. A client sends one JSON patch whose operations
may touch several collections: `/Users/Fields`, `/Books/Fields`, and so
on. The database splits the patch into one group per collection version, the
one each operation's path names first (by name or by version ID, resolved to
the version before the split, so here a "collection" is that resolved key),
and applies each group to its collection. This
file proves the split loses nothing and moves nothing: the group for a
collection holds exactly that collection's operations, in the order the patch
gave them. Interleaved operations (Book, Author, Book) still reach each
collection in patch order.

A patch may also add a field whose kind names the collection it lives on
(`"Kind": "Users"` on `/Users`). Go treats that as a self reference, the
`Self` kind. This file proves the substitution turns every such kind into
`Self`, keeps whether the field is a list, leaves every other kind alone, and
does nothing the second time.

What must never happen: an operation dropped, sent to the wrong collection,
or reordered; a kind naming the host collection left as a plain name (it
would be refused as "must specify 'Self' kind"); any other kind changed.

Rust owners:
  crates/db/src/definition/patch/targets.rs   group_by_target
  crates/db/src/definition/patch/validate.rs  validate_patched_schema (the
                                              self-reference substitution)
-/

namespace CollectionPatch

-- An operation is anything the patch carries; its contents do not matter to
-- the split, only the collection its path names.
variable {Op : Type}

-- Add one operation for collection `t` to the groups: append it to `t`'s
-- group when there is one, else start a new group for `t` at the end.
def insertOp : List (String × List Op) → String → Op → List (String × List Op)
  -- no group yet for `t`: it gets one, holding this operation
  | [], t, op => [(t, [op])]
  -- the first group is `t`'s: the operation goes at its end
  | (k, ops) :: rest, t, op =>
    if k = t then (k, ops ++ [op]) :: rest
    -- otherwise keep looking further along
    else (k, ops) :: insertOp rest t op

-- `group_by_target`: walk the patch in order, adding each operation to the
-- group of the collection it names.
def group (ops : List (String × Op)) : List (String × List Op) :=
  ops.foldl (fun gs p => insertOp gs p.1 p.2) []

-- The operations the groups hold for collection `t` (none when it has no
-- group).
def lookup : List (String × List Op) → String → List Op
  -- no groups: nothing for `t`
  | [], _ => []
  -- the first group is `t`'s, or `t`'s is further along
  | (k, ops) :: rest, t => if k = t then ops else lookup rest t

-- Adding an operation for `t'` adds it at the end of `t'`'s group and leaves
-- every other collection's group as it was.
theorem lookup_insertOp (gs : List (String × List Op)) (t' t : String) (op : Op) :
    lookup (insertOp gs t' op) t =
      if t' = t then lookup gs t ++ [op] else lookup gs t := by
  -- one group at a time
  induction gs with
  -- no groups: a new one for `t'` is the only one
  | nil => by_cases h : t' = t <;> simp [insertOp, lookup, h]
  -- a first group `k`, then the rest
  | cons g rest ih =>
    -- name the first group's collection and operations
    obtain ⟨k, ops⟩ := g
    -- the cases: is `k` the one we add to, and the one we read?
    by_cases hk : k = t' <;> by_cases ht : k = t <;> by_cases h : t' = t <;>
      simp_all [insertOp, lookup]

-- Walking more of the patch only appends to `t`'s group, and exactly the
-- operations that name `t`, in order.
theorem lookup_fold (ops : List (String × Op)) :
    ∀ (gs : List (String × List Op)) (t : String),
      lookup (ops.foldl (fun gs p => insertOp gs p.1 p.2) gs) t =
        lookup gs t ++ (ops.filter (fun p => p.1 = t)).map Prod.snd := by
  -- one operation at a time
  induction ops with
  -- an empty rest adds nothing
  | nil => intro gs t; simp
  -- the next operation `(t', op)`, then the rest
  | cons p rest ih =>
    intro gs t
    -- name the operation's collection and the operation
    obtain ⟨t', op⟩ := p
    -- the rest is walked from the groups with this operation added
    simp only [List.foldl_cons]
    rw [ih, lookup_insertOp]
    -- it lands in `t`'s group exactly when it names `t`
    by_cases h : t' = t <;> simp [h, List.filter_cons]

-- The split is exact: the group for collection `t` is the patch's
-- operations on `t`, every one of them, in patch order.
theorem group_exact (ops : List (String × Op)) (t : String) :
    lookup (group ops) t = (ops.filter (fun p => p.1 = t)).map Prod.snd := by
  -- the walk starts from no groups
  simp [group, lookup_fold, lookup]

-- A field kind, as far as relations go.
inductive Kind where
  -- any kind that is not a relation
  | scalar (n : Nat)
  -- a relation to a collection named in the patch
  | named (name : String) (array : Bool)
  -- a relation to the host collection (or one of its set)
  | selfRef (relative : String) (array : Bool)
  -- a relation to a collection by its ID
  | relation (id : String) (array : Bool)
  deriving DecidableEq, Repr

-- Go's substituteRelationFieldKinds for a lone collection: a kind naming
-- `host` becomes `Self`, keeping whether it is a list.
def substitute (host : String) : Kind → Kind
  -- a name: the host's becomes `Self`, any other stays
  | .named n a => if n = host then .selfRef "" a else .named n a
  -- every other kind stays
  | k => k

-- After the substitution no kind names the host collection.
theorem no_host_name (host : String) (k : Kind) (a : Bool) :
    substitute host k ≠ .named host a := by
  -- by the kind's shape
  cases k with
  -- a name: either it was the host's (now `Self`) or it is another name
  | named n b => by_cases h : n = host <;> simp [substitute, h]
  -- any other shape is untouched and is not a name
  | _ => simp [substitute]

-- A kind naming the host becomes `Self`, and keeps whether it is a list.
theorem host_becomes_self (host : String) (a : Bool) :
    substitute host (.named host a) = .selfRef "" a := by
  -- the name is the host's
  simp [substitute]

-- Every kind that does not name the host is left alone.
theorem others_unchanged (host : String) (k : Kind)
    (h : ∀ a, k ≠ .named host a) : substitute host k = k := by
  -- by the kind's shape
  cases k with
  -- a name other than the host's
  | named n b =>
    -- it is not the host's name, or `h` would be contradicted
    have hn : n ≠ host := fun e => h b (by rw [e])
    simp [substitute, hn]
  -- any other shape is untouched
  | _ => rfl

-- Substituting twice is substituting once.
theorem substitute_idempotent (host : String) (k : Kind) :
    substitute host (substitute host k) = substitute host k := by
  -- by the kind's shape
  cases k with
  -- a name: `Self` stays `Self`, another name stays itself
  | named n b => by_cases h : n = host <;> simp [substitute, h]
  -- any other shape is untouched both times
  | _ => rfl

end CollectionPatch
