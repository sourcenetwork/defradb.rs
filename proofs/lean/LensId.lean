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
`PreparedLens::new`, `module_arguments`. The Rust checks are
crates/defra-core/tests/lens_block_id.rs and the testo scenarios
collection_version::patch_migration_numeric_fidelity and
collection_version::patch_migration_presence.
-/

namespace LensId

-- An argument's value: a string, a number or a flag, as JSON carries them.
inductive Arg where
  | str (s : String)
  | num (n : Nat)
  | flag (b : Bool)
  deriving DecidableEq

-- A module: its WASM bytes, whether its inverse runs, and its arguments.
structure Module where
  wasm : List Nat
  inverse : Bool
  args : List (String × Arg)
  deriving DecidableEq

-- The module block the node writes: every part of the module. Each argument
-- value goes in JSON-encoded, which keeps the whole value; we model that
-- encoding as keeping the value itself.
structure ModuleBlock where
  wasm : List Nat
  inverse : Bool
  args : List (String × Arg)
  deriving DecidableEq

-- Build a module's block.
def moduleBlock (m : Module) : ModuleBlock := ⟨m.wasm, m.inverse, m.args⟩

-- The config block: one module block per module, in order.
def configBlock (lens : List Module) : List ModuleBlock := lens.map moduleBlock

-- Two modules with the same block are the same module.
theorem moduleBlock_injective (a b : Module) (h : moduleBlock a = moduleBlock b) :
    a = b := by
  -- Take both modules apart and compare their parts.
  cases a; cases b
  simp only [moduleBlock, ModuleBlock.mk.injEq] at h
  obtain ⟨h1, h2, h3⟩ := h
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
      simp only [configBlock, List.map_cons, List.cons.injEq] at h
      obtain ⟨hx, hxs⟩ := h
      rw [moduleBlock_injective x y hx, configBlock_injective xs ys hxs]

-- An empty lens has a config block (with no module links).
theorem empty_lens_has_block : configBlock [] = [] := rfl

-- The old blocks: only the first module, and every non-string argument as "".
def asStr : Arg → String
  | .str s => s
  | _ => ""

-- What the old code hashed; `none` (no first module) was an error.
def oldBlock (lens : List Module) : Option (List Nat × Bool × List (String × String)) :=
  lens.head?.map fun m => (m.wasm, m.inverse, m.args.map fun (k, v) => (k, asStr v))

-- Two lenses that differ only in a large number.
def bigger : Module := ⟨[0], false, [("value", .num 9007199254740993)]⟩
def smaller : Module := ⟨[0], false, [("value", .num 9007199254740992)]⟩

-- The old blocks could not tell them apart.
theorem old_numbers_collide : oldBlock [bigger] = oldBlock [smaller] := rfl

-- The new blocks can.
theorem new_numbers_differ : configBlock [bigger] ≠ configBlock [smaller] := by
  decide

-- The old blocks ignored every module after the first.
theorem old_ignores_later_modules (m1 m2 : Module) :
    oldBlock [m1, m2] = oldBlock [m1] := rfl

-- The new blocks keep them.
theorem new_keeps_later_modules (m1 m2 : Module) :
    configBlock [m1, m2] ≠ configBlock [m1] := by
  simp [configBlock]

-- The old code had no block at all for an empty lens.
theorem old_empty_lens_has_none : oldBlock [] = none := rfl

end LensId
