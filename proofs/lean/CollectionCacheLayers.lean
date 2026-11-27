/-!
# CollectionCacheLayers: the two-layer collection cache answers like one map

The story, in plain words. A transaction caches the collections it loads,
by name. It publishes the cache as an immutable snapshot and replaces it on
every load, so a load must not copy everything already cached. The cache
therefore keeps two layers: `folded`, a map that many snapshots share, and
`recent`, a short list of the latest loads that answers ahead of it. A load
copies only `recent`; when `recent` is full, the load folds it, with the new
entry, into a fresh `folded` and empties `recent`.

Who acts: the transaction, adding a collection, removing one, loading the
full set, or looking a name up.

What must never happen: the layers answer a lookup differently from the one
plain map they stand for. Adding must make the name answer the new
collection and leave every other name alone; removing must make the name
answer nothing and leave every other name alone; folding must change no
answer. And `recent`, the part every load copies, must never grow past its
bound.

This file proves each of those, against the plain map the layers stand for
(`view`): `add_view`, `remove_view`, `populate_view`, `fold_view` and
`add_recent_bounded`.

Rust anchors: `crates/database/db/src/collection/cache.rs`
`CollectionCache::get`, `CollectionCache::add`, `CollectionCache::fold`,
`CollectionCache::remove`, `CollectionCache::populate`, `RECENT`.
-/

namespace CollectionCacheLayers

-- One map write: the written name answers the new value, every other name
-- what it answered before.
def update {β : Type} (f : String → β) (key : String) (value : β) : String → β :=
  -- Compare each name asked for with the written one.
  fun other => if other = key then value else f other

-- The answer of the recent list for a name: its first entry with that name.
-- (`CollectionCache::recent_entry`.)
def lookup {C : Type} : List (String × C) → String → Option C
  -- An empty list answers nothing.
  | [], _ => none
  -- The first entry answers if it carries the name, else the rest.
  | (n, c) :: rest, k => if n = k then some c else lookup rest k

-- The two layers.
structure Cache (C : Type) where
  -- The shared map of folded entries.
  folded : String → Option C
  -- The names the folded map answers.
  folded_names : List String
  -- The latest loads, answering ahead of `folded`.
  recent : List (String × C)

-- The names in a layer, in the order it answers them.
def namesOf {C : Type} (l : List (String × C)) : List String :=
  -- Keep each entry's name.
  l.map Prod.fst

-- Add a name to a list if it is not there yet.
def remember (name : String) (names : List String) : List String :=
  -- A name already present is left once; a new name joins the front.
  if names.contains name then names else name :: names

-- Keep one of each name.
def uniqueNames : List String → List String
  -- No names, nothing to keep.
  | [] => []
  -- Keep this name once, then the rest.
  | name :: rest => remember name (uniqueNames rest)

-- Remembering a name keeps a duplicate-free list duplicate-free.
theorem remember_nodup (name : String) (names : List String) (h : names.Nodup) :
    (remember name names).Nodup := by
  -- Split on whether the name is already present.
  unfold remember
  split
  · exact h
  · rename_i absent
    have notMember : name ∉ names := by
      intro member
      exact absent ((List.contains_iff_mem).mpr member)
    exact List.nodup_cons.mpr ⟨notMember, h⟩

-- A list of unique names contains each name once.
theorem uniqueNames_nodup (names : List String) : (uniqueNames names).Nodup := by
  -- Walk the names from the first, remembering each once.
  induction names with
  | nil => simp [uniqueNames]
  | cons name rest ih => exact remember_nodup name (uniqueNames rest) ih

-- Every name the cache answers, as a set (a duplicate-free list).
def names {C : Type} (s : Cache C) : List String :=
  -- Recent names come first; folded names shadowed by recent are skipped.
  uniqueNames
    (namesOf s.recent ++ s.folded_names.filter fun k => (lookup s.recent k).isNone)

-- Whether a name is not `k`.
def keepUnless (k name : String) : Bool :=
  -- Keep every name except `k`.
  !decide (name = k)

-- The number of collections the cache holds: the size of the view's domain.
def len {C : Type} (s : Cache C) : Nat :=
  -- Count each name in the view once.
  (names s).length

-- The plain map the layers stand for: `recent` first, then `folded`.
-- (`CollectionCache::get`.)
def view {C : Type} (s : Cache C) (k : String) : Option C :=
  -- A recent entry answers; otherwise the folded map does.
  match lookup s.recent k with
  | some c => some c
  | none => s.folded k

