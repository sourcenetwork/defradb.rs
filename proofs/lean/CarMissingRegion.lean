/-!
# CarMissingRegion: a fetch naming its haves is served exactly what it lacks

The story, in plain words. A receiver asks a provider for the DAG below some
wanted blocks, and names blocks it holds whose whole DAG it holds too (its
haves). The provider serves every block it asked for by name, plus every
block below them that no have reaches. The provider walks blocks highest
priority first, and every link goes to a lower priority, so all the blocks
that can tell it "this one is held" are walked before the block itself.

The libp2p request carries the root CID and then the have CIDs, each CID
self-delimiting, so an older provider that reads one CID reads the root.

What is promised:
- a block asked for by name is always served (`wanted_served`);
- a served block a have reaches was asked for by name: no other held block
  is sent again (`served_held_is_asked`);
- every block below the wanted ones is served or reached by a have, so a
  receiver that holds its haves' DAGs holds the whole wanted DAG after the
  response (`missing_is_served`, `receiver_complete`);
- a block's priority is below that of every block that reaches it along
  heads, so the walk's order takes them first (`reach_lowers_priority`);
- when the walk does not run (no have is held here, or a wanted block is not
  a composite), the provider serves the whole closure below the wanted
  blocks, and the receiver again ends up holding all of it
  (`fallback_complete`);
- the libp2p request round-trips (`decode_encode`), and its first CID is the
  root (`first_is_root`).

A DAG is a function naming each block's links; blocks are numbers. A CID is a
list of bytes encoded behind its length, which is what makes it
self-delimiting.

Assumed, not modelled: the Rust walk colours "held" along composite heads and
serves a composite's field blocks with it, without following field blocks'
own links. That matches this whole-DAG reach because each field block is
linked by the composite that made it; `ExactDagFetch.tla` checks the walk
step by step on a DAG with branches and field chains, and
`a_concurrent_branch_deeper_than_any_cut_is_served_exactly` and
`a_merge_of_a_held_and_a_missing_branch_serves_the_missing_side`
(`crates/p2p/tests/unit/car_region.rs`) check it on real blocks. The caps
(`CAR_MAX_BLOCKS`, `CAR_MAX_BYTES`, `MAX_HAVE_CIDS`) and a block the provider
cannot read (`read_wanted` returning nothing) can serve less; the receiver
then asks again for what is still missing. The cap on held reads can only
colour fewer blocks held, so it serves more, never less.

The walk orders only heads: a composite's heads are composites one priority
below it. A field block carries its composite's priority and is served with
that composite, never popped on its own, so the priority theorems are about
the heads DAG, not every link.

The walk runs only when the provider holds a have and every wanted block is
a composite. Otherwise (`collect_dag_blocks_within`) it serves the closure of
the wanted blocks, cut at the caller's depth on iroh and whole on libp2p, and
`FallbackServed` is that closure. A branchable collection's blocks take this
path: their heads are collection blocks, and their document composite is a
plain link the composite walk would not follow
(`a_collection_block_head_outside_the_haves_is_served`). A depth cut serves
less, and the receiver asks again from its missing frontier.

Rust anchors: `collect_missing_region` (`crates/p2p/src/sync/car/region.rs`),
`haves_from_heads` (`crates/p2p/src/sync/coordinator/haves.rs`),
`encode_car_request` and `decode_car_request`
(`crates/p2p/src/two_stream/handler/car.rs`).
-/
namespace CarMissingRegion

-- Each block names the blocks it links to.
abbrev Dag := Nat → List Nat

-- `b` is `a`, or below `a` along links.
inductive Reaches (g : Dag) : Nat → Nat → Prop
  -- Every block reaches itself.
  | refl (a : Nat) : Reaches g a a
  -- A block reaches whatever its links reach.
  | step (a c b : Nat) : c ∈ g a → Reaches g c b → Reaches g a b

-- Some block of `S` reaches `b`.
def ReachedFrom (g : Dag) (S : List Nat) (b : Nat) : Prop :=
  ∃ s, s ∈ S ∧ Reaches g s b

-- What the provider serves: a block asked for by name, or a block below the
-- wanted ones that no have reaches.
def Served (g : Dag) (wants haves : List Nat) (b : Nat) : Prop :=
  b ∈ wants ∨ (ReachedFrom g wants b ∧ ¬ ReachedFrom g haves b)

-- A block asked for by name is always served.
theorem wanted_served (g : Dag) (wants haves : List Nat) (b : Nat)
    (h : b ∈ wants) : Served g wants haves b := by
  -- It is the first case of `Served`.
  exact Or.inl h

-- A served block that a have reaches was asked for by name.
theorem served_held_is_asked (g : Dag) (wants haves : List Nat) (b : Nat)
    (hs : Served g wants haves b) (hh : ReachedFrom g haves b) : b ∈ wants := by
  -- The second case of `Served` says no have reaches it, which `hh` refutes.
  cases hs with
  | inl hw => exact hw
  | inr hr => exact absurd hh hr.2

-- Every block below the wanted ones is served, or a have reaches it.
theorem missing_is_served (g : Dag) (wants haves : List Nat) (b : Nat)
    (hw : ReachedFrom g wants b) :
    Served g wants haves b ∨ ReachedFrom g haves b := by
  -- Either a have reaches it, or it is served by the second case.
  by_cases hh : ReachedFrom g haves b
  · exact Or.inr hh
  · exact Or.inl (Or.inr ⟨hw, hh⟩)

