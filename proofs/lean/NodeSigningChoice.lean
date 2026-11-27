/-!
The story: a node may carry an identity, and it may sign the blocks it writes.
Go keeps the two apart: a node always has a `NodeIdentity`, and `EnableSigning`
alone decides whether its blocks are signed; a request runs as its caller.
A defradb.rs node used to sign whenever it had an identity, and ran every
request without an identity of its own as that identity. `with_identity_only`
keeps the identity (for P2P, the KMS and NAC) and turns both off.

Who acts: one request the node runs that names no identity of its own (one
that names its own keeps it, which this model leaves out). What it ends with:
run as its caller and unsigned, run as the node and signed, or refused for a
missing signer.

What must never happen: an identity-only node signs a block or makes a
request act as itself; a node built without `with_identity_only` changes how
it runs a request (the default build is unchanged); or an identity-only node
is refused for a signer it does not need.

The second part follows a transaction the node begins. The node writes down
an owner for it, and every request run in it must arrive as that same owner,
or the database answers that the transaction does not exist. What must never
happen: a request the node runs in a transaction it began, signed or not,
arrives as someone other than the transaction's owner. Before this fix an
identity-only node owned its transactions as its DID but ran unsigned
requests in them as no one, so they were all refused.

Rust owner: crates/entry-points/defra-node/src/lib.rs `Node::execute_prepared_request`
(the `signing_identity` filter), `Node::node_principal` and
`Node::node_context` (the owner of the transactions the node begins), and
crates/entry-points/defra-node/src/assemble.rs `assemble` (`node_query_identity`, the
identity a request without its own acts as). The signed path stamps the
principal in crates/entry-points/defra-node/src/signed_query/mod.rs
`execute_with_signing_context`. The HTTP server takes the same choice through
`ServerConfig::no_signing` (crates/entry-points/defra-node/src/node_builder.rs
`NodeBuilder::build`).
-/

namespace NodeSigningChoice

-- How a node is built: whether it has an identity, whether it was built
-- identity-only, and whether its identity's signer can be found.
structure Node where
  -- The node has a DID (`with_signing_key`, `enable_signing` or `with_node_identity_did`).
  hasIdentity : Bool
  -- The builder asked for an identity that does not sign (`with_identity_only`).
  identityOnly : Bool
  -- The signing registry holds a usable signer for that DID.
  signerFound : Bool

-- What one request without an identity of its own ends with.
inductive Outcome where
  -- The request runs as its caller, and its blocks are unsigned.
  | asCaller
  -- The request acts as the node's DID, and its blocks are signed with it.
  | asNodeSigned
  -- `ScopedNode::execute` signs with the node's key but leaves the request
  -- acting as its caller.
  | signedAsCaller
  -- The node should sign but its signer is missing: the request is refused.
  | refused
  deriving DecidableEq

-- The node's choice, line by line as `execute_prepared_request` makes it.
def run (n : Node) : Outcome :=
  -- No identity, or an identity that does not sign: run as the caller.
  if !n.hasIdentity || n.identityOnly then .asCaller
  -- A signing node needs its signer; with it the request acts as the node.
  else if n.signerFound then .asNodeSigned
  -- Without the signer the request is refused.
  else .refused

-- What the node did before `with_identity_only` existed: it acted as and
-- signed with its identity whenever it had one, and refused without a signer.
def before (hasIdentity signerFound : Bool) : Outcome :=
  -- No identity: run as the caller, unsigned.
  if !hasIdentity then .asCaller
  -- An identity: act as the node and sign when the signer is there, else refuse.
  else if signerFound then .asNodeSigned
  else .refused

-- A node built without `with_identity_only` runs every request without an
-- identity of its own exactly as before.
theorem default_build_unchanged (hasIdentity signerFound : Bool) :
    run ⟨hasIdentity, false, signerFound⟩ = before hasIdentity signerFound := by
  -- Every combination of the two flags gives the same outcome.
  cases hasIdentity <;> cases signerFound <;> rfl

