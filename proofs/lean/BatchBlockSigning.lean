/-!
# BatchBlockSigning — a batch session writes its blocks unsigned

The story. A request that writes blocks may carry a signing key, and may run
in a batch signing session. Outside a session, each block it writes is
signed as before. Inside one, the session collects the blocks' CIDs and signs
their Merkle root once, so Go Shinzo writes the blocks themselves unsigned,
and so does Rust.

Who acts: the request (its key and its session) and the block builder,
which asks the request for the key to sign each block with.

What must never happen:
* a block written in a batch session carries its own signature;
* a block written outside a session loses the signature it had before;
* the session's root misses a document's composite block.

Rust anchors: `RequestContext::signing` in
`crates/defra-core/src/request_context.rs`, read by every block-writing path
in `crates/db/src/write` and `crates/db/src/block/builder`, and
`collect_on_commit` in `crates/db/src/block/builder/batch_signing.rs`.
Go anchor (`shinzo-develop` 2a899b860): `addDelta` in
`internal/core/block/store.go`.
-/

namespace BatchBlockSigning

/-- What a session keeps: composite blocks only (Go's), or every block. -/
inductive Scope where
  | composite
  | full
  deriving DecidableEq

/-- The kinds of block a write produces. -/
inductive Kind where
  | composite
  | field
  | collection
  deriving DecidableEq

/-- A request: whether it carries a key, and the session it runs in, if any. -/
structure Request where
  -- a signing key is configured
  keyed : Bool
  -- the batch session's scope, or none outside a session
  session : Option Scope

/-- The key the builder gets to sign a block with: the request's, unless the
    request is in a session (`RequestContext::signing`). -/
def signingKey (r : Request) : Bool := r.keyed && r.session.isNone

/-- Whether a block is signed: the builder has a key and the block's own rule
    (`perBlock`, e.g. a field block above priority 1 is not) says to sign it. -/
def signed (r : Request) (perBlock : Kind → Bool) (k : Kind) : Bool :=
  signingKey r && perBlock k

/-- Whether the session collects a block of kind `k` (`collect_on_commit`). -/
def collected (scope : Scope) (k : Kind) : Bool :=
  match scope with
  -- Go's scope: composites only
  | .composite => k == .composite
  -- the opt-in scope: everything
  | .full => true

/-- In a session, no block is signed, whatever its kind and the key. -/
theorem batched_blocks_unsigned (r : Request) (s : Scope) (h : r.session = some s)
    (perBlock : Kind → Bool) (k : Kind) : signed r perBlock k = false := by
  -- the key is withheld in a session
  simp [signed, signingKey, h]

/-- Outside a session, a block is signed exactly as before: with a key, by its own rule. -/
theorem unbatched_unchanged (r : Request) (h : r.session = none)
    (perBlock : Kind → Bool) (k : Kind) : signed r perBlock k = (r.keyed && perBlock k) := by
  -- no session: the key is the request's
  simp [signed, signingKey, h]

/-- In either scope, the session collects every composite block, so each
    document's root block is under the signed Merkle root. -/
theorem composites_collected (s : Scope) : collected s .composite = true := by
  -- one case per scope
  cases s <;> rfl

/-- RED: the old builder, which signed with the request's key in a session too. -/
def oldSigned (r : Request) (perBlock : Kind → Bool) (k : Kind) : Bool := r.keyed && perBlock k

/-- RED: the old builder signs a composite written in a session. -/
theorem neg_old_signs_in_batch :
    ¬ ∀ r s perBlock k, r.session = some s → oldSigned r perBlock k = false := by
  -- a keyed request in a session, writing a composite its rule signs
  intro hall
  have := hall ⟨true, some .composite⟩ .composite (fun _ => true) .composite rfl
  simp [oldSigned] at this

end BatchBlockSigning

#print axioms BatchBlockSigning.batched_blocks_unsigned
#print axioms BatchBlockSigning.unbatched_unchanged
#print axioms BatchBlockSigning.composites_collected
#print axioms BatchBlockSigning.neg_old_signs_in_batch
