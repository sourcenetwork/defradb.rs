/-!
# StreamProtocols.Table: an embedder's protocol table serves each tag as registered

The story, in plain words. An application embedding a node hands it a list of
stream protocols when the node starts: each names a tag, the largest request it
accepts and how long its handler may run. The node checks every protocol, builds
a table once, and from then on looks each incoming stream's tag up in it.

What must never happen:
* a protocol the node could not serve as declared gets into the table: a tag
  outside 1 to 64 bytes, a tag under Defra's reserved `/defra-iroh/` prefix, a
  request limit above one peer's share, or a deadline of zero or past the cap
  (`register_all_valid`);
* a protocol is accepted and then never served, because another one with the
  same tag answers its streams instead (`register_serves_each_as_given`);
* a tag under the reserved prefix, which is where every Defra tag lives, finds
  an embedder's protocol (`reserved_tags_find_nothing`);
* a tag nobody registered finds a protocol (`unregistered_tags_find_nothing`).

Rust anchors: `StreamProtocol::validate` and `StreamProtocols::new` /
`StreamProtocols::get` in `crates/p2p/src/iroh/stream_protocols.rs`. A tag is
modeled as its bytes; the per-priority share as one number, since each protocol
names one priority.
-/

namespace StreamProtocols

-- A tag is the bytes that name a protocol in a stream's first frame.
abbrev Tag := List Nat

-- What a protocol declares: its tag, its largest request, its handler deadline.
structure Protocol where
  -- the protocol's name on the wire
  tag : Tag
  -- the largest request accepted, in bytes
  maxMessage : Nat
  -- how long a handler may run, in any unit
  deadline : Nat

-- Defra's reserved prefix, `/defra-iroh/`, as bytes.
def reserved : Tag := "/defra-iroh/".toList.map Char.toNat

-- The longest tag a tag frame carries.
def maxTagLen : Nat := 64

-- Whether the node could serve `p` as declared, given one peer's share and the deadline cap.
def valid (share cap : Nat) (p : Protocol) : Bool :=
  -- the tag is 1 to 64 bytes,
  decide (1 ≤ p.tag.length) && decide (p.tag.length ≤ maxTagLen)
  -- does not start with the reserved prefix,
  && !reserved.isPrefixOf p.tag
  -- accepts no request larger than one peer's share,
  && decide (p.maxMessage ≤ share)
  -- and has a deadline above zero and at most the cap.
  && decide (0 < p.deadline) && decide (p.deadline ≤ cap)

-- Look a tag up: the first protocol in the table with that tag.
def get (table : List Protocol) (t : Tag) : Option Protocol :=
  table.find? (fun r => decide (r.tag = t))

-- Whether some entry of `acc` already has tag `t`.
def taken (acc : List Protocol) (t : Tag) : Bool :=
  acc.any (fun r => decide (r.tag = t))

-- Build the table: each protocol in turn is checked and must not repeat a tag
-- already in the table; one failure refuses the whole list.
def register (share cap : Nat) : List Protocol → List Protocol → Option (List Protocol)
  -- every protocol was taken: the table is done
  | acc, [] => some acc
  -- the next one joins only if it is valid and its tag is new
  | acc, p :: ps =>
    if valid share cap p && !taken acc p.tag then register share cap (acc ++ [p]) ps
    else none

-- Every entry of a table is found by its own tag.
def servesEach (table : List Protocol) : Prop := ∀ r ∈ table, get table r.tag = some r

