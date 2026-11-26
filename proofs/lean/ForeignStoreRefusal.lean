/-!
# ForeignStoreRefusal: a Go DefraDB datastore is refused before the engine touches it, and a regolith store never is

The story, in plain words. A node starts on a data directory. Go DefraDB
keeps a Badger datastore in the same place, so a rootdir Go created holds
Badger's files where this node expects regolith's: a `MANIFEST` that starts
with Badger's magic text `Bdgr`, and a `KEYREGISTRY` file. regolith cannot
read either. Opened anyway, it makes folders of its own in the directory and
then fails on the `MANIFEST`'s first bytes with a message that says nothing
about Go. So before regolith opens the directory, the node looks at those
two files. If either is Badger's, the start is refused with an error that
says what the directory holds and what to do, and the directory is left as
it was. Otherwise regolith opens it.

What must never happen: a Badger directory reaches regolith (it changes
files there and fails with the engine's message), or a regolith store, or a
new empty directory, is refused (a node could not start on its own data).

This file proves that a Badger `MANIFEST` is refused (`badger_manifest_refused`),
that a `KEYREGISTRY` is refused whatever the `MANIFEST` says
(`key_registry_refused`), that a regolith store opens
(`regolith_store_opens`), that an empty directory opens
(`empty_directory_opens`), and that a refused start leaves the directory
unchanged (`refused_start_changes_nothing`). Nothing here is concurrent: one
start looks, then opens or refuses, so there is no TLA+ model.

Rust anchors: `crates/engine/storage/src/backends/regolith/foreign.rs`
`refuse_badger`, `manifest_is_badger`, `BADGER_MANIFEST_MAGIC`,
`BADGER_KEY_REGISTRY`; called first in
`crates/engine/storage/src/backends/regolith/rewrite/mod.rs` `open_with_steps`
and `crates/engine/storage/src/backends/regolith/transaction.rs`
`RegolithTxn::open_read_only`.
-/

namespace ForeignStoreRefusal

-- Badger's `magicText`, the first four bytes of every Badger MANIFEST. The
-- Rust test `the_magic_matches_the_lean_model` reads this line.
def badgerMagic : List Char := ['B', 'd', 'g', 'r']

-- regolith's stamp, the first seven bytes of every regolith MANIFEST, sealed or not.
def regolithMagic : List Char := ['R', 'E', 'G', 'O', 'M', 'A', 'N']

-- What the node can see in a data directory before anything opens it.
structure Directory where
  -- The bytes of `MANIFEST`, or `none` when there is no such file.
  manifest : Option (List Char)
  -- Whether a file named `KEYREGISTRY` is there.
  keyRegistry : Bool
  -- The names of every entry in the directory, to tell whether a start changed it.
  entries : List String

-- `manifest_is_badger`: the MANIFEST's first four bytes are Badger's magic.
-- A missing MANIFEST, or one shorter than four bytes, is not Badger's:
-- `take 4` of a shorter list is shorter than the magic, so never equal to it.
def manifestIsBadger : Option (List Char) → Bool
  -- No MANIFEST: nothing to say it is Badger's.
  | none => false
  -- A MANIFEST: Badger's when its first four bytes are the magic.
  | some bytes => bytes.take 4 == badgerMagic

-- `refuse_badger`: the directory is refused when it holds a KEYREGISTRY or a Badger MANIFEST.
def refused (d : Directory) : Bool :=
  -- Either Badger file is enough.
  d.keyRegistry || manifestIsBadger d.manifest

-- What a start ends with.
inductive Outcome where
  -- Refused with the Go datastore error; regolith never ran.
  | refusedGoStore
  -- regolith opened the directory.
  | opened
deriving DecidableEq

-- regolith opening a directory: it adds its lock and its folders, among other files.
def engineOpen (d : Directory) : Directory :=
  -- The directory gains the entries regolith makes.
  { d with entries := d.entries ++ ["LOCK", "sst", "wal"] }

-- A start: look first, then refuse or let regolith open.
def start (d : Directory) : Outcome × Directory :=
  -- Badger's files: refuse, and hand the directory back as it was.
  if refused d then (.refusedGoStore, d)
  -- Anything else: regolith opens it.
  else (.opened, engineOpen d)

-- A MANIFEST that starts with Badger's magic is refused, whatever follows it
-- and whatever else the directory holds.
theorem badger_manifest_refused (rest : List Char) (keyRegistry : Bool) (entries : List String) :
    refused ⟨some (badgerMagic ++ rest), keyRegistry, entries⟩ = true := by
  -- The first four bytes are the magic itself, so the comparison holds.
  simp [refused, manifestIsBadger, badgerMagic]

-- A KEYREGISTRY file is refused, whatever the MANIFEST holds or whether there is one.
theorem key_registry_refused (manifest : Option (List Char)) (entries : List String) :
    refused ⟨manifest, true, entries⟩ = true := by
  -- `true || _` is `true`.
  simp [refused]

-- A regolith store (a MANIFEST that starts with regolith's stamp, and no
-- KEYREGISTRY) is never refused: the node starts on its own data.
theorem regolith_store_opens (rest : List Char) (entries : List String) :
    refused ⟨some (regolithMagic ++ rest), false, entries⟩ = false := by
  -- The first four bytes are `REGO`, which are not `Bdgr`.
  simp [refused, manifestIsBadger, regolithMagic, badgerMagic]

-- A new empty directory (no MANIFEST, no KEYREGISTRY) is never refused.
theorem empty_directory_opens (entries : List String) :
    refused ⟨none, false, entries⟩ = false := by
  -- Neither file is there to refuse.
  simp [refused, manifestIsBadger]

-- A refused start hands the directory back exactly as it found it: regolith
-- never ran, so it made no folder and wrote no file there.
theorem refused_start_changes_nothing (d : Directory) (h : refused d = true) :
    start d = (.refusedGoStore, d) := by
  -- The start takes its refusing branch.
  simp [start, h]

-- A start that is not refused is the only one that lets regolith open the directory.
theorem only_an_unrefused_start_opens (d : Directory) :
    (start d).1 = .opened ↔ refused d = false := by
  -- Unfold the start and split on the look.
  unfold start
  -- Each branch decides the outcome.
  cases refused d <;> simp

end ForeignStoreRefusal
