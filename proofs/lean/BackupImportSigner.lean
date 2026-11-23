/-!
# BackupImportSigner — an import writes as a create on the same edge

The story. A caller imports a backup file through one of four edges: the
in-process Node, the HTTP server a NodeBuilder starts, the HTTP server
`defradb start` starts, or the C bindings. Each edge turns a write into a
request with its own rules: who the request acts as, which key signs its
blocks, and who owns the document it creates. The import hands each of its
writes to the edge's own request path (its `ImportHost`), so the request it
sends is the one a create by the same caller would send. The laws below say
the import is signed, acts and owns exactly as that create does.

The import also gathers documents into batches, and checks a batch's aliases
against the store just before it saves the batch. A batch is saved before a
document whose alias it already holds joins it, so no batch holds one alias
twice. A batch whose check refuses it saved nothing, and its documents are
then saved one at a time, up to the first one refused. Together these make
saving a batch exactly what saving its documents one at a time does: the
same documents saved, the same one refused, and never one alias twice.

Who acts: the caller; the edge, which builds the request; the driver, which
batches. Nothing here is temporal: the ordering and the races are the TLA+
model `proofs/tla/BackupImportWritePath.tla`.

What must never happen:
* an imported document signed by a key a create would not sign with;
* an import acting as, or owned by, someone a create would not be;
* two documents of one file saved under one alias, or one under an alias
  the store already holds;
* a refused batch losing a document that comes before the one refused.

Rust anchors: `crates/database/db/src/backup/import.rs` `import_database_as`,
`save`, `create`,
`crates/database/db/src/backup/batch.rs` `Batch::must_save_before`;
`crates/entry-points/defra-node/src/ops/backup.rs` `NodeHost`;
`crates/surfaces/http/src/request_path.rs` `Server`;
`crates/entry-points/cli/src/commands/start/server_query.rs`
`setup_query_runner`; `crates/engine/query/src/runner/mod.rs`
`QueryRunner::resolve_identity`;
`crates/entry-points/ffi/src/backup/import.rs` `FfiHost`;
`crates/entry-points/defra-node/src/lib.rs` `execute_prepared_request`.
-/

namespace BackupImportSigner

/-! ## 1. Edges, requests and who they act as -/

/-- The four edges an import comes through. -/
inductive Edge where
  -- the in-process Node
  | node
  -- the HTTP server a NodeBuilder starts
  | httpNodeBuilder
  -- the HTTP server `defradb start` starts
  | httpCliStart
  -- the C bindings
  | ffi
  deriving DecidableEq

/-- The node a request lands on: whether it signs with its identity, the
number standing for that identity (and its key), and the identity
`defradb start` was given when its document ACP is local, which its runner
acts as for a request that names none (`setup_query_runner`,
`with_default_identity`). -/
structure NodeSetup where
  -- TRUE when the node signs with its identity
  signs : Bool
  -- the node identity's number
  id : Nat
  -- `defradb start`'s own identity for anonymous requests, or none
  cliDefault : Option Nat

/-- A write's request as an edge builds it: the identity it names, and the
write's own signing choice (`enableSigning`). -/
structure Request where
  -- the caller's identity, by number, or none
  identity : Option Nat
  -- the write's own signing choice, or none to follow the node
  choice : Option Bool
  deriving DecidableEq

/-- Who request `r` acts as on edge `e`: the Node fills in its own identity
for a request that names none when it signs with it
(`with_default_query_identity`); `defradb start`'s runner fills in the
identity it was given (`QueryRunner::resolve_identity`); every other edge
acts as the identity the request names. -/
def actor (n : NodeSetup) (e : Edge) (r : Request) : Option Nat :=
  -- look at the edge
  match e with
  -- the Node: the request's identity, else the node's when it signs
  | .node => if n.signs then (r.identity <|> some n.id) else r.identity
  -- `defradb start`: the request's identity, else the one it was given
  | .httpCliStart => r.identity <|> n.cliDefault
  -- any other edge: the request's identity
  | _ => r.identity

