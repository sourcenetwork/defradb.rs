/-!
# A lens's ID records every module and every argument

The story, in plain words. A lens is a list of modules, run in order; each
module is some WASM code, a flag saying whether its inverse runs, and named
arguments. The node gives a lens an ID by hashing the blocks it writes for it:
one config block that links a block per module, as Go's lens store does. Two
lenses with the same ID are treated as the same lens, so the second one added
is not stored again.

The old ID hashed only the first module, and kept an argument only when it was
a string (any other value became ""). So two lenses that differ only in a
later module, or only in a number, got the same ID, and the second was lost
(issue #2378). This file proves the blocks the node writes now tell any two
different lenses apart, so with a collision-free hash their IDs differ, and
shows the old blocks really did collide. An empty lens also has blocks (a
config block with no links), where the old code had none.

Rust owner: crates/defra-core/src/lens_block.rs `build_lens_ipld_blocks`,
`build_module_blocks`; crates/db/src/definition/migration/committed_lenses.rs
`PreparedLens::new`, `module_arguments`; crates/defra-core/src/lens_argument.rs
`encode_lens_argument` (each value's JSON, keeping every integer digit, as
Go's `UseNumber` decode does; a non-canonical spelling such as `30.00` or
`1e21` is respelled by serde_json, where Go keeps it). The Rust checks are
crates/defra-core/tests/lens_block_id.rs, crates/defra-core/tests/lens_argument.rs
and the testo scenarios
collection_version::patch_migration_numeric_fidelity and
collection_version::patch_migration_presence.
-/

namespace LensId

-- An argument's value: a string, a number or a flag, as JSON carries them.
inductive Arg where
  -- A text value, such as a field name.
  | str (s : String)
  -- A whole number, kept exactly as written.
  | num (n : Nat)
  -- A true-or-false value.
  | flag (b : Bool)
  -- Lean can check whether two arguments are equal.
  deriving DecidableEq

-- A module: its WASM bytes, whether its inverse runs, and its arguments.
structure Module where
  -- The module's code, as a list of bytes.
  wasm : List Nat
  -- Whether the module runs backwards (its inverse).
  inverse : Bool
  -- Its named arguments: each a name and a value.
  args : List (String × Arg)
  -- Lean can check whether two modules are equal.
  deriving DecidableEq

-- The module block the node writes: every part of the module. Each argument
-- value goes in JSON-encoded, which keeps the whole value; we model that
-- encoding as keeping the value itself.
structure ModuleBlock where
  -- The block keeps the module's code.
  wasm : List Nat
  -- It keeps the inverse flag.
  inverse : Bool
  -- It keeps every argument, its name and its whole value.
  args : List (String × Arg)
  -- Lean can check whether two blocks are equal.
  deriving DecidableEq

-- Build a module's block: copy each of its three parts in.
def moduleBlock (m : Module) : ModuleBlock := ⟨m.wasm, m.inverse, m.args⟩

-- The config block: one module block per module, in order (`map` builds the
-- block of each module and keeps the list's order).
def configBlock (lens : List Module) : List ModuleBlock := lens.map moduleBlock

-- Two modules with the same block are the same module.
theorem moduleBlock_injective (a b : Module) (h : moduleBlock a = moduleBlock b) :
    a = b := by
  -- Take both modules apart and compare their parts.
  cases a; cases b
  -- Equal blocks means equal code, equal flag and equal arguments.
  simp only [moduleBlock, ModuleBlock.mk.injEq] at h
  -- Name those three equalities.
  obtain ⟨h1, h2, h3⟩ := h
  -- Put one module's parts in place of the other's: the two are now the same.
  subst h1; subst h2; subst h3; rfl

-- Two lenses with the same config block are the same lens: the blocks lose
-- no module, no order and no argument.
theorem configBlock_injective :
    ∀ (a b : List Module), configBlock a = configBlock b → a = b
  -- Two empty lenses are equal.
  | [], [], _ => rfl
  -- An empty lens and a non-empty one have different blocks.
  | [], _ :: _, h => by simp [configBlock] at h
  | _ :: _, [], h => by simp [configBlock] at h
  -- Compare the first modules, then the rest.
  | x :: xs, y :: ys, h => by
      -- Equal lists of blocks means equal first blocks and equal rests.
      simp only [configBlock, List.map_cons, List.cons.injEq] at h
      -- Name those two equalities.
      obtain ⟨hx, hxs⟩ := h
      -- The first modules are equal (the theorem above), and so are the rests
      -- (this theorem again, on the shorter lists).
      rw [moduleBlock_injective x y hx, configBlock_injective xs ys hxs]

-- An empty lens has a config block (with no module links).
theorem empty_lens_has_block : configBlock [] = [] := rfl

-- The old blocks: only the first module, and every non-string argument as "".
def asStr : Arg → String
  -- A string was kept as it was.
  | .str s => s
  -- Anything else became the empty string.
  | _ => ""

-- What the old code hashed; `none` (no first module) was an error.
def oldBlock (lens : List Module) : Option (List Nat × Bool × List (String × String)) :=
  -- Take the first module, if there is one, with its code, flag and arguments as strings.
  lens.head?.map fun m => (m.wasm, m.inverse, m.args.map fun (k, v) => (k, asStr v))

-- Two lenses that differ only in a large number.
def bigger : Module := ⟨[0], false, [("value", .num 9007199254740993)]⟩
def smaller : Module := ⟨[0], false, [("value", .num 9007199254740992)]⟩

-- The old blocks could not tell them apart.
theorem old_numbers_collide : oldBlock [bigger] = oldBlock [smaller] := rfl

-- The new blocks can.
theorem new_numbers_differ : configBlock [bigger] ≠ configBlock [smaller] := by
  -- Lean compares the two blocks itself and finds the numbers differ.
  decide

-- The old blocks ignored every module after the first.
theorem old_ignores_later_modules (m1 m2 : Module) :
    oldBlock [m1, m2] = oldBlock [m1] := rfl

-- The new blocks keep them.
theorem new_keeps_later_modules (m1 m2 : Module) :
    configBlock [m1, m2] ≠ configBlock [m1] := by
  -- One list holds two blocks and the other holds one, so they differ.
  simp [configBlock]

-- The old code had no block at all for an empty lens.
theorem old_empty_lens_has_none : oldBlock [] = none := rfl

end LensId
