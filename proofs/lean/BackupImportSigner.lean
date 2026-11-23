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

The import also gathers documents into batches. A batch is flushed before a
document whose alias it already holds, so that document's alias check sees
the earlier one registered: checking in batches refuses exactly the
documents a one-at-a-time check refuses.

Who acts: the caller; the edge, which builds the request; the driver, which
batches. Nothing here is temporal: the ordering and the races are the TLA+
model `proofs/tla/BackupImportWritePath.tla`.

What must never happen:
* an imported document signed by a key a create would not sign with;
* an import acting as, or owned by, someone a create would not be;
* two documents of one file saved under one alias, or one under an alias
  the store already holds.

Rust anchors: `crates/database/db/src/backup/import.rs` `import_database_as`,
`crates/database/db/src/backup/batch.rs` `Batch::must_save_before`;
`crates/entry-points/defra-node/src/ops/backup.rs` `NodeHost`;
`crates/surfaces/http/src/request_path.rs` `Server`;
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

/-- The node a request lands on: whether it signs with its identity, and the
number standing for that identity (and its key). -/
structure NodeSetup where
  -- TRUE when the node signs with its identity
  signs : Bool
  -- the node identity's number
  id : Nat

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
(`with_default_query_identity`); every other edge acts as the identity the
request names. -/
def actor (n : NodeSetup) (e : Edge) (r : Request) : Option Nat :=
  -- look at the edge
  match e with
  -- the Node: the request's identity, else the node's when it signs
  | .node => if n.signs then (r.identity <|> some n.id) else r.identity
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

/-- The HTTP case: through either HTTP server an anonymous import acts as no
one, as an anonymous create through it does, and a named one as its caller. -/
theorem http_actor_is_the_caller (n : NodeSetup) (c : Option Nat)
    (e : Edge) (h : e = .httpNodeBuilder ∨ e = .httpCliStart) :
    actor n e (importRequest c) = c := by
  -- split on which HTTP server it is
  cases h with
  -- the NodeBuilder's: the request's identity
  | inl h => subst h; rfl
  -- `defradb start`'s: the request's identity
  | inr h => subst h; rfl

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
    creator ⟨true, 7⟩ .node (createRequest none) = some 7 ∧
      oldNodeImportActor none = none := by
  -- both sides compute to concrete values
  exact ⟨rfl, rfl⟩

/-! ## 4. Batches keep the one-at-a-time alias check -/

