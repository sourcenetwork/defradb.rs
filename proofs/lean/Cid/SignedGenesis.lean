/-!
# A signed create's ID is known before it commits

The story, in plain words. A node or a client makes a new document. It
builds one small block per field, signs each one, then builds the document's
top block (the genesis composite) whose links are the field blocks' content
addresses, and signs that too. The document's ID is a UUID of the genesis
block's content address. Whoever builds those blocks therefore knows the ID
before anything is written.

Who acts: the creator (a node that signs, or a client that signs its own
graph), the node that commits, a peer that receives the blocks, the
store-upgrade re-key of legacy documents, and a backup import.

What must never happen: a path commits the document under an ID other than
the one its creator computed. Each path derives the committed ID from the
genesis it holds, and this file proves, for the node, client-signed and peer
paths, which check makes that genesis the creator's; a RED theorem beside
each shows the ID moving once the check is gone. It shows the two
exceptions: backup import re-creates the document under a new genesis, so a
new ID whenever another key signs it, and the store-upgrade re-key moves a legacy document whose stored ID
is not its genesis's ID to that ID, keeping the old one as an alias. A lens
migration rewrites field values and never a genesis, so it is no path here.
It also proves that both kinds of signature reach
the ID, that a signer whose signature does not depend on a random nonce gives
the same ID every time, that one that does can give two IDs, that a create
whose ID is already stored is refused, and that a buggy builder which signs
the genesis after its address is taken loses the signer from the ID.

Hashes are the crypto boundary: we take them as injective (two different
blocks never share an address), as `proofs/README.md` records.

Rust owners:
* crates/database/db/src/block/builder/compute.rs `compute_document_blocks`
  (field blocks signed before their CIDs; the composite signed before its CID).
* crates/database/db/src/block/builder/mod.rs `derive_doc_id`
  (the ID is `DocID::new_v0` of the genesis CID).