/-- The key that signs request `r`'s blocks: none when the write says
`false`; else, when signing is on (the write's choice, else the node's
setting), the caller's key, else the node's. -/
def signer (n : NodeSetup) (r : Request) : Option Nat :=
  -- look at the write's choice
  match r.choice with
  -- `false` never signs
  | some false => none
  -- otherwise signing follows the choice, or the node
  | _ => if r.choice.getD n.signs then (r.identity <|> some n.id) else none

/-- Who owns the document request `r` creates: the identity it acts as. -/
def creator (n : NodeSetup) (e : Edge) (r : Request) : Option Nat :=
  -- document ACP registers the acting identity as the owner
  actor n e r

/-- The request a create by caller `c` sends through any edge, with no
signing choice of its own. -/
def createRequest (c : Option Nat) : Request :=
  -- the caller's identity, no choice
  { identity := c, choice := none }

/-- The request an import write by caller `c` sends through the edge's host:
the host gives it the caller's context, as the edge gives a create. -/
def importRequest (c : Option Nat) : Request :=
  -- the caller's identity, no choice: what the host's `run` sends
  { identity := c, choice := none }

/-! ## 2. The laws -/

/-- The host's request is the create's request. -/
theorem import_request_is_create_request (c : Option Nat) :
    importRequest c = createRequest c := by
  -- both build the same record
  rfl

/-- Signer law: an import write is signed by the key a create by the same
caller on the same node is signed by. -/
theorem signer_law (n : NodeSetup) (c : Option Nat) :
    signer n (importRequest c) = signer n (createRequest c) := by
  -- the requests are equal
  rw [import_request_is_create_request]

/-- Actor law: on every edge an import write acts as a create would. -/
theorem actor_law (n : NodeSetup) (e : Edge) (c : Option Nat) :
    actor n e (importRequest c) = actor n e (createRequest c) := by
  -- the requests are equal
  rw [import_request_is_create_request]

/-- Creator law: an imported document is owned as a created one is. -/
theorem creator_law (n : NodeSetup) (e : Edge) (c : Option Nat) :
    creator n e (importRequest c) = creator n e (createRequest c) := by
  -- the owner is the actor, and the actor law holds
  simp [creator, actor_law]

/-- The NodeBuilder's HTTP server: an anonymous import acts as no one, as an
anonymous create through it does, and a named one as its caller. -/
theorem http_actor_is_the_caller (n : NodeSetup) (c : Option Nat) :
    actor n .httpNodeBuilder (importRequest c) = c := by
  -- the request's identity, unchanged
  rfl

/-- `defradb start`'s HTTP server: a named import acts as its caller, and an
anonymous one as the identity the server was given (none when it has none
or its ACP is remote), as an anonymous create through it does. -/
theorem cli_start_actor_falls_back_to_its_identity (n : NodeSetup)
    (c : Option Nat) :
    actor n .httpCliStart (importRequest c) = (c <|> n.cliDefault) := by
  -- the request's identity, else the server's own
  rfl

/-- So on a `defradb start` given identity `d`, an anonymous import is owned
by `d`, exactly as an anonymous create there is. -/
theorem cli_start_anonymous_import_owned_by_its_identity (s : Bool) (i d : Nat) :
    creator ⟨s, i, some d⟩ .httpCliStart (importRequest none) = some d ∧
      creator ⟨s, i, some d⟩ .httpCliStart (createRequest none) = some d := by
  -- both sides compute to d
  exact ⟨rfl, rfl⟩

/-- On a signing node an import signs with the caller's key when it names
one, and with the node's when it is anonymous. -/
theorem signing_node_signs_every_import (n : NodeSetup) (h : n.signs = true)
    (c : Option Nat) :
    signer n (importRequest c) = (c <|> some n.id) := by
  -- the choice is absent, so the node's setting decides, and it signs
  simp [signer, importRequest, h]

/-! ## 3. The bug this replaces -/

