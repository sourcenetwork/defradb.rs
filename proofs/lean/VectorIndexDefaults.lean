/-!
# VectorIndexDefaults: filling in Go's defaults is stable, and leaves no zero

The story, in plain words. Go writes a vector index description with parts
left unset: an algorithm or metric as the empty string, and an HNSW block
with zero parameters (or none at all). Before the index is stored, Rust
fills in Go's defaults (`VectorIndexDescription::normalized`), and reading
an empty string gives the default (`empty_is_default`). So the stored form
of a description changes: it is the normalized one.

What must never happen: normalizing a description that is already stored
changes it again (the stored form would drift on every read and rewrite),
an HNSW index is stored with a zero parameter, a non-HNSW index is touched,
or an empty string reads as anything but the default.

This file proves that normalizing twice is normalizing once
(`normalize_idempotent`), that an HNSW index comes out with every parameter
set (`normalize_hnsw_fills`), that any other index comes out unchanged
(`normalize_other_unchanged`), and that the empty string reads as the
default while any other string reads as itself (`empty_reads_default`,
`nonempty_reads_itself`).

Rust anchors: `crates/schema/src/vector_index_defaults.rs`
`VectorIndexDescription::normalized`, `empty_is_default`; the defaults are
`crates/schema/src/index.rs` `default_hnsw_m` (16),
`default_hnsw_ef_construction` (128), `default_hnsw_ef_search` (64).
-/

namespace VectorIndexDefaults

-- Which algorithm an index uses: HNSW, which has parameters, or another.
inductive Algorithm where
  -- Hierarchical navigable small world: the one with an HNSW block.
  | hnsw
  -- Any other algorithm, which has no HNSW block to fill.
  | other
  deriving DecidableEq

-- An HNSW block's parameters; zero means "unset" in Go's form.
structure Params where
  -- Neighbours per node.
  m : Nat
  -- Candidates kept while building.
  efConstruction : Nat
  -- Candidates kept while searching.
  efSearch : Nat
  deriving DecidableEq

-- Go's defaults, as `HnswParams::default`.
def defaults : Params := { m := 16, efConstruction := 128, efSearch := 64 }

-- The parts of a description normalization looks at.
structure Desc where
  -- Its algorithm.
  algorithm : Algorithm
  -- Its HNSW block, or none.
  hnsw : Option Params

-- Each zero parameter replaced by its default, the others kept.
def fill (p : Params) : Params :=
  { m := if p.m = 0 then defaults.m else p.m
    efConstruction := if p.efConstruction = 0 then defaults.efConstruction else p.efConstruction
    efSearch := if p.efSearch = 0 then defaults.efSearch else p.efSearch }

-- `normalized`: an HNSW index gets a block (an empty one if it had none)
-- with its zero parameters filled; any other index is left as it is.
def normalize (d : Desc) : Desc :=
  match d.algorithm with
  -- HNSW: fill the block, or the defaults if there was none.
  | .hnsw => { d with hnsw := some (fill (d.hnsw.getD defaults)) }
  -- Anything else: unchanged.
  | .other => d

-- Filling a block twice is filling it once: a filled parameter is never zero.
theorem fill_idempotent (p : Params) : fill (fill p) = fill p := by
  -- Split on which parameters were zero; each case is arithmetic on literals.
  unfold fill
  by_cases h1 : p.m = 0 <;> by_cases h2 : p.efConstruction = 0 <;>
    by_cases h3 : p.efSearch = 0 <;> simp [h1, h2, h3, defaults]

-- A filled block has no zero parameter.
theorem fill_nonzero (p : Params) :
    (fill p).m ≠ 0 ∧ (fill p).efConstruction ≠ 0 ∧ (fill p).efSearch ≠ 0 := by
  -- Each parameter is either its nonzero default or a nonzero original.
  unfold fill
  by_cases h1 : p.m = 0 <;> by_cases h2 : p.efConstruction = 0 <;>
    by_cases h3 : p.efSearch = 0 <;> simp [h1, h2, h3, defaults]

-- Normalizing a normalized description changes nothing: the stored form is
-- stable across reads and rewrites.
theorem normalize_idempotent (d : Desc) : normalize (normalize d) = normalize d := by
  -- Split on the algorithm.
  cases h : d.algorithm with
  -- HNSW: the second pass refills an already filled block.
  | hnsw => simp [normalize, h, fill_idempotent]
  -- Other: both passes leave it alone.
  | other => simp [normalize, h]

-- An HNSW index comes out with a block and every parameter set.
theorem normalize_hnsw_fills (d : Desc) (h : d.algorithm = .hnsw) :
    ∃ p, (normalize d).hnsw = some p ∧ p.m ≠ 0 ∧ p.efConstruction ≠ 0 ∧ p.efSearch ≠ 0 := by
  -- The block is the filled one, which has no zero parameter.
  exact ⟨fill (d.hnsw.getD defaults), by simp [normalize, h], fill_nonzero _⟩

-- Any other index comes out exactly as it went in.
theorem normalize_other_unchanged (d : Desc) (h : d.algorithm = .other) :
    normalize d = d := by
  -- The other branch returns the description itself.
  simp [normalize, h]

-- `empty_is_default`: the empty string reads as the default; any other string
-- is read as usual by `parse`.
def emptyIsDefault {α : Type} (default : α) (parse : String → Option α) (s : String) : Option α :=
  if s = "" then some default else parse s

-- The empty string reads as the default.
theorem empty_reads_default {α : Type} (default : α) (parse : String → Option α) :
    emptyIsDefault default parse "" = some default := by
  -- The empty-string branch answers the default.
  simp [emptyIsDefault]

-- Any other string reads exactly as `parse` reads it.
theorem nonempty_reads_itself {α : Type} (default : α) (parse : String → Option α)
    (s : String) (h : s ≠ "") : emptyIsDefault default parse s = parse s := by
  -- The other branch hands the string to `parse`.
  simp [emptyIsDefault, h]

end VectorIndexDefaults