-- An identity-only node runs every request as its caller: never signed, never as itself.
theorem identity_only_runs_as_caller (n : Node) (h : n.identityOnly = true) :
    run n = .asCaller := by
  -- The first condition holds because the node is identity-only.
  unfold run
  simp [h]

-- So an identity-only node never signs a block.
theorem identity_only_never_signs (n : Node) (h : n.identityOnly = true) :
    run n ≠ .asNodeSigned := by
  -- Its outcome is `asCaller`, which is not the signed one.
  rw [identity_only_runs_as_caller n h]
  decide

-- And an identity-only node is never refused for a signer it does not need.
theorem identity_only_never_refused (n : Node) (h : n.identityOnly = true) :
    run n ≠ .refused := by
  -- Its outcome is `asCaller`, which is not a refusal.
  rw [identity_only_runs_as_caller n h]
  decide

-- One caller's acting identity and DAC bypass. The caller hands both to the
-- request and expects both back unchanged.
structure Caller where
  -- The caller has an acting DID.
  identityKnown : Bool
  -- The caller was granted a DAC bypass.
  dacBypass : Bool
  deriving DecidableEq

-- What `ScopedNode::execute` does for a request that carries no signing config
-- of its own. A node without an identity, or one told not to sign, leaves the
-- request unsigned. A signing node signs with its node key. Without that key
-- it refuses; it never falls back to an unsigned write.
def runAs (n : Node) : Outcome :=
  -- A node with no signer identity, or one told not to sign, stays unsigned.
  if !n.hasIdentity || n.identityOnly then .asCaller
  -- A signing node signs with the node key, without becoming the caller.
  else if n.signerFound then .signedAsCaller
  -- It cannot write unsigned just because its signer went missing.
  else .refused

-- `execute_as` returns its outcome and the caller's context unchanged.
def executeAs (n : Node) (caller : Caller) : Outcome × Caller :=
  (runAs n, caller)

-- Whether the signed path preserved the caller's identity and bypass.
theorem execute_as_keeps_the_caller (n : Node) (caller : Caller) :
    (executeAs n caller).2 = caller := by
  -- The second element is the caller itself, whatever the node chooses.
  rfl

-- A signing node's `execute_as` signs with the node key but does not make the
-- request act as the node.
theorem execute_as_signs_as_the_caller (n : Node)
    (hIdentity : n.hasIdentity = true) (hNotOnly : n.identityOnly = false)
    (hSigner : n.signerFound = true) :
    runAs n = .signedAsCaller := by
  -- Substitute the three node facts and compute the branch.
  simp [runAs, hIdentity, hNotOnly, hSigner]

-- The node key's signature is distinguishable from running as the node.
theorem execute_as_does_not_act_as_the_node (n : Node)
    (hIdentity : n.hasIdentity = true) (hNotOnly : n.identityOnly = false)
    (hSigner : n.signerFound = true) :
    runAs n ≠ .asNodeSigned := by
  -- Its signed outcome is the caller-preserving one.
  rw [execute_as_signs_as_the_caller n hIdentity hNotOnly hSigner]
  decide

-- A node told not to sign, or one with no signing identity, stays unsigned.
theorem execute_as_unsigned_without_signing (n : Node)
    (h : (!n.hasIdentity || n.identityOnly) = true) :
    runAs n = .asCaller := by
  -- The unsigned guard is exactly the caller's reason.
  simp [runAs, h]

-- A configured signer that cannot be found fails closed.
theorem execute_as_refuses_a_missing_signer (n : Node)
    (hIdentity : n.hasIdentity = true) (hNotOnly : n.identityOnly = false)
    (hMissing : n.signerFound = false) :
    runAs n = .refused := by
  -- Substitute the node's facts and compute the final branch.
  simp [runAs, hIdentity, hNotOnly, hMissing]

