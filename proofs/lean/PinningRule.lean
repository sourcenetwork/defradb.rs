/-!
# PinningRule: when a worker pool and its network runtime pin their threads

The story, in plain words. A host runs one or more processes that use
DefraDB. Each starts a worker pool and a network runtime, and may pin their
threads (the workers, the compaction and blocking threads, the network
shards) to CPUs. A pinned thread never moves: its caches stay warm, and no
other thread of the process lands on its core. Pinning is the default.

Each process keeps its own ledger of the CPUs its pools hold. No process can
read another one's, so two processes on one host plan the same CPUs: the
first worker on CPU 0, compaction on the next cores, and so on. With a dozen
such processes, CPU 0 is full while most other CPUs sit idle.

The operator picks a placement:
- `spread` (the Linux default), `pack` or CPU lists: always pin;
- `unpinned`: never pin, the setting for processes that share their CPUs;
- `auto`, an opt-in: pin only when the set of CPUs the process may run on
  (its mask) leaves out some CPU the host has online, as a container cpuset,
  `taskset` or systemd's `CPUAffinity` does.

A narrowed mask is a hint, not a promise. Several processes may be given one
narrowed set (one `--cpuset-cpus` for many containers, one systemd slice),
and under `auto` they pile up inside it, as forced pinning piles up on a
whole host.

The network runtime asks the pool's question with the pool's placement, so
the two never disagree: under `spread` both pin, as before `auto` existed;
under `unpinned` neither does.

Who acts: each process, which decides once, at start, whether to pin its
pool's threads and its network runtime's, and plans which CPUs each thread
would take (always CPUs of its mask).

What can happen: the mask covers every online CPU or leaves one out; the
operator keeps the default, forces pinning, turns it off, or opts into
`auto`.

What must never happen:
- the default does not pin (`the_default_pins`), a forced placement does not
  pin (`forced_always_pins`), or `unpinned` pins (`unpinned_never_pins`);
- processes running `unpinned` pin a CPU in common, even on one narrowed set
  (`unpinned_shares_nothing`);
- under `auto`, a process whose mask covers every online CPU pins a thread
  (`all_cpu_mask_pins_nothing`), so such processes pin a CPU in common
  (`shared_host_pins_nothing_in_common`);
- under `auto`, a process given CPUs of its own stops pinning
  (`narrowed_mask_pins`), or any process pins a thread outside its mask
  (`pins_within_mask`), so processes given disjoint CPU sets meet
  (`disjoint_masks_stay_apart`);
- the network runtime pins when the pool does not, or the other way round
  (`edge_pins_iff_pool_pins`).

What is not promised, shown by example:
- forced pinning on a shared host piles up
  (`forced_on_a_shared_host_piles_up`);
- `auto` on one narrowed mask that two processes share piles up too
  (`auto_on_equal_narrowed_masks_piles_up`).

`narrowed_iff` says the mask test is exactly "some online CPU is left out".
Nothing here is concurrent (each process decides once, on its own), so there
is no TLA+ model; the CPU ledger itself is modeled in `CpuLedger.tla`.

Rust anchors: `pins` and `plan` in
`crates/foundation/workers/src/placement/mod.rs`, `reserve` in
`placement/reserve.rs`, `EdgeCores::reserve_on` in `placement/edge.rs`,
`Placement::default` in `config/mod.rs`, and `narrowed` and `detect` in
`topology/linux.rs`. The tests `the_rule_matches_the_lean_model` in
`placement/pinning_tests.rs` and `the_edge_pins_as_a_pool_placed_the_same_would`
in `tests/edge_cores.rs` check the Rust pool and network runtime against the
table of `pins` here.
-/

namespace PinningRule -- Keep these definitions together.

-- A CPU, named by its id.
abbrev Cpu := Nat

-- Where the operator asks a pool's threads to run (`Placement` in Rust).
inductive Placement where
  | auto     -- the opt-in: pin only within a mask narrowed for the process
  | unpinned -- never pin
  | spread   -- pin, alternating NUMA nodes and caches; the Linux default
  | pack     -- pin, filling one node before the next
  | custom   -- pin to the CPU lists given
  deriving DecidableEq, Repr

-- The placement a pool gets when nobody picks one on Linux
-- (`Placement::default` in config/mod.rs).
def linuxDefault : Placement := .spread -- spread, which pins

-- Whether the mask leaves out a CPU the host has online (`narrowed` in
-- topology/linux.rs): some online CPU is not in the mask.
def narrowed (online mask : List Cpu) : Bool :=
  online.any (fun c => !mask.contains c) -- an online CPU missing from the mask