-- Writes `c` over the recent entry named `k`, keeping every other entry.
def replace {C : Type} (l : List (String × C)) (k : String) (c : C) : List (String × C) :=
  -- Each entry named `k` takes the new collection; the rest stay.
  match l with
  | [] => []
  | e :: rest =>
      (if e.1 = k then (k, c) else e) :: replace rest k c

-- Drops every recent entry named `k`.
def drop {C : Type} : List (String × C) → String → List (String × C)
  -- Nothing to drop from an empty list.
  | [], _ => []
  -- An entry named `k` goes; any other stays.
  | (n, c) :: rest, k => if n = k then drop rest k else (n, c) :: drop rest k

-- Folding: the folded map becomes what the layers answer, and `recent`
-- empties. (`CollectionCache::fold` with no extra entry.)
def fold {C : Type} (s : Cache C) : Cache C :=
  -- The new folded map is the view itself; nothing stays recent.
  { folded := view s, folded_names := names s, recent := [] }

-- Adding collection `c` under name `k` with `bound` the most entries
-- `recent` holds (`RECENT`). (`CollectionCache::add`.)
def add {C : Type} (bound : Nat) (s : Cache C) (k : String) (c : C) : Cache C :=
  -- A name already recent is overwritten in place.
  match lookup s.recent k with
  | some _ => { s with recent := replace s.recent k c }
  | none =>
      -- Room left: the entry joins the recent list at its end.
      if s.recent.length < bound then { s with recent := s.recent ++ [(k, c)] }
      -- Full: fold the recent list with the new entry into a new folded map.
      else { folded := update (view s) k (some c),
             folded_names := remember k (names s), recent := [] }

-- Removing the name `k` from both layers. (`CollectionCache::remove`.)
def remove {C : Type} (s : Cache C) (k : String) : Cache C :=
  -- Dropped from the recent list and cleared in the folded map.
  { folded := update s.folded k none,
    folded_names := s.folded_names.filter (keepUnless k),
    recent := drop s.recent k }

-- Loading the full set: a new folded map, nothing recent.
-- (`CollectionCache::populate`.)
def populate {C : Type} (all_names : List String) (all : String → Option C) : Cache C :=
  -- The full set is the folded map.
  { folded := all, folded_names := all_names, recent := [] }

-- Overwriting a recent value keeps its place and its name.
theorem replace_names {C : Type} (l : List (String × C)) (k : String) (c : C) :
    namesOf (replace l k c) = namesOf l := by
  -- Walk the list; the overwritten entry still carries `k`.
  induction l with
  | nil => rfl
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases h : n = k
      · simpa [replace, namesOf, h] using ih
      · simpa [replace, namesOf, h] using ih

-- Overwriting a recent value keeps the layer's length.
theorem replace_length {C : Type} (l : List (String × C)) (k : String) (c : C) :
    (replace l k c).length = l.length := by
  -- Walk the list; the recursion keeps its length.
  induction l with
  | nil => rfl
  | cons e rest ih => simp [replace, ih]

-- A name absent from the recent layer is absent from its names.
theorem not_mem_of_lookup_none {C : Type} (l : List (String × C)) (k : String)
    (h : lookup l k = none) : k ∉ namesOf l := by
  -- Walk the list until the absent name would have answered.
  induction l with
  | nil => simp [namesOf]
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases hn : n = k
      · simp [lookup, hn] at h
      · have hrest : lookup rest k = none := by simpa [lookup, hn] using h
        have ih' := ih hrest
        have hkn : k ≠ n := fun same => hn same.symm
        intro member
        have member' : k ∈ namesOf rest := by simpa [namesOf, hkn] using member
        exact ih' member'

-- Dropping a name filters exactly that name from the recent layer.
theorem drop_names {C : Type} (l : List (String × C)) (k : String) :
    namesOf (drop l k) = (namesOf l).filter (keepUnless k) := by
  -- Walk the list, dropping every entry named `k`.
  induction l with
  | nil => rfl
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases hn : n = k
      · calc
          namesOf (drop ((n, v) :: rest) k) = namesOf (drop rest k) := by
            simp [drop, namesOf, hn]
          _ = (namesOf rest).filter (keepUnless k) := ih
          _ = (namesOf ((n, v) :: rest)).filter (keepUnless k) := by
            simp [namesOf, keepUnless, hn]
      · calc
          namesOf (drop ((n, v) :: rest) k) = n :: namesOf (drop rest k) := by
            simp [drop, namesOf, hn]
          _ = n :: (namesOf rest).filter (keepUnless k) := by rw [ih]
          _ = (namesOf ((n, v) :: rest)).filter (keepUnless k) := by
            simp [namesOf, keepUnless, hn]