-- A receiver holding the DAG of every have holds the whole wanted DAG once
-- it stores what it was served.
theorem receiver_complete (g : Dag) (wants haves : List Nat)
    (holds : Nat → Prop)
    (held : ∀ b, ReachedFrom g haves b → holds b)
    (stored : ∀ b, Served g wants haves b → holds b)
    (b : Nat) (hw : ReachedFrom g wants b) : holds b := by
  -- Each block is served or a have reaches it; both are held.
  cases missing_is_served g wants haves b hw with
  | inl hs => exact stored b hs
  | inr hh => exact held b hh

-- What the fallback serves: every block below the wanted ones, held or not.
def FallbackServed (g : Dag) (wants : List Nat) (b : Nat) : Prop :=
  ReachedFrom g wants b

-- The fallback serves every block a wanted one reaches, so the receiver ends
-- up holding the whole wanted DAG, whatever it held before.
theorem fallback_complete (g : Dag) (wants : List Nat)
    (holds : Nat → Prop)
    (stored : ∀ b, FallbackServed g wants b → holds b)
    (b : Nat) (hw : ReachedFrom g wants b) : holds b := by
  -- The block is in the closure, which is what the fallback serves.
  exact stored b hw

-- The fallback serves at least what the walk serves: a block the walk sends
-- is one the fallback sends too, so falling back loses nothing.
theorem served_within_fallback (g : Dag) (wants haves : List Nat) (b : Nat)
    (hs : Served g wants haves b) : FallbackServed g wants b := by
  -- A wanted block reaches itself; a served block below is reached already.
  cases hs with
  | inl hw => exact ⟨b, hw, Reaches.refl b⟩
  | inr hr => exact hr.1

-- With every heads link going to a lower priority, a block's priority is at
-- most that of every block that reaches it along heads.
theorem reach_priority_le (heads : Dag) (prio : Nat → Nat)
    (lower : ∀ a c, c ∈ heads a → prio c < prio a) (a b : Nat)
    (h : Reaches heads a b) : prio b ≤ prio a := by
  -- Along the path, each link lowers the priority.
  induction h with
  | refl a => exact Nat.le_refl _
  | step a c b hc _ ih =>
    have := lower a c hc
    omega

-- A block strictly below another along heads has a strictly lower priority,
-- so the walk takes every composite that reaches it first.
theorem reach_lowers_priority (heads : Dag) (prio : Nat → Nat)
    (lower : ∀ a c, c ∈ heads a → prio c < prio a) (a c b : Nat)
    (hc : c ∈ heads a) (h : Reaches heads c b) : prio b < prio a := by
  -- One link lowers it, and the rest of the path does not raise it.
  have h1 := lower a c hc
  have h2 := reach_priority_le heads prio lower c b h
  omega

-- A CID's bytes behind their length.
def encodeCid (c : List Nat) : List Nat := c.length :: c

-- Read one length-prefixed CID; `none` when the bytes run out.
def decodeCid : List Nat → Option (List Nat × List Nat)
  -- No bytes: no CID.
  | [] => none
  -- A length, then that many bytes, if they are there.
  | n :: rest => if n ≤ rest.length then some (rest.take n, rest.drop n) else none

-- The request: the root, then each have.
def encodeRequest (root : List Nat) (haves : List (List Nat)) : List Nat :=
  encodeCid root ++ (haves.map encodeCid).flatten

-- Read CIDs until the bytes end, or a CID is torn.
def decodeAll : Nat → List Nat → List (List Nat)
  -- Out of fuel: stop.
  | 0, _ => []
  -- Read one CID and go on with the rest.
  | fuel + 1, bytes =>
    match decodeCid bytes with
    | some (c, rest) => c :: decodeAll fuel rest
    | none => []

-- Reading the encoding of a CID gives the CID and what follows it.
theorem decodeCid_encode (c rest : List Nat) :
    decodeCid (encodeCid c ++ rest) = some (c, rest) := by
  -- The length fits, and taking it splits off exactly the CID.
  simp [encodeCid, decodeCid]

-- An older provider reads one CID and gets the root.
theorem first_is_root (root : List Nat) (haves : List (List Nat)) :
    (decodeCid (encodeRequest root haves)).map Prod.fst = some root := by
  -- The request starts with the root's encoding.
  simp [encodeRequest, decodeCid_encode]

-- Reading the encodings of a list of CIDs gives the list back.
theorem decodeAll_encode (cs : List (List Nat)) (fuel : Nat)
    (h : cs.length ≤ fuel) : decodeAll fuel (cs.map encodeCid).flatten = cs := by
  -- One CID at a time, with fuel to spare.
  induction cs generalizing fuel with
  | nil => cases fuel <;> simp [decodeAll, decodeCid]
  | cons c cs ih =>
    cases fuel with
    | zero => simp at h
    | succ fuel =>
      simp only [List.map_cons, List.flatten_cons, decodeAll, decodeCid_encode]
      rw [ih fuel (by simp at h; omega)]

-- The request round-trips: the root first, then the haves.
theorem decode_encode (root : List Nat) (haves : List (List Nat)) :
    decodeAll (haves.length + 1) (encodeRequest root haves) = root :: haves := by
  -- The root comes off first, then the haves.
  simp only [encodeRequest, decodeAll, decodeCid_encode]
  rw [decodeAll_encode haves haves.length (Nat.le_refl _)]

end CarMissingRegion