/-- The driver's state: the aliases the store holds, the aliases of the
documents gathered into the unsaved batch, and the aliases saved so far. -/
structure Driver where
  -- aliases the store holds (the ones it had, then every saved batch's)
  registered : List Nat
  -- aliases of the documents waiting in the batch
  pending : List Nat
  -- aliases of the documents saved by this import
  committed : List Nat

/-- Save the batch and register its aliases. -/
def flush (s : Driver) : Driver :=
  -- the batch's aliases join the store and the saved list; the batch empties
  { registered := s.registered ++ s.pending, pending := [],
    committed := s.committed ++ s.pending }

/-- Check alias `a` against the store and gather its document: a taken
alias fails the import. -/
def admit (s : Driver) (a : Nat) : Option Driver :=
  -- a held alias is the error `a document with the given ID already exists`
  if a ∈ s.registered then none
  -- a free one joins the batch
  else some { s with pending := s.pending ++ [a] }

/-- The driver's step for one document: flush first when the batch already
holds its alias, then check and gather it. -/
def gather (s : Driver) (a : Nat) : Option Driver :=
  -- the batch holds the alias: save it, so the check sees the alias stored
  if a ∈ s.pending then admit (flush s) a
  -- otherwise check against the store and gather
  else admit s a

/-- The step without the flush (the RED variant): check the store only. -/
def gatherNoFlush (s : Driver) (a : Nat) : Option Driver :=
  -- the batch is not consulted
  admit s a

/-- Batch-alias lemma: a document is refused exactly when an earlier one
(stored, or waiting in the batch) carries its alias, which is what checking
each document alone against everything before it refuses. -/
theorem gather_refuses_iff (s : Driver) (a : Nat) :
    gather s a = none ↔ (a ∈ s.registered ∨ a ∈ s.pending) := by
  -- split on whether the batch holds the alias
  by_cases hp : a ∈ s.pending
  -- it does: the flushed store holds it, so the check refuses it
  · simp [gather, hp, admit, flush]
  -- it does not: the plain store check decides
  · by_cases hr : a ∈ s.registered
    -- the store holds it: refused
    · simp [gather, hp, admit, hr]
    -- free everywhere: gathered
    · simp [gather, hp, admit, hr]

/-- What the driver keeps true: the store is what it had plus what was
saved, and no alias appears twice across the store and the batch. -/
def Good (r0 : List Nat) (s : Driver) : Prop :=
  -- the store holds the old aliases and the saved ones
  s.registered = r0 ++ s.committed ∧
  -- no alias twice in the store and the batch together
  (s.registered ++ s.pending).Nodup

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

/-- Flushing keeps the driver good. -/
theorem flush_good {r0 : List Nat} {s : Driver} (h : Good r0 s) :
    Good r0 (flush s) := by
  -- take the two facts apart
  obtain ⟨hreg, hnd⟩ := h
  -- the store grows by the batch, as the saved list does
  refine ⟨?_, ?_⟩
  -- r0 ++ committed ++ pending, regrouped
  · simp [flush, hreg, List.append_assoc]
  -- the same aliases, now all in the store
  · simpa [flush] using hnd

/-- Gathering a document keeps the driver good. -/
theorem admit_good {r0 : List Nat} {s t : Driver} {a : Nat} (h : Good r0 s)
    (ha : a ∉ s.pending) (hs : admit s a = some t) : Good r0 t := by
  -- take the two facts apart
  obtain ⟨hreg, hnd⟩ := h
  -- the check passed, so the store lacks the alias
  unfold admit at hs
  -- split on the store check
  by_cases hr : a ∈ s.registered
  -- refused: no new state
  · simp [hr] at hs
  -- gathered: the new state adds a to the batch
  · simp [hr] at hs
    -- the new state is the old one with a gathered
    subst hs
    -- the store is unchanged; a is new to the store and the batch
    refine ⟨hreg, ?_⟩
    -- regroup to (store ++ batch) ++ [a]
    rw [← List.append_assoc]
    -- a is in neither part
    exact nodup_snoc hnd (by simp [hr, ha])

/-- One step of the driver keeps it good. -/
theorem gather_good {r0 : List Nat} {s t : Driver} {a : Nat} (h : Good r0 s)
    (hs : gather s a = some t) : Good r0 t := by
  -- split on whether the batch holds the alias
  unfold gather at hs
  by_cases hp : a ∈ s.pending
  -- it does: flush, which keeps it good and empties the batch, then gather
  · simp [hp] at hs
    exact admit_good (flush_good h) (by simp [flush]) hs
  -- it does not: gather directly
  · simp [hp] at hs
    exact admit_good h hp hs

/-- The driver over a whole file: one step per document, stopping at the
first refusal, and a last flush. -/
def run (s : Driver) : List Nat → Option Driver
  -- the file is done: save what waits
  | [] => some (flush s)
  -- the next document: step, then the rest
  | a :: rest => (gather s a).bind (fun t => run t rest)

/-- Running the driver keeps it good. -/
theorem run_good {r0 : List Nat} :
    ∀ (file : List Nat) (s t : Driver), Good r0 s → run s file = some t → Good r0 t
  -- the empty file: the last flush keeps it good
  | [], s, t, h, hr => by
    -- the result is the flushed state
    simp [run] at hr
    subst hr
    exact flush_good h
  -- a document, then the rest
  | a :: rest, s, t, h, hr => by
    -- the step succeeded with some state u, and the rest ran from u
    simp [run, Option.bind_eq_some] at hr
    obtain ⟨u, hu, hrest⟩ := hr
    -- u is good, so the rest keeps it good
    exact run_good rest u t (gather_good h hu) hrest

/-- No duplicate alias: an import that finishes saved no two documents under
one alias and none under an alias the store held before it. -/
theorem no_duplicate_alias (r0 file : List Nat) (t : Driver)
    (h0 : r0.Nodup) (hr : run ⟨r0, [], []⟩ file = some t) :
    (r0 ++ t.committed).Nodup := by
  -- the start is good: the store is r0, nothing waits or is saved
  have hstart : Good r0 ⟨r0, [], []⟩ := ⟨by simp, by simpa using h0⟩
  -- so the end is good
  obtain ⟨hreg, hnd⟩ := run_good file _ t hstart hr
  -- the store at the end is r0 ++ committed, without repeats
  rw [hreg] at hnd
  -- no repeats means pairwise distinct
  unfold List.Nodup at hnd ⊢
  -- dropping the batch keeps it pairwise distinct
  exact (List.pairwise_append.mp hnd).1

/-- RED witness: without the flush, a file naming alias 1 twice gathers both
documents into one batch and saves both. -/
def runNoFlush (s : Driver) : List Nat → Option Driver
  -- the file is done: save what waits
  | [] => some (flush s)
  -- the next document: the store-only check, then the rest
  | a :: rest => (gatherNoFlush s a).bind (fun t => runNoFlush t rest)

/-- The RED run saves alias 1 twice. -/
theorem no_flush_saves_a_duplicate :
    (runNoFlush ⟨[], [], []⟩ [1, 1]).map (·.committed) = some [1, 1] := by
  -- the run computes to concrete values
  rfl

end BackupImportSigner