-- Whether the recent layer has each name once.
def recentNamesNodup {C : Type} (s : Cache C) : Prop :=
  -- Names are unique in the recent list.
  (namesOf s.recent).Nodup

-- Adding a collection keeps every recent name unique.
theorem add_recent_names_nodup {C : Type} (bound : Nat) (s : Cache C) (k : String)
    (c : C) (h : recentNamesNodup s) : recentNamesNodup (add bound s k c) := by
  -- Follow the three branches of `add`.
  unfold add
  cases hk : lookup s.recent k with
  | some v =>
      -- Overwriting in place keeps the names exactly.
      simpa [recentNamesNodup, hk, replace_names] using h
  | none =>
      by_cases room : s.recent.length < bound
      · -- Appending keeps uniqueness because the name was absent.
        have absent : k ∉ namesOf s.recent := not_mem_of_lookup_none s.recent k hk
        have appended : (namesOf s.recent ++ [k]).Nodup := by
          change List.Pairwise (fun x y => x ≠ y) (namesOf s.recent ++ [k])
          rw [List.pairwise_append]
          refine ⟨h, by simp, ?_⟩
          intro name member last lastMember
          simp at lastMember
          subst last
          exact fun same => absent (same ▸ member)
        simpa [recentNamesNodup, namesOf, hk, room] using appended
      · -- Folding empties the recent layer.
        simp [recentNamesNodup, namesOf, hk, room]

-- Removing a collection keeps every recent name unique.
theorem remove_recent_names_nodup {C : Type} (s : Cache C) (k : String)
    (h : recentNamesNodup s) : recentNamesNodup (remove s k) := by
  -- Removing filters the names, and a filtered unique list stays unique.
  simpa [recentNamesNodup, remove, drop_names] using
    List.Pairwise.filter (keepUnless k) h

-- The cache never reports a name twice.
theorem names_nodup {C : Type} (s : Cache C) : (names s).Nodup := by
  -- `names` remembers each name the view answers once.
  exact uniqueNames_nodup _

-- The length is the size of the view's domain.
theorem len_eq_names_length {C : Type} (s : Cache C) : len s = (names s).length := by
  -- That is the definition of the model.
  rfl

-- Overwriting a name leaves every other name answering as it did.
theorem lookup_replace_other {C : Type} (l : List (String × C)) (k j : String) (c : C)
    (hj : j ≠ k) : lookup (replace l k c) j = lookup l j := by
  -- Walk the list from its first entry.
  induction l with
  -- The empty list stays empty.
  | nil => rfl
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases hn : n = k
      · -- The overwritten entry carries `k`, not `j`: both lists go on to the rest.
        subst hn
        have hkj : ¬ n = j := fun h => hj h.symm
        simp [replace, lookup, hkj] at ih ⊢
        exact ih
      · -- An untouched entry answers alike in both lists.
        by_cases hnj : n = j
        · subst hnj; simp [replace, lookup, hn]
        · simp [replace, lookup, hn, hnj] at ih ⊢
          exact ih

-- Overwriting a name the list holds makes it answer the new value.
theorem lookup_replace_same {C : Type} (l : List (String × C)) (k : String) (c : C)
    (present : lookup l k ≠ none) : lookup (replace l k c) k = some c := by
  -- Walk the list from its first entry.
  induction l with
  -- The empty list holds no `k`, so this case cannot happen.
  | nil => simp [lookup] at present
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases hn : n = k
      · -- The first entry is `k`'s, now carrying the new value.
        simp [replace, lookup, hn]
      · -- `k` is further on, where the rest answers it.
        have present' : lookup rest k ≠ none := by simpa [lookup, hn] using present
        have step := ih present'
        simp [replace, lookup, hn] at step ⊢
        exact step

