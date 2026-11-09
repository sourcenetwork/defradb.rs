/-!
The story: a node may carry an identity, and it may sign the blocks it writes.
Go keeps the two apart: a node always has a `NodeIdentity`, and `EnableSigning`
alone decides whether its blocks are signed; a request runs as its caller.
A defradb.rs node used to sign whenever it had an identity, and ran every
request without an identity of its own as that identity. `with_identity_only`
keeps the identity (for P2P, the KMS and NAC) and turns both off.

Who acts: one request the node runs. What it ends with: run as its caller
and unsigned, run as the node and signed, or refused for a missing signer.

What must never happen: an identity-only node signs a block or makes a
request act as itself; a node built without `with_identity_only` changes how
it runs a request (the default build is unchanged); or an identity-only node
is refused for a signer it does not need.

Rust owner: crates/defra-node/src/lib.rs `Node::execute_prepared_request`
(the `signing_identity` filter) and crates/defra-node/src/assemble.rs
`assemble` (`node_query_identity`, the identity a request without its own
acts as). The HTTP server takes the same choice through
`ServerConfig::no_signing` (crates/defra-node/src/node_builder.rs
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

-- What one request the node runs ends with.
inductive Outcome where
  -- The request runs as its caller, and its blocks are unsigned.
  | asCaller
  -- The request acts as the node's DID, and its blocks are signed with it.
  | asNodeSigned
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

-- A node built without `with_identity_only` runs every request exactly as before.
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

end NodeSigningChoice