/-- Main's Node import acted as the identity its context named, skipping the
node's default. -/
def oldNodeImportActor (c : Option Nat) : Option Nat :=
  -- no default filled in
  c

/-- RED witness: on a signing Node, an anonymous create was owned by the
node while main's anonymous import was owned by no one. -/
theorem old_anonymous_import_owned_by_no_one :
    creator ⟨true, 7, none⟩ .node (createRequest none) = some 7 ∧
      oldNodeImportActor none = none := by
  -- both sides compute to concrete values
  exact ⟨rfl, rfl⟩

/-! ## 4. A batch saves what one document at a time saves -/

/-- Save the documents of `batch`, aliases by number, one at a time against
the store `reg` (the aliases it holds): each one is checked, then saved and
its alias registered; the first one whose alias is held stops it. The
aliases saved, in order. -/
def saveEach (reg : List Nat) : List Nat → List Nat
  -- nothing left: nothing more saved
  | [] => []
  -- the next document: refused when its alias is held, else saved and
  -- registered before the one after it is checked
  | a :: rest => if a ∈ reg then [] else a :: saveEach (reg ++ [a]) rest

/-- Save `batch` as the import does: one check for the whole batch, then one
create of all of it; a batch the check refuses saved nothing, and is saved
again one document at a time. -/
def saveBatch (reg batch : List Nat) : List Nat :=
  -- every alias free: the batch is saved together
  if ∀ a ∈ batch, a ∉ reg then batch
  -- refused: one at a time, up to the first one refused
  else saveEach reg batch

/-- A batch whose aliases are all free and distinct is saved whole one at a
time too. -/
theorem saveEach_all_free :
    ∀ (reg batch : List Nat), batch.Nodup → (∀ a ∈ batch, a ∉ reg) →
      saveEach reg batch = batch
  -- the empty batch saves nothing either way
  | _, [], _, _ => rfl
  -- a document, then the rest
  | reg, a :: rest, hnd, hfree => by
    -- a is not repeated later, and the rest has no repeats
    have hcons := List.nodup_cons.mp hnd
    -- a is free in the store
    have ha : a ∉ reg := hfree a (by simp)
    -- the rest stays free once a is registered: none of it is a, none is held
    have hrest : ∀ x ∈ rest, x ∉ reg ++ [a] := by
      -- take a later document
      intro x hx
      -- it is neither held nor a
      simp only [List.mem_append, List.mem_singleton, not_or]
      -- held would contradict the check; equal to a would repeat a
      exact ⟨hfree x (by simp [hx]), fun h => hcons.1 (h ▸ hx)⟩
    -- a is saved, and the rest is saved whole by the same law
    simp [saveEach, ha, saveEach_all_free (reg ++ [a]) rest hcons.2 hrest]

/-- Batch law: a batch with no alias twice (what saving before a pending
alias guarantees, `Batch::must_save_before`) saves exactly what saving its
documents one at a time saves. -/
theorem saveBatch_eq_saveEach (reg batch : List Nat) (hb : batch.Nodup) :
    saveBatch reg batch = saveEach reg batch := by
  -- split on the batch's check
  unfold saveBatch
  split
  -- free: saving it together is saving it whole one at a time
  · next h => exact (saveEach_all_free reg batch hb h).symm
  -- refused: the import saves one at a time
  · rfl

/-- Adding an alias missing from a list keeps it without repeats. -/
theorem nodup_snoc {l : List Nat} {a : Nat} (h : l.Nodup) (ha : a ∉ l) :
    (l ++ [a]).Nodup := by
  -- a list without repeats is one whose elements are pairwise distinct
  unfold List.Nodup at *
  -- pairwise over an append: each side, and every pair across
  rw [List.pairwise_append]
  -- the left side holds; one element is pairwise trivially; across, a is new
  refine ⟨h, List.pairwise_singleton _ _, ?_⟩
  -- every element of l differs from a
  intro x hx y hy
  -- y is a
  simp at hy
  -- so x = y would put a in l
  subst hy
  -- which ha refutes
  exact fun hxy => ha (hxy ▸ hx)