-- Whether a pool pins, given its placement and whether its mask was
-- narrowed (`pins` in placement/mod.rs).
def pins : Placement → Bool → Bool
  | .auto, narrow => narrow -- the opt-in follows the mask
  | .unpinned, _ => false   -- never
  | .spread, _ => true      -- always
  | .pack, _ => true        -- always
  | .custom, _ => true      -- always

-- One process, as far as pinning goes.
structure Process where
  online : List Cpu          -- the host's online CPUs
  mask : List Cpu            -- the CPUs this process may run on
  placement : Placement      -- what its operator asked for
  seats : List (List Cpu)    -- the CPUs its plan gives each thread it places

-- The CPUs a thread on `seat` is pinned to: the seat's CPUs when the pool
-- pins, none when it runs unpinned (an empty set asks for no pin).
def pinnedTo (p : Process) (seat : List Cpu) : List Cpu :=
  if pins p.placement (narrowed p.online p.mask) then seat else [] -- pin, or nothing

-- Whether the network runtime, sized from the machine, pins its threads to
-- cores of its own (`EdgeCores::reserve_on` in placement/edge.rs). It is
-- given the pool's placement (`EdgeConfig::placement`; `defradb start` passes
-- `--worker-placement`) and asks `pins` the pool's question.
def edgePins (p : Process) : Bool :=
  pins p.placement (narrowed p.online p.mask) -- the pool's question, the pool's placement

-- The mask leaves out an online CPU exactly when the test says it does.
theorem narrowed_iff (online mask : List Cpu) :
    narrowed online mask = true ↔ ∃ c ∈ online, c ∉ mask := by
  simp [narrowed, List.any_eq_true] -- `any` is "some element passes"

-- The default pins, whatever the mask.
theorem the_default_pins (narrow : Bool) : pins linuxDefault narrow = true := by
  rfl -- the default is spread, and spread always pins

-- A forced placement pins, whatever the mask.
theorem forced_always_pins (placement : Placement) (narrow : Bool)
    (h : placement = .spread ∨ placement = .pack ∨ placement = .custom) :
    pins placement narrow = true := by
  rcases h with h | h | h <;> simp [h, pins] -- each forced placement pins

-- `unpinned` never pins, whatever the mask.
theorem unpinned_never_pins (narrow : Bool) : pins .unpinned narrow = false := by
  rfl -- by its definition

-- A process running `unpinned` pins no thread to anything, whatever its mask.
theorem unpinned_pins_nothing (p : Process) (h : p.placement = .unpinned)
    (seat : List Cpu) : pinnedTo p seat = [] := by
  simp [pinnedTo, h, pins] -- `unpinned` never pins: no pin

-- Processes running `unpinned` pin no CPU in common, even when they were
-- given one narrowed set: the setting for processes that share their CPUs.
theorem unpinned_shares_nothing (p q : Process)
    (hp : p.placement = .unpinned) (hq : q.placement = .unpinned)
    (s t : List Cpu) (c : Cpu) : c ∉ pinnedTo p s ∧ c ∉ pinnedTo q t := by
  rw [unpinned_pins_nothing p hp s,                -- p pins nothing,
    unpinned_pins_nothing q hq t]                  -- and neither does q,
  exact ⟨List.not_mem_nil c, List.not_mem_nil c⟩   -- so c is among neither's pins

-- Under `auto`, a process whose mask covers every online CPU pins no thread
-- to anything.
theorem all_cpu_mask_pins_nothing (p : Process)
    (hauto : p.placement = .auto) (hall : ∀ c ∈ p.online, c ∈ p.mask)
    (seat : List Cpu) : pinnedTo p seat = [] := by
  have hn : narrowed p.online p.mask = false := by -- the mask is not narrowed:
    cases h : narrowed p.online p.mask with          --   suppose it were,
    | false => rfl                                   --   (it is not: done)
    | true =>                                        --   then
      obtain ⟨c, hc, hm⟩ := (narrowed_iff _ _).mp h  --   some online CPU is missing,
      exact absurd (hall c hc) hm                    --   yet every one is in the mask
  simp [pinnedTo, hauto, hn, pins] -- `auto` follows the mask: no pin

-- Two processes on one host, each under `auto` with a mask of every online
-- CPU, pin no CPU in common: neither pins any thread to any CPU.
theorem shared_host_pins_nothing_in_common (p q : Process)
    (hp : p.placement = .auto) (hq : q.placement = .auto)
    (hpall : ∀ c ∈ p.online, c ∈ p.mask) (hqall : ∀ c ∈ q.online, c ∈ q.mask)
    (s t : List Cpu) (c : Cpu) : c ∉ pinnedTo p s ∧ c ∉ pinnedTo q t := by
  rw [all_cpu_mask_pins_nothing p hp hpall s,      -- p pins nothing,
    all_cpu_mask_pins_nothing q hq hqall t]        -- and neither does q,
  exact ⟨List.not_mem_nil c, List.not_mem_nil c⟩   -- so c is among neither's pins

