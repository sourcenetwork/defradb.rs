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
it was. If the `MANIFEST` cannot be read at all (a directory where a file
should be, say), the start fails with the error that reading it gave, and
the directory is left as it was too. Otherwise regolith opens it.

What must never happen: a Badger directory reaches regolith (it changes
files there and fails with the engine's message), or a regolith store, or a
new empty directory, is refused or fails the look (a node could not start on
its own data), or a failed look changes the directory.

This file proves that a Badger `MANIFEST` is refused (`badger_manifest_refused`),
that a `KEYREGISTRY` is refused whatever the `MANIFEST` says
(`key_registry_refused`), that an unreadable `MANIFEST` fails the read
(`unreadable_manifest_fails`), that a regolith store opens
(`regolith_store_opens`), that an empty directory opens
(`empty_directory_opens`), that a refused start leaves the directory
unchanged (`refused_start_changes_nothing`), that a failed read leaves it
unchanged too (`read_failed_start_changes_nothing`), and that only a start
the look does not refuse or fail opens it (`only_an_unrefused_start_opens`).
Nothing here is concurrent: one start looks, then opens, refuses or fails,
so there is no TLA+ model.

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

-- What the lookup made of the `MANIFEST` file: what the node can tell before
-- anything opens the directory.
inductive Manifest where
  -- There is no `MANIFEST` file at all.
  | absent
  -- The file is there but opening or reading it failed for any reason but a
  -- short read: a directory sitting where the file should be, or a read the
  -- file system refused. The code answers with that read error.
  | unreadable
  -- The file was read: its bytes, however many there are.
  | bytes (content : List Char)
deriving DecidableEq

-- What the node can see in a data directory before anything opens it.
structure Directory where
  -- What the lookup made of `MANIFEST`.
  manifest : Manifest
  -- Whether a file named `KEYREGISTRY` is there.
  keyRegistry : Bool
  -- The names of every entry in the directory, to tell whether a start changed it.
  entries : List String

-- `manifest_is_badger`: the MANIFEST's first four bytes are Badger's magic.
-- A missing MANIFEST, or one shorter than four bytes, is not Badger's:
-- `take 4` of a shorter list is shorter than the magic, so never equal to it.
-- An unreadable MANIFEST was never compared: the code stopped at the error.
def manifestIsBadger : Manifest → Bool
  -- No MANIFEST: nothing to say it is Badger's.
  | .absent => false
  -- Unreadable: the code fails on the read before any comparison.
  | .unreadable => false
  -- A MANIFEST: Badger's when its first four bytes are the magic.
  | .bytes bytes => bytes.take 4 == badgerMagic

-- `refuse_badger`: the directory is refused when it holds a KEYREGISTRY or a Badger MANIFEST.
def refused (d : Directory) : Bool :=
  -- Either Badger file is enough.
  d.keyRegistry || manifestIsBadger d.manifest

-- What a start ends with.
inductive Outcome where
  -- Refused with the Go datastore error; regolith never ran.
  | refusedGoStore
  -- The MANIFEST could not be read: the start fails with that read error, and
  -- regolith never ran.
  | readFailed
  -- regolith opened the directory.
  | opened
deriving DecidableEq

-- regolith opening a directory: it adds its lock and its folders, among other files.
def engineOpen (d : Directory) : Directory :=
  -- The directory gains the entries regolith makes.
  { d with entries := d.entries ++ ["LOCK", "sst", "wal"] }

-- A start: look first, then refuse, fail the read, or let regolith open.
def start (d : Directory) : Outcome × Directory :=
  -- A KEYREGISTRY is Badger's without reading anything: refuse, and hand the
  -- directory back as it was. `refuse_badger` checks it before the MANIFEST.
  if d.keyRegistry then (.refusedGoStore, d)
  -- No KEYREGISTRY: what did the MANIFEST read give?
  else match d.manifest with
    -- The read itself failed: fail the start with it, nothing changed.
    | .unreadable => (.readFailed, d)
    -- A MANIFEST: Badger's first bytes refuse; anything else opens.
    | manifest =>
      if manifestIsBadger manifest then (.refusedGoStore, d) else (.opened, engineOpen d)

-- A MANIFEST that starts with Badger's magic is refused, whatever follows it
-- and whatever else the directory holds.
theorem badger_manifest_refused (rest : List Char) (keyRegistry : Bool) (entries : List String) :
    (start ⟨.bytes (badgerMagic ++ rest), keyRegistry, entries⟩).1 = .refusedGoStore := by
  -- Either a KEYREGISTRY decides first, or the first four bytes are the magic
  -- itself, so the comparison holds.
  cases keyRegistry <;> simp [start, manifestIsBadger, badgerMagic]

-- A KEYREGISTRY file is refused, whatever the MANIFEST holds or whether there is one.
theorem key_registry_refused (manifest : Manifest) (entries : List String) :
    (start ⟨manifest, true, entries⟩).1 = .refusedGoStore := by
  -- The KEYREGISTRY branch is taken before the MANIFEST is looked at.
  simp [start]

-- An unreadable MANIFEST (a directory where the file should be, say) fails the
-- start with the read error, and no regolith open is attempted.
theorem unreadable_manifest_fails (entries : List String) :
    (start ⟨.unreadable, false, entries⟩).1 = .readFailed := by
  -- No KEYREGISTRY and no read: the failing branch is taken.
  simp [start]

-- A regolith store (a MANIFEST that starts with regolith's stamp, and no
-- KEYREGISTRY) is never refused: the node starts on its own data.
theorem regolith_store_opens (rest : List Char) (entries : List String) :
    (start ⟨.bytes (regolithMagic ++ rest), false, entries⟩).1 = .opened := by
  -- The first four bytes are `REGO`, which are not `Bdgr`, so the open branch is taken.
  simp [start, manifestIsBadger, regolithMagic, badgerMagic]

-- A new empty directory (no MANIFEST, no KEYREGISTRY) is never refused.
theorem empty_directory_opens (entries : List String) :
    (start ⟨.absent, false, entries⟩).1 = .opened := by
  -- Neither file is there to refuse or fail on.
  simp [start, manifestIsBadger]

-- A refused start hands the directory back exactly as it found it: regolith
-- never ran, so it made no folder and wrote no file there.
theorem refused_start_changes_nothing (d : Directory) (h : refused d = true) :
    start d = (.refusedGoStore, d) := by
  -- Look at the two files the start reads.
  obtain ⟨manifest, keyRegistry, entries⟩ := d
  by_cases hk : keyRegistry = true
  -- A KEYREGISTRY alone refuses, before the MANIFEST is read.
  · simp [start, hk]
  -- Without one, `refused` says the MANIFEST's first bytes are the magic.
  · have hb : manifestIsBadger manifest = true := by
      simpa [refused, hk] using h
    cases manifest with
    -- No MANIFEST can never be Badger's, so `hb` is a contradiction.
    | absent => simp [manifestIsBadger] at hb
    -- Unreadable is never compared, so `hb` is a contradiction.
    | unreadable => simp [manifestIsBadger] at hb
    -- A MANIFEST whose first four bytes are the magic takes the refusing branch.
    | bytes content =>
      have htake : content.take 4 = badgerMagic := by
        simpa [manifestIsBadger] using hb
      simp [start, hk, manifestIsBadger, htake]

-- A start that fails the read hands the directory back exactly as it found
-- it: the failure came from looking, and regolith never ran.
theorem read_failed_start_changes_nothing (d : Directory) (hu : d.manifest = .unreadable)
    (hk : d.keyRegistry = false) : start d = (.readFailed, d) := by
  -- No KEYREGISTRY and an unreadable MANIFEST: the failing branch, returning `d`.
  obtain ⟨manifest, keyRegistry, entries⟩ := d
  simp only at hu hk
  subst hu
  simp [start, hk]

-- A start that opens the directory is exactly one the look neither refused nor
-- failed: an unreadable MANIFEST never reaches regolith.
theorem only_an_unrefused_start_opens (d : Directory) :
    (start d).1 = .opened ↔ refused d = false ∧ d.manifest ≠ .unreadable := by
  -- Look at the two files the start reads.
  obtain ⟨manifest, keyRegistry, entries⟩ := d
  cases manifest with
  -- No MANIFEST: nothing is Badger's, and nothing failed to read.
  | absent => cases keyRegistry <;> simp [start, refused, manifestIsBadger]
  -- Unreadable: it fails the read, which is not opening.
  | unreadable => cases keyRegistry <;> simp [start, refused, manifestIsBadger]
  -- A MANIFEST: it opens exactly when its first bytes are not the magic.
  | bytes content =>
    cases keyRegistry with
    -- A KEYREGISTRY refuses, so no prefix decides it.
    | true => simp [start, refused]
    -- Without one, Badger's magic refuses and anything else opens.
    | false =>
      by_cases hb : content.take 4 = badgerMagic
      -- Badger's: refused, not opened.
      · simp [start, refused, manifestIsBadger, hb]
      -- Not Badger's: opened.
      · simp [start, refused, manifestIsBadger, hb]

end ForeignStoreRefusal