-- Who a request, or a transaction's owner, is when the caller names no one.
inductive Principal where
  -- The node's own DID.
  | node
  -- Nobody: the request context carries no identity.
  | noOne
  deriving DecidableEq

-- How the node sends one request to the database.
inductive Path where
  -- No key signs it, and its context is kept as the caller gave it.
  | unsigned
  -- A key signs it (the node's, or a caller's `EmbeddedSigner`).
  | signed
  -- A caller brought a key to a node with no identity: refused, never run.
  | refusedNoIdentity
  deriving DecidableEq

-- The node's principal, `Node::node_principal`: its DID when it acts as its
-- identity, nobody on an identity-only node or a node without an identity.
def principal (n : Node) : Principal :=
  -- Only a node with an identity that is not identity-only acts as itself.
  if n.hasIdentity && !n.identityOnly then .node else .noOne

-- The owner of a transaction the node begins: `Node::new_txn` and
-- `txn_owner` begin it in `node_context`, whose identity is the principal.
def txnOwner (n : Node) : Principal :=
  -- The node's principal, nothing more.
  principal n

-- The owner the node gave its transactions before this fix: its DID whenever
-- it had one, identity-only or not.
def txnOwnerBefore (n : Node) : Principal :=
  -- Any identity made the node the owner.
  if n.hasIdentity then .node else .noOne

-- The path `execute_prepared_request` picks, given whether the caller
-- attached a key of its own.
def pathOf (n : Node) (callerKey : Bool) : Path :=
  -- A node with an identity signs when it signs itself or a caller brings a key.
  if n.hasIdentity && (!n.identityOnly || callerKey) then .signed
  -- A caller's key on a node with no identity is refused.
  else if callerKey then .refusedNoIdentity
  -- Otherwise the request runs unsigned.
  else .unsigned

-- The principal a request that names no identity arrives in the transaction
-- as, along `p`; nothing when it never runs.
def execPrincipal (n : Node) : Path → Option Principal
  -- Unsigned: its context is kept, and it carries no identity.
  | .unsigned => some .noOne
  -- Signed: `execute_with_signing_context` stamps the node's principal, and
  -- stamps nothing (leaving no one) when that principal is nobody.
  | .signed => some (principal n)
  -- Refused: it never reaches the transaction.
  | .refusedNoIdentity => none

-- Whether a request run along the node's path arrives as the owner `owner`
-- gives the transaction: true when it does, or when it never runs.
def agreesWith (owner : Node → Principal) (n : Node) (callerKey : Bool) : Bool :=
  -- Look at who the request arrives as.
  match execPrincipal n (pathOf n callerKey) with
  -- A refused request meets no transaction, so nothing can disagree.
  | none => true
  -- A request that runs must arrive as the transaction's owner.
  | some p => decide (p = owner n)

-- Every request the node runs in a transaction it began arrives as that
-- transaction's owner, on every node, signed or not.
theorem begin_and_execute_agree (n : Node) (callerKey : Bool) :
    agreesWith txnOwner n callerKey = true := by
  -- Take the node apart into its three flags.
  cases n with
  | mk hasIdentity identityOnly signerFound =>
    -- Each of the sixteen combinations is checked by computing it.
    cases hasIdentity <;> cases identityOnly <;> cases signerFound <;> cases callerKey <;> decide

-- The planted bug: with the old owner, an identity-only node's unsigned
-- request arrives as no one in a transaction owned by the node, so it fails.
theorem owner_before_disagreed :
    agreesWith txnOwnerBefore ⟨true, true, true⟩ false = false := by
  -- Computing that one case shows the disagreement.
  decide

-- A node built without `with_identity_only` owns its transactions as before.
theorem default_owner_unchanged (hasIdentity signerFound : Bool) :
    txnOwner ⟨hasIdentity, false, signerFound⟩ = txnOwnerBefore ⟨hasIdentity, false, signerFound⟩ := by
  -- Every combination of the two flags gives the same owner.
  cases hasIdentity <;> cases signerFound <;> decide

end NodeSigningChoice