-- Adding a protocol whose tag is new keeps every entry, old and new, found by its own tag.
theorem servesEach_append (acc : List Protocol) (p : Protocol)
    -- the table so far finds each entry by its tag,
    (h : servesEach acc)
    -- and no entry so far has `p`'s tag:
    (fresh : ∀ r ∈ acc, r.tag ≠ p.tag) :
    -- the extended table finds each entry, `p` included, by its tag.
    servesEach (acc ++ [p]) := by
  -- Take any entry of the extended table.
  intro r hr
  -- A lookup in `acc ++ [p]` is the lookup in `acc`, else the one in `[p]`.
  unfold get
  rw [List.find?_append]
  -- The entry is either an old one or `p` itself.
  rcases List.mem_append.mp hr with hold | hnew
  -- An old entry: `acc` already finds it.
  · have hfound := h r hold
    unfold get at hfound
    rw [hfound]
    rfl
  -- `p` itself: nothing in `acc` has its tag, so the lookup reaches `[p]`.
  · rw [List.mem_singleton] at hnew
    subst hnew
    -- `find?` in `acc` is none, since no element of `acc` has the tag.
    have hnone : acc.find? (fun x => decide (x.tag = r.tag)) = none := by
      rw [List.find?_eq_none]
      intro x hx
      simpa using fresh x hx
    rw [hnone]
    simp