-- Forced pinning on a shared host promises no such thing: two processes
-- whose plans are alike pin their first thread to the same CPU.
theorem forced_on_a_shared_host_piles_up :
    ∃ p q : Process, p.mask = p.online ∧ q.mask = q.online ∧
      0 ∈ pinnedTo p [0] ∧ 0 ∈ pinnedTo q [0] :=
  ⟨⟨[0, 1], [0, 1], .spread, [[0]]⟩, ⟨[0, 1], [0, 1], .spread, [[0]]⟩, -- two alike processes
    rfl, rfl, by decide, by decide⟩                                    -- both pin CPU 0

-- Nor does `auto` on one narrowed mask that two processes share: the mask
-- leaves an online CPU out, so both pin, and two processes whose plans are
-- alike pin their first thread to the same CPU. A narrowed mask does not
-- make its CPUs the process's alone.
theorem auto_on_equal_narrowed_masks_piles_up :
    ∃ p q : Process, p.placement = .auto ∧ q.placement = .auto ∧
      p.mask = q.mask ∧ narrowed p.online p.mask = true ∧
      0 ∈ pinnedTo p [0] ∧ 0 ∈ pinnedTo q [0] :=
  ⟨⟨[0, 1, 2], [0, 1], .auto, [[0]]⟩, ⟨[0, 1, 2], [0, 1], .auto, [[0]]⟩, -- one set of 2 of 3 CPUs
    rfl, rfl, rfl, by decide, by decide, by decide⟩                     -- narrowed; both pin CPU 0

-- Under `auto`, a process given CPUs of its own pins every thread to its
-- seat.
theorem narrowed_mask_pins (p : Process)
    (hauto : p.placement = .auto) (hgiven : ∃ c ∈ p.online, c ∉ p.mask)
    (seat : List Cpu) : pinnedTo p seat = seat := by
  have hn : narrowed p.online p.mask = true := (narrowed_iff _ _).mpr hgiven -- it is narrowed
  simp [pinnedTo, hauto, hn, pins] -- `auto` follows the mask: pin

-- When the plan seats every thread on CPUs of the mask (as `plan` does: it
-- only uses the topology's allowed CPUs), no thread is pinned outside it.
theorem pins_within_mask (p : Process)
    (hplan : ∀ s ∈ p.seats, ∀ c ∈ s, c ∈ p.mask)
    (s : List Cpu) (hs : s ∈ p.seats) (c : Cpu) (hc : c ∈ pinnedTo p s) :
    c ∈ p.mask := by
  unfold pinnedTo at hc                   -- the pin is the seat, or nothing:
  split at hc                             -- look at both cases
  · exact hplan s hs c hc                 -- the seat: its CPUs are in the mask
  · exact absurd hc (List.not_mem_nil c)  -- nothing: c is not there

-- Two processes given disjoint CPU sets never pin threads to one CPU.
theorem disjoint_masks_stay_apart (p q : Process)
    (hp : ∀ s ∈ p.seats, ∀ c ∈ s, c ∈ p.mask)
    (hq : ∀ s ∈ q.seats, ∀ c ∈ s, c ∈ q.mask)
    (hdisjoint : ∀ c ∈ p.mask, c ∉ q.mask)
    (s : List Cpu) (hs : s ∈ p.seats) (t : List Cpu) (ht : t ∈ q.seats)
    (c : Cpu) (hcp : c ∈ pinnedTo p s) : c ∉ pinnedTo q t := by
  intro hcq                                        -- suppose q pins c too:
  have inp := pins_within_mask p hp s hs c hcp     -- c is in p's mask,
  have inq := pins_within_mask q hq t ht c hcq     -- and in q's mask,
  exact hdisjoint c inp inq                        -- but the masks share no CPU

-- The network runtime pins exactly when the pool pins a thread to its seat:
-- under `spread` both pin, as before `auto` existed; under `unpinned`
-- neither does; under `auto` both follow the mask.
theorem edge_pins_iff_pool_pins (p : Process) (seat : List Cpu) (hseat : seat ≠ []) :
    edgePins p = true ↔ pinnedTo p seat = seat := by
  unfold edgePins pinnedTo                                -- both ask `pins` one question:
  cases pins p.placement (narrowed p.online p.mask) <;>   -- look at both answers;
    simp [Ne.symm hseat]                                  -- no: neither pins; yes: both do

end PinningRule