-- Appending a name the list does not hold makes it answer the new value and
-- leaves every other name as it was.
theorem lookup_append {C : Type} (l : List (String × C)) (k j : String) (c : C)
    (absent : lookup l k = none) :
    lookup (l ++ [(k, c)]) j = if j = k then some c else lookup l j := by
  -- Walk the list from its first entry.
  induction l with
  -- On an empty list the new entry is the only one.
  | nil =>
      by_cases hj : j = k
      · subst hj; simp [lookup]
      · have : k ≠ j := fun h => hj h.symm
        simp [lookup, this, hj]
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      -- The first entry is not named `k`, as the list holds no `k`.
      have hn : n ≠ k := by
        intro h; subst h; simp [lookup] at absent
      have absent' : lookup rest k = none := by simpa [lookup, hn] using absent
      have step := ih absent'
      by_cases hnj : n = j
      · subst hnj
        have : ¬ n = k := hn
        simp [lookup, this]
      · simp only [List.cons_append, lookup, hnj, if_false]
        exact step

-- Dropping a name makes it answer nothing and leaves every other name as it
-- was.
theorem lookup_drop {C : Type} (l : List (String × C)) (k j : String) :
    lookup (drop l k) j = if j = k then none else lookup l j := by
  -- Walk the list from its first entry.
  induction l with
  -- An empty list answers nothing either way.
  | nil => by_cases hj : j = k <;> simp [drop, lookup, hj]
  | cons e rest ih =>
      obtain ⟨n, v⟩ := e
      by_cases hn : n = k
      · -- The dropped entry: the rest answers.
        subst hn
        by_cases hj : j = n
        · subst hj; simp [drop, ih]
        · have : n ≠ j := fun h => hj h.symm
          simp [drop, lookup, this, ih, hj]
      · -- A kept entry answers for its own name.
        by_cases hnj : n = j
        · subst hnj
          have : ¬ n = k := hn
          simp [drop, lookup, this]
        · simp [drop, lookup, hn, hnj, ih]

-- Adding makes the added name answer the new collection and leaves every
-- other name as it was.
theorem add_view {C : Type} (bound : Nat) (s : Cache C) (k : String) (c : C) :
    view (add bound s k c) = update (view s) k (some c) := by
  -- Compare the two maps name by name.
  funext j
  unfold add
  -- One case per branch of `add`.
  cases hk : lookup s.recent k with
  | some v =>
      -- The name is recent: overwritten in place.
      have present : lookup s.recent k ≠ none := by simp [hk]
      by_cases hj : j = k
      · subst hj; simp [view, lookup_replace_same s.recent j c present, update]
      · simp [view, lookup_replace_other s.recent k j c hj, update, hj]
  | none =>
      by_cases room : s.recent.length < bound
      · -- Room left: appended.
        have hr := lookup_append s.recent k j c hk
        by_cases hj : j = k
        · subst hj; simp [view, room, hr, update]
        · simp [view, room, hr, update, hj]
      · -- Full: folded with the new entry.
        simp [view, room, lookup]

-- Removing makes the removed name answer nothing and leaves every other name
-- as it was.
theorem remove_view {C : Type} (s : Cache C) (k : String) :
    view (remove s k) = update (view s) k none := by
  -- Compare the two maps name by name.
  funext j
  have hr := lookup_drop s.recent k j
  by_cases hj : j = k
  · subst hj; simp [view, remove, hr, update]
  · simp [view, remove, hr, update, hj]

-- Folding changes no answer.
theorem fold_view {C : Type} (s : Cache C) : view (fold s) = view s := by
  -- After a fold nothing is recent, so the folded map, which is the old view,
  -- answers every name.
  funext j
  simp [fold, view, lookup]

-- Loading the full set answers exactly the full set.
theorem populate_view {C : Type} (all_names : List String) (all : String → Option C) :
    view (populate all_names all) = all := by
  -- Nothing is recent, so the full set answers every name.
  funext j
  simp [populate, view, lookup]

-- The recent list, the part every load copies, never grows past its bound.
theorem add_recent_bounded {C : Type} (bound : Nat) (s : Cache C) (k : String) (c : C)
    (h : s.recent.length ≤ bound) :
    (add bound s k c).recent.length ≤ bound := by
  unfold add
  -- One case per branch of `add`.
  cases hk : lookup s.recent k with
  -- Overwriting in place keeps the length.
  | some v => simpa [replace_length] using h
  | none =>
      by_cases room : s.recent.length < bound
      -- Appending to a list with room makes it at most the bound.
      · simp [room]; omega
      -- Folding empties it.
      · simp [room]

end CollectionCacheLayers