/-- One at a time never saves an alias twice, nor one the store held. -/
theorem saveEach_nodup :
    ∀ (reg batch : List Nat), reg.Nodup → (reg ++ saveEach reg batch).Nodup
  -- nothing saved: the store as it was
  | reg, [], h => by simpa [saveEach] using h
  -- a document, then the rest
  | reg, a :: rest, h => by
    -- split on whether its alias is held
    by_cases ha : a ∈ reg
    -- held: refused, nothing more saved
    · simpa [saveEach, ha] using h
    -- free: the store with a registered has no repeats, nor does the rest
    · have := saveEach_nodup (reg ++ [a]) rest (nodup_snoc h ha)
      -- regroup reg ++ [a] ++ saved into reg ++ a :: saved
      simpa [saveEach, ha, List.append_assoc] using this

/-- No duplicate alias: saving a batch with no alias twice puts no alias in
the store twice, and none the store already held. -/
theorem no_duplicate_alias (reg batch : List Nat) (h0 : reg.Nodup)
    (hb : batch.Nodup) : (reg ++ saveBatch reg batch).Nodup := by
  -- the batch saves what one at a time saves, which never repeats
  rw [saveBatch_eq_saveEach reg batch hb]
  exact saveEach_nodup reg batch h0

/-- Prefix law: one at a time saves a first part of the batch, and the
document right after that part is refused because its alias is held. So a
failed batch keeps every document before the one refused. -/
theorem keeps_prefix_until_refused :
    ∀ (reg batch : List Nat), ∃ unsaved,
      saveEach reg batch ++ unsaved = batch ∧
        ∀ a later, unsaved = a :: later → a ∈ reg ++ saveEach reg batch
  -- nothing in the batch: nothing unsaved
  | _, [] => ⟨[], rfl, fun _ _ h => nomatch h⟩
  -- a document, then the rest
  | reg, a :: rest => by
    -- split on whether its alias is held
    by_cases ha : a ∈ reg
    -- held: nothing saved, the whole batch unsaved, starting with a, held
    · refine ⟨a :: rest, by simp [saveEach, ha], ?_⟩
      -- the first unsaved document is a, which the store holds
      intro b later hb
      -- b is a
      simp only [List.cons.injEq] at hb
      -- so it is held
      simp [saveEach, ha, hb.1 ▸ ha]
    -- free: a is saved, and the rest keeps its own prefix
    · obtain ⟨unsaved, hsplit, hstop⟩ := keeps_prefix_until_refused (reg ++ [a]) rest
      refine ⟨unsaved, by simp [saveEach, ha, hsplit], ?_⟩
      -- the first unsaved document is held by the store with a and the rest saved
      intro b later hb
      -- regroup reg ++ [a] ++ saved into reg ++ a :: saved
      simpa [saveEach, ha, List.append_assoc] using hstop b later hb

/-! ## 5. The bugs the guards rule out -/

/-- RED variant: a batch the check refuses fails as a whole and saves
nothing (no one-at-a-time retry). -/
def saveBatchDropped (reg batch : List Nat) : List Nat :=
  -- free: saved together; refused: nothing
  if ∀ a ∈ batch, a ∉ reg then batch else []

/-- RED witness: with alias 2 held, the batch [1, 2] dropped whole loses
document 1, which one at a time saves before 2 is refused. -/
theorem dropped_batch_loses_the_prefix :
    saveBatchDropped [2] [1, 2] = [] ∧ saveEach [2] [1, 2] = [1] := by
  -- both sides compute to concrete values
  decide

/-- RED witness: without saving before a pending alias, a batch can hold
alias 1 twice; it passes the check and saves 1 twice. -/
theorem no_flush_saves_a_duplicate :
    saveBatch [] [1, 1] = [1, 1] ∧ ¬ ([] ++ saveBatch [] [1, 1]).Nodup := by
  -- both sides compute to concrete values
  decide

end BackupImportSigner