* crates/database/db/src/block/builder/verified_client_create.rs
  `VerifiedClientCreate::verify`, `ExactSignatures::sign_sync` (a client-signed
  create replays only the client's signatures).
* crates/entry-points/defra-node/src/node_core/execute.rs `NodeCore::execute_verified_graph`
  (refuses a committed `_docID` other than `graph.doc_id()`).
* crates/surfaces/http/src/client_signed_create.rs `conversation_on_socket`
  (the Rust client refuses an out-of-graph wave and a different `_docID`).
* crates/database/db/src/write/prepare.rs `PreparedCreateBlocks` and
  crates/database/db/src/write/create.rs `compute` (an attempt reuses the
  prepared blocks when the schema and short ID still match; after a schema
  change it recomputes on the current schema and commits that ID).
* crates/database/db/src/merge/merge_handler/doc_identity.rs
  `resolve_composite_doc_id` (a peer derives the ID from the genesis CID),
  and the hash check on each way a block arrives, which keeps a block only
  when it hashes to its CID: crates/engine/blockstore/src/verify.rs
  `verify_block_cid`, run on a replicator push
  (crates/database/p2p/src/sync/manager/process/pushlog.rs
  `process_block_inner`, the usual way a fresh genesis reaches a peer), on
  gossip, block sync and CAR fetch; and
  crates/building-blocks/bitswap/src/message/mod.rs `verify_hash` on a
  bitswap fetch.
* crates/database/db/src/definition/migration/shortid.rs
  `resolve_legacy_identities` (a legacy ID moves to its genesis's ID and
  stays as an alias).
* crates/database/db/src/write/autocommit/helpers.rs `register_created_doc`
  (a stored ID is refused, the `get_doc_ref` branch).
* crates/database/db/src/backup/import.rs (import creates again; the file's ID
  becomes an alias).

Verify: `cd proofs/lean && lake build Cid`. No `sorry`, `admit` or `axiom`.
-/

namespace Cid.SignedGenesis

-- A content address, kept abstract as a number.
abbrev Address := Nat
-- A document ID, kept abstract as a number.
abbrev DocId := Nat
-- A signing key, named by its public key.
abbrev Key := Nat
-- The randomness a signer may mix in; a deterministic signer ignores it.
abbrev Nonce := Nat

-- One field's block: which field, its encoded value, and the address of its
-- signature block once it is signed (`none` before signing).
structure FieldBlock where
  -- The field the block writes.
  field : Nat
  -- The field's encoded value.
  value : Nat
  -- The signature block's address, or `none` while unsigned.
  sig : Option Address
  deriving DecidableEq, Repr

-- The genesis composite block: the collection version it belongs to, the
-- addresses of its field blocks, and its own signature's address.
structure Composite where
  -- The collection version ID the genesis names.
  version : Nat
  -- The field blocks' addresses, in order.
  links : List Address
  -- The composite's signature block address, or `none` while unsigned.
  sig : Option Address
  deriving DecidableEq, Repr

-- What a signer signs: an unsigned field block or an unsigned composite.
inductive Payload where
  -- A field block before its signature is set.
  | field (b : FieldBlock)
  -- A composite before its signature is set.
  | composite (c : Composite)

-- A signature block: the public key it names and the signature value.
structure SigBlock where
  -- The signer's public key (the signature block names it).
  key : Key
  -- The signature bytes, kept abstract.
  value : Nat
  deriving DecidableEq, Repr

-- Our own injectivity, so the file needs nothing beyond Lean's core.
def Injective {α β : Type} (f : α → β) : Prop :=
  -- Equal outputs only come from equal inputs.
  ∀ a b, f a = f b → a = b

-- The world the proofs live in: how blocks get addresses, how an address
-- becomes an ID, and how a key signs.
structure World where
  -- The address of a field block (DAG-CBOR then SHA-256).
  addrF : FieldBlock → Address
  -- The address of a composite block.
  addrC : Composite → Address
  -- The address of a signature block.
  addrS : SigBlock → Address
  -- `derive_doc_id`: the ID of a genesis address (UUIDv5, version 0).
  docIdOf : Address → DocId
  -- A signer: key, payload and nonce give the signature value.
  sign : Key → Payload → Nonce → Nat

-- Sign one field block: sign its unsigned form, then set the signature's
-- address. This is compute.rs setting `field_block.signature` before encoding.
def signField (w : World) (k : Key) (n : Nonce) (b : FieldBlock) : FieldBlock :=
  -- The block keeps its field and value and gains its signature's address.
  { b with sig := some (w.addrS ⟨k, w.sign k (.field { b with sig := none }) n⟩) }

-- The genesis composite a create builds: sign every field block, link their
-- addresses, then sign the composite over its unsigned form.
def genesis (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) : Composite :=
  -- The unsigned composite: the version and the signed fields' addresses.
  let unsigned : Composite :=
    { version := version, links := fields.map (fun b => w.addrF (signField w k n b)), sig := none }
  -- The signed composite: the same, with its own signature's address.
  { unsigned with sig := some (w.addrS ⟨k, w.sign k (.composite unsigned) n⟩) }

-- The ID a creator computes before anything commits: the ID of its genesis.
def precommitId (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) : DocId :=
  -- `derive_doc_id(genesis_cid)`.
  w.docIdOf (w.addrC (genesis w k n version fields))

/-! ## Backup import re-creates the document -/

-- Backup import creates the document again on the importing node, with that
-- node's key and nonce, so it builds a new genesis.
def importedId (w : World) (k' : Key) (n' : Nonce) (version : Nat)
    (fields : List FieldBlock) : DocId :=
  -- A fresh create: its own precomputed ID.
  precommitId w k' n' version fields

-- Promise: import keeps the ID only when the importing node's signature is the
-- creator's; with injective addresses, a different composite signature gives a
-- different ID. So import does not preserve identity in general.
theorem import_is_not_identity_preserving (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf) (hS : Injective w.addrS)
    (k k' : Key) (n n' : Nonce) (version : Nat) (fields : List FieldBlock)
    (hk : k ≠ k') :
    importedId w k' n' version fields ≠ precommitId w k n version fields := by
  -- Suppose the two IDs were equal.
  intro heq
  -- Equal IDs mean equal genesis addresses.
  have haddr := hId _ _ heq
  -- Equal addresses mean equal genesis blocks.
  have hblock := hC _ _ haddr
  -- Equal blocks have equal signature addresses.
  have hsig := congrArg Composite.sig hblock
  -- Unfold the two signatures.
  simp only [genesis] at hsig
  -- Equal `some` values: equal signature addresses.
  have hsa := Option.some.inj hsig
  -- Equal signature addresses mean equal signature blocks.
  have hsb := hS _ _ hsa
  -- So their keys are equal, which contradicts `k ≠ k'`.
  exact hk (congrArg SigBlock.key hsb).symm

/-! ## Both kinds of signature reach the ID -/

-- Promise: two genesis blocks that differ in the composite's signature have
-- different IDs (the composite is signed before its address is taken).
theorem composite_sig_reaches_doc_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (c₁ c₂ : Composite) (h : c₁.sig ≠ c₂.sig) :
    w.docIdOf (w.addrC c₁) ≠ w.docIdOf (w.addrC c₂) := by
  -- Suppose the IDs were equal.
  intro heq
  -- Then the blocks are equal, so their signatures are equal.
  exact h (congrArg Composite.sig (hC _ _ (hId _ _ heq)))

-- Promise: two genesis blocks whose field links differ (a field block's
-- signature changed its address) have different IDs.
theorem field_sig_reaches_doc_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (c₁ c₂ : Composite) (h : c₁.links ≠ c₂.links) :
    w.docIdOf (w.addrC c₁) ≠ w.docIdOf (w.addrC c₂) := by
  -- Suppose the IDs were equal.
  intro heq
  -- Then the blocks are equal, so their links are equal.
  exact h (congrArg Composite.links (hC _ _ (hId _ _ heq)))

-- Promise: a field signature reaches the field's address: two signed field
-- blocks with different signature blocks have different addresses.
theorem field_sig_reaches_field_address (w : World)
    (hF : Injective w.addrF) (hS : Injective w.addrS)
    (b : FieldBlock) (s₁ s₂ : SigBlock) (h : s₁ ≠ s₂) :
    w.addrF { b with sig := some (w.addrS s₁) } ≠ w.addrF { b with sig := some (w.addrS s₂) } := by
  -- Suppose the addresses were equal.
  intro heq
  -- Then the blocks are equal, so their signature addresses are equal.
  have hb := congrArg FieldBlock.sig (hF _ _ heq)
  -- Peel the `some` and the signature address.
  exact h (hS _ _ (Option.some.inj hb))

/-! ## Determinism of the signer -/

-- Promise: a signer that ignores its nonce (Ed25519, Secp256k1 with RFC 6979)
-- gives the same ID on every run.
theorem nonce_independent_signer_same_doc_id (w : World)
    (hdet : ∀ k p n n', w.sign k p n = w.sign k p n')
    (k : Key) (n n' : Nonce) (version : Nat) (fields : List FieldBlock) :
    precommitId w k n version fields = precommitId w k n' version fields := by
  -- The signed field blocks are the same under both nonces.
  have hf : ∀ b, signField w k n b = signField w k n' b := by
    -- Each field's signature value ignores the nonce.
    intro b; simp [signField, hdet k _ n n']
  -- So the genesis blocks are the same, and so the IDs.
  simp [precommitId, genesis, hf, hdet k _ n n']

-- Promise: a signer whose composite signature depends on its nonce
-- (randomized ECDSA, Secp256r1) gives two IDs for the same content.
theorem nonce_dependent_signer_can_differ (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf) (hS : Injective w.addrS)
    (k : Key) (n n' : Nonce) (version : Nat) (fields : List FieldBlock)
    (hfields : ∀ b, signField w k n b = signField w k n' b)
    (hdiff : ∀ c, w.sign k (.composite c) n ≠ w.sign k (.composite c) n') :
    precommitId w k n version fields ≠ precommitId w k n' version fields := by
  -- Suppose the two IDs were equal.
  intro heq
  -- Then the genesis blocks are equal.
  have hblock := hC _ _ (hId _ _ heq)
  -- So their signature addresses are equal.
  have hsig := congrArg Composite.sig hblock
  -- Unfold both signatures; the field links agree by `hfields`.
  simp only [genesis, hfields] at hsig
  -- Equal signature addresses mean equal signature blocks, so equal values.
  have hv := congrArg SigBlock.value (hS _ _ (Option.some.inj hsig))
  -- That contradicts the nonce changing the composite's signature.
  exact hdiff _ hv

/-! ## Each path commits the ID of the genesis it holds

Every path below derives the committed ID from a genesis block it holds at
commit time, never from the creator's answer. Each promise names the code
check that makes that block the creator's, and each RED theorem shows the ID
moves once that check is gone. -/

-- Promise: two different genesis blocks have two different IDs.
theorem different_genesis_different_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (c₁ c₂ : Composite) (h : c₁ ≠ c₂) :
    w.docIdOf (w.addrC c₁) ≠ w.docIdOf (w.addrC c₂) :=
  -- Equal IDs would give equal addresses, so equal blocks.
  fun heq => h (hC _ _ (hId _ _ heq))

-- A node-signed commit attempt (autocommit or explicit transaction).
-- `prepare` computed the blocks with nonce `n` against the schema current at
-- that time (`PreparedCreateBlocks`, write/prepare.rs). The attempt, in
-- `create::compute` (write/create.rs), reuses them when the schema and short
-- ID still match; otherwise it computes them again against the schema current
-- now, signing again with the nonce `retryNonce`. A changed schema can change
-- the collection version and the field blocks, so both are part of the attempt.
structure NodeAttempt where
  -- `create::compute` found matching prepared blocks and reused them.
  reusesPrepared : Bool
  -- The nonce a recomputation draws.
  retryNonce : Nonce
  -- The collection version ID the current schema gives a recomputation.
  currentVersion : Nat
  -- The field blocks the current schema gives a recomputation.
  currentFields : List FieldBlock

-- The attempt recomputes against the same schema the prepared blocks used:
-- the same collection version and the same field blocks.
def NodeAttempt.sameSchema (a : NodeAttempt) (version : Nat)
    (fields : List FieldBlock) : Prop :=
  -- Both schema-dependent inputs are the prepared ones.
  a.currentVersion = version ∧ a.currentFields = fields

-- The ID a node attempt commits: `persist` keys the document by the ID of the
-- blocks it inserts, and the response names that same ID.
def nodeCommitted (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) (a : NodeAttempt) : DocId :=
  -- Reused blocks keep the prepared nonce and schema; recomputed ones use the
  -- new nonce and the current schema's version and fields.
  if a.reusesPrepared then precommitId w k n version fields
  else precommitId w k a.retryNonce a.currentVersion a.currentFields

-- Promise: a node attempt commits the prepared ID when it reuses the prepared
-- blocks, or when it recomputes against the same schema with a signer that
-- ignores its nonce (Ed25519, Secp256k1). A recomputation after a schema
-- change is not covered: it commits the ID of the blocks the current schema
-- gives, which `node_recompute_after_schema_change_moves_id` shows can differ.
theorem node_commits_precomputed (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) (a : NodeAttempt)
    (h : a.reusesPrepared = true ∨
      (a.sameSchema version fields ∧ ∀ k p n n', w.sign k p n = w.sign k p n')) :
    nodeCommitted w k n version fields a = precommitId w k n version fields := by
  -- Split on whether the attempt reused the prepared blocks.
  cases hr : a.reusesPrepared with
  -- Reused: the same nonce and schema, so the same genesis.
  | true => simp [nodeCommitted, hr]
  -- Recomputed: only the same schema with a nonce-free signer keeps the ID.
  | false =>
      -- The premise must then be the same schema and a nonce-free signer.
      have hs : a.sameSchema version fields ∧ ∀ k p n n', w.sign k p n = w.sign k p n' := by
        -- The reuse branch of `h` contradicts `hr`.
        cases h with
        -- Reuse claimed, but the attempt recomputed.
        | inl hreuse => simp [hr] at hreuse
        -- The same schema and a signer that ignores its nonce.
        | inr hd => exact hd
      -- Name the two schema equalities and the signer's determinism.
      obtain ⟨⟨hv, hf⟩, hdet⟩ := hs
      -- Unfold the attempt: it signs with the retry nonce on the current schema,
      -- which is the prepared one.
      simp only [nodeCommitted, hr, hv, hf]
      -- A nonce-free signer gives the same ID under either nonce.
      exact (nonce_independent_signer_same_doc_id w hdet k n a.retryNonce version fields).symm

-- RED: an attempt that recomputes with a signer whose composite signature
-- depends on its nonce (randomized ECDSA) commits an ID other than the
-- prepared one, even on the same schema. Removing the reuse in
-- `create::compute` plants this bug for such a signer.
theorem node_recompute_with_random_signer_moves_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf) (hS : Injective w.addrS)
    (k : Key) (n : Nonce) (version : Nat) (fields : List FieldBlock) (a : NodeAttempt)
    (hr : a.reusesPrepared = false) (hsame : a.sameSchema version fields)
    (hfields : ∀ b, signField w k n b = signField w k a.retryNonce b)
    (hdiff : ∀ c, w.sign k (.composite c) n ≠ w.sign k (.composite c) a.retryNonce) :
    nodeCommitted w k n version fields a ≠ precommitId w k n version fields := by
  -- Name the two schema equalities.
  obtain ⟨hv, hf⟩ := hsame
  -- The attempt signs with the retry nonce on the prepared schema.
  simp only [nodeCommitted, hr, hv, hf]
  -- Two nonces give two composite signatures, so two IDs.
  exact fun heq => nonce_dependent_signer_can_differ w hC hId hS k n a.retryNonce
    version fields hfields hdiff heq.symm

-- Not a guard but the limit of the promise: a recomputation after the schema
-- moved to another collection version commits another ID, whatever the signer,
-- because the genesis names the version. The response and the stored document
-- both carry that new ID; only the prepared one is left behind.
theorem node_recompute_after_schema_change_moves_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (k : Key) (n : Nonce) (version : Nat) (fields : List FieldBlock) (a : NodeAttempt)
    (hr : a.reusesPrepared = false) (hv : a.currentVersion ≠ version) :
    nodeCommitted w k n version fields a ≠ precommitId w k n version fields := by
  -- Suppose the two IDs were equal.
  intro heq
  -- The attempt recomputed, so it built its genesis on the current schema.
  simp only [nodeCommitted, hr] at heq
  -- Equal IDs mean equal genesis blocks, so equal collection versions.
  have hblock := hC _ _ (hId _ _ heq)
  -- Each genesis names the version it was built on.
  have hver := congrArg Composite.version hblock
  -- Unfold both versions.
  simp only [genesis] at hver
  -- That contradicts the schema having moved.
  exact hv hver

-- A client-signed create: the node rebuilds the genesis from the client's
-- signatures (`ExactSignatures::sign_sync` refuses any other block, so the
-- rebuild may fail: `none`), then `NodeCore::execute_verified_graph` commits only
-- when the stored `_docID` equals `graph.doc_id()`.
def clientCommitted (w : World) (graphId : DocId) (rebuilt : Option Composite) :
    Option DocId :=
  -- What the node rebuilt, if anything.
  match rebuilt with
  -- The replay refused a block outside the graph: nothing commits.
  | none => none
  -- A rebuilt genesis commits only when its ID is the graph's.
  | some g => if w.docIdOf (w.addrC g) = graphId then some (w.docIdOf (w.addrC g)) else none

-- Promise: whatever a client-signed create commits is `graph.doc_id()`,
-- whatever block the node rebuilt.
theorem client_signed_commits_precomputed (w : World) (k : Key) (n : Nonce)
    (version : Nat) (fields : List FieldBlock) (rebuilt : Option Composite) (id : DocId)
    (h : clientCommitted w (precommitId w k n version fields) rebuilt = some id) :
    id = precommitId w k n version fields := by
  -- Look at what the node rebuilt.
  cases rebuilt with
  -- Nothing rebuilt: nothing committed, so this case cannot arise.
  | none => simp [clientCommitted] at h
  -- A block: the commit happened only on the equal-ID branch.
  | some g =>
      -- Unfold the rule.
      simp only [clientCommitted] at h
      -- Split on the ID check.
      split at h
      -- The check passed: the committed ID is the graph's.
      · rename_i heq; simp at h; rw [← h, heq]
      -- The check failed: nothing committed.
      · simp at h

-- RED: the client-signed rule without its ID check commits whatever was
-- rebuilt.
def clientUnchecked (w : World) (rebuilt : Option Composite) : Option DocId :=
  -- Commit the ID of the rebuilt block, compared with nothing.
  rebuilt.map (fun g => w.docIdOf (w.addrC g))

-- Promise (of the bug): a node that rebuilt a genesis other than the client's
-- (for example, one it re-signed) commits an ID other than `graph.doc_id()`.
theorem client_unchecked_commits_another_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (k : Key) (n : Nonce) (version : Nat) (fields : List FieldBlock) (g : Composite)
    (hg : g ≠ genesis w k n version fields) :
    clientUnchecked w (some g) ≠ some (precommitId w k n version fields) := by
  -- Suppose the planted rule committed the graph's ID.
  intro heq
  -- Peel the `some`: two different blocks would share an ID.
  exact different_genesis_different_id w hC hId _ _ hg (Option.some.inj heq)

-- A peer merge: a block reaches the peer under a CID `asked`, either one the
-- sender announced (a replicator push, gossip) or one the peer requested
-- (block sync, CAR fetch, bitswap). Both arrivals keep the block only when its
-- bytes hash to that CID: blockstore verify.rs `verify_block_cid` (called from
-- pushlog.rs `process_block_inner` on a push) and bitswap message/mod.rs
-- `verify_hash`. `resolve_composite_doc_id` (merge_handler/doc_identity.rs)
-- then derives the ID from that CID. This rule covers every arrival path.
def peerCommitted (w : World) (asked : Address) (received : Composite) : Option DocId :=
  -- A block that does not hash to the CID asked for is dropped.
  if w.addrC received = asked then some (w.docIdOf (w.addrC received)) else none

-- Promise: whatever a peer merges for the creator's head is stored under the
-- creator's ID.
theorem peer_commits_creators_id (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) (received : Composite) (id : DocId)
    (h : peerCommitted w (w.addrC (genesis w k n version fields)) received = some id) :
    id = precommitId w k n version fields := by
  -- Unfold the rule.
  simp only [peerCommitted] at h
  -- Split on the hash check.
  split at h
  -- The hash matched: the ID is the creator's.
  · rename_i heq; simp at h; rw [← h, heq]; rfl
  -- It did not: nothing merged.
  · simp at h

-- RED: a peer that skips the hash check and derives the ID from whatever block
-- arrived stores another ID when that block is not the creator's.
theorem peer_without_hash_check_moves_id (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (k : Key) (n : Nonce) (version : Nat) (fields : List FieldBlock) (received : Composite)
    (hr : received ≠ genesis w k n version fields) :
    w.docIdOf (w.addrC received) ≠ precommitId w k n version fields :=
  -- Two different blocks, two different IDs.
  different_genesis_different_id w hC hId _ _ hr

-- The store-upgrade re-key (definition/migration/shortid.rs
-- `resolve_legacy_identities`, run by `initialize_migrations` on open): a
-- legacy document stored under `old` is mapped to the ID of its genesis, and
-- `old` is kept as an alias when the two differ. The result is the primary ID
-- and the alias, if any.
def rekey (w : World) (old : DocId) (g : Composite) : DocId × Option DocId :=
  -- `canonical_doc_id` comes from the genesis in the block graph, and
  -- `set_doc_id_alias` runs only when it differs from the old ID.
  (w.docIdOf (w.addrC g), if old = w.docIdOf (w.addrC g) then none else some old)

-- Promise: a document whose stored ID is its genesis's ID keeps it, with no alias.
theorem rekey_keeps_a_genesis_id (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) :
    rekey w (precommitId w k n version fields) (genesis w k n version fields) =
      (precommitId w k n version fields, none) := by
  -- The old and canonical IDs are the same, so no alias is written.
  simp [rekey, precommitId]

-- Promise: a legacy document whose stored ID is not its genesis's ID gets a
-- new primary ID, and its old one becomes an alias. This is the second path,
-- beside backup import, that changes a stored document's ID.
theorem rekey_moves_a_legacy_id (w : World) (old : DocId) (g : Composite)
    (h : old ≠ w.docIdOf (w.addrC g)) :
    (rekey w old g).1 ≠ old ∧ (rekey w old g).2 = some old :=
  -- The primary is the canonical ID, and the alias branch is taken.
  ⟨fun heq => h heq.symm, by simp [rekey, h]⟩

/-! ## A stored ID is refused -/

-- What the store holds under an ID: nothing, a live document, or a deleted one.
inductive Slot where
  -- No document has this ID.
  | absent
  -- A live document has this ID.
  | live
  -- A deleted document has this ID.
  | deleted
  deriving DecidableEq, Repr

-- Why a create is refused, with the ID it names (Go's two texts).
inductive Refusal where
  -- `a document with the given ID already exists. DocID: <id>`.
  | alreadyExists (id : DocId)
  -- `a document with the given ID has been deleted. DocID: <id>`.
  | deleted (id : DocId)
  deriving DecidableEq, Repr

-- `register_created_doc`'s `get_doc_ref` branch: a stored ID is refused,
-- deleted or not; an absent one is registered as live.
def register (store : DocId → Slot) (id : DocId) : Except Refusal (DocId → Slot) :=
  -- Look the create's ID up.
  match store id with
  -- Not stored: register it as live, leaving every other ID alone.
  | .absent => .ok (fun x => if x = id then .live else store x)
  -- Stored and live: refused as existing.
  | .live => .error (.alreadyExists id)
  -- Stored and deleted: refused as deleted; it stays deleted.
  | .deleted => .error (.deleted id)

-- Promise: a create whose ID is stored is refused with its own ID, and writes
-- nothing; the refusal says which state the ID was in.
theorem collision_refused (store : DocId → Slot) (id : DocId) (h : store id ≠ .absent) :
    register store id = .error (.alreadyExists id) ∧ store id = .live ∨
    register store id = .error (.deleted id) ∧ store id = .deleted := by
  -- Look at what the store holds under the ID.
  cases hs : store id with
  -- Absent contradicts the premise.
  | absent => exact absurd hs h
  -- Live: refused as existing.
  | live => left; simp [register, hs]
  -- Deleted: refused as deleted.
  | deleted => right; simp [register, hs]

/-! ## RED: signing the genesis after its address is taken -/

-- The bug compute.rs avoids: take the composite's address from its unsigned
-- form, then set the signature. The signature no longer reaches the ID.
def buggyId (w : World) (k : Key) (n : Nonce) (version : Nat)
    (fields : List FieldBlock) : DocId :=
  -- The address is taken before the signature is set.
  w.docIdOf (w.addrC { version := version,
                       links := fields.map (fun b => w.addrF (signField w k n b)),
                       sig := none })

-- Promise (of the bug): two signers that sign their field blocks alike but
-- their composites differently still get one ID, so the ID no longer names
-- who signed the genesis. `composite_sig_reaches_doc_id` is what this breaks.
theorem resign_after_encode_breaks_doc_id (w : World) (k₁ k₂ : Key) (n₁ n₂ : Nonce)
    (version : Nat) (fields : List FieldBlock)
    (hfields : ∀ b, signField w k₁ n₁ b = signField w k₂ n₂ b) :
    buggyId w k₁ n₁ version fields = buggyId w k₂ n₂ version fields := by
  -- The unsigned composites agree, and nothing else feeds the buggy ID.
  simp [buggyId, hfields]

-- Promise: under injective addresses, the correct builder tells those same two
-- signers apart whenever their composite signatures differ.
theorem correct_builder_tells_signers_apart (w : World)
    (hC : Injective w.addrC) (hId : Injective w.docIdOf)
    (k₁ k₂ : Key) (n₁ n₂ : Nonce) (version : Nat) (fields : List FieldBlock)
    (h : (genesis w k₁ n₁ version fields).sig ≠ (genesis w k₂ n₂ version fields).sig) :
    precommitId w k₁ n₁ version fields ≠ precommitId w k₂ n₂ version fields :=
  -- The composite's signature reaches the ID.
  composite_sig_reaches_doc_id w hC hId _ _ h

end Cid.SignedGenesis