-- What a successful registration from `acc` promises about its table.
theorem register_spec (share cap : Nat) :
    ∀ (ps acc table : List Protocol),
      -- registration from a table that finds each entry by its tag and holds only valid ones
      servesEach acc → (∀ r ∈ acc, valid share cap r = true) →
      -- and that succeeds
      register share cap acc ps = some table →
      -- yields a table that still finds each entry, holds only valid ones, kept
      -- every entry it started from and every protocol given, and holds nothing else.
      servesEach table ∧ (∀ r ∈ table, valid share cap r = true) ∧
        (∀ r ∈ acc, r ∈ table) ∧ (∀ p ∈ ps, p ∈ table) ∧ (∀ r ∈ table, r ∈ acc ∨ r ∈ ps) := by
  -- By induction on the protocols still to register.
  intro ps
  induction ps with
  -- None left: the table is `acc`, which already has every property.
  | nil =>
    intro acc table hs hv hreg
    simp only [register, Option.some.injEq] at hreg
    subst hreg
    exact ⟨hs, hv, fun _ h => h, fun _ h => (by cases h), fun _ h => Or.inl h⟩
  -- One more, `p`: it either joins, or the whole registration fails.
  | cons p ps ih =>
    intro acc table hs hv hreg
    simp only [register] at hreg
    split at hreg
    -- It joined: it is valid and its tag is new.
    · rename_i hok
      simp only [Bool.and_eq_true, Bool.not_eq_true'] at hok
      obtain ⟨hvalid, hnew⟩ := hok
      -- No entry so far has `p`'s tag.
      have hfresh : ∀ r ∈ acc, r.tag ≠ p.tag := by
        intro r hr htag
        have : taken acc p.tag = true := List.any_eq_true.mpr ⟨r, hr, by simp [htag]⟩
        rw [this] at hnew
        cases hnew
      -- The extended table finds each entry by its tag,
      have hs' := servesEach_append acc p hs hfresh
      -- and holds only valid entries.
      have hv' : ∀ r ∈ acc ++ [p], valid share cap r = true := by
        intro r hr
        rcases List.mem_append.mp hr with h1 | h2
        · exact hv r h1
        · rw [List.mem_singleton] at h2
          subst h2
          exact hvalid
      -- The rest of the registration keeps both.
      obtain ⟨hs'', hv'', hkeep, hkept, honly⟩ := ih (acc ++ [p]) table hs' hv' hreg
      refine ⟨hs'', hv'', ?_, ?_, ?_⟩
      -- An entry of `acc` is in `acc ++ [p]`, which the rest kept.
      · exact fun r hr => hkeep r (List.mem_append_left _ hr)
      -- `p` is in `acc ++ [p]`; a later protocol was kept by the rest.
      · intro q hq
        rcases List.mem_cons.mp hq with heq | hrest
        · subst heq
          exact hkeep q (List.mem_append_right _ (List.mem_singleton.mpr rfl))
        · exact hkept q hrest
      -- Every entry came from `acc`, from `p`, or from the rest.
      · intro r hr
        rcases honly r hr with hacc | hrest
        · rcases List.mem_append.mp hacc with h1 | h2
          · exact Or.inl h1
          · rw [List.mem_singleton] at h2
            subst h2
            exact Or.inr (List.mem_cons_self _ _)
        · exact Or.inr (List.mem_cons_of_mem _ hrest)
    -- It did not join: the registration returned nothing, not `table`.
    · cases hreg

-- Registration starting from an empty table: what `StreamProtocols::new` runs.
theorem register_from_empty (share cap : Nat) (ps table : List Protocol)
    -- registration of `ps` succeeded with `table`
    (h : register share cap [] ps = some table) :
    -- so every promise of `register_spec` holds from an empty start.
    servesEach table ∧ (∀ r ∈ table, valid share cap r = true) ∧
      (∀ p ∈ ps, p ∈ table) ∧ (∀ r ∈ table, r ∈ ps) := by
  -- An empty table has nothing to find and nothing invalid.
  obtain ⟨hs, hv, _, hkept, honly⟩ :=
    register_spec share cap ps [] table (fun _ h => by cases h) (fun _ h => by cases h) h
  -- Nothing came from the empty start, so every entry was given.
  refine ⟨hs, hv, hkept, fun r hr => ?_⟩
  rcases honly r hr with hnil | hps
  · cases hnil
  · exact hps

-- Promise: every protocol in a table the node built is one it can serve as declared.
theorem register_all_valid (share cap : Nat) (ps table : List Protocol)
    -- registration of `ps` from an empty table succeeded with `table`
    (h : register share cap [] ps = some table) :
    -- so every entry is valid.
    ∀ r ∈ table, valid share cap r = true :=
  (register_from_empty share cap ps table h).2.1

-- Promise: every protocol given is served under its tag by exactly that protocol,
-- so none is accepted and then shadowed by another with the same tag.
theorem register_serves_each_as_given (share cap : Nat) (ps table : List Protocol)
    -- registration of `ps` from an empty table succeeded with `table`
    (h : register share cap [] ps = some table) :
    -- so looking up any given protocol's tag finds that protocol.
    ∀ p ∈ ps, get table p.tag = some p := by
  -- A given protocol is in the table, and the table finds each entry by its tag.
  obtain ⟨hs, _, hkept, _⟩ := register_from_empty share cap ps table h
  exact fun p hp => hs p (hkept p hp)

-- Promise: a tag under the reserved prefix finds nothing, so no embedder
-- protocol answers a Defra tag, today's or a later one.
theorem reserved_tags_find_nothing (share cap : Nat) (ps table : List Protocol) (t : Tag)
    -- registration succeeded with `table`
    (h : register share cap [] ps = some table)
    -- and `t` starts with the reserved prefix:
    (ht : reserved.isPrefixOf t = true) :
    -- the lookup finds nothing.
    get table t = none := by
  -- Every entry is valid, so none has a reserved tag.
  have hv := register_all_valid share cap ps table h
  -- `find?` is none when no element matches.
  unfold get
  rw [List.find?_eq_none]
  intro r hr heq
  -- An entry with tag `t` would have a reserved tag, which `valid` refuses.
  have hr' := hv r hr
  simp only [decide_eq_true_eq] at heq
  subst heq
  simp [valid, ht] at hr'

-- Promise: a tag no given protocol carries finds nothing, so its stream is reset as unknown.
theorem unregistered_tags_find_nothing (share cap : Nat) (ps table : List Protocol) (t : Tag)
    -- registration succeeded with `table`
    (h : register share cap [] ps = some table)
    -- and no given protocol has tag `t`:
    (ht : ∀ p ∈ ps, p.tag ≠ t) :
    -- the lookup finds nothing.
    get table t = none := by
  -- Every entry of the table was given.
  have honly := (register_from_empty share cap ps table h).2.2.2
  -- `find?` is none when no element matches.
  unfold get
  rw [List.find?_eq_none]
  intro r hr heq
  -- A matching entry would be a given protocol with tag `t`.
  simp only [decide_eq_true_eq] at heq
  exact ht r (honly r hr) heq

-- Promise: registering one protocol twice is refused.
theorem a_repeated_tag_is_refused (share cap : Nat) (p : Protocol) :
    -- the same protocol twice never builds a table.
    register share cap [] [p, p] = none := by
  -- The first step either fails, or takes `p`; then the second finds its tag taken.
  unfold register
  split
  · simp [register, taken]
  · rfl

end StreamProtocols
