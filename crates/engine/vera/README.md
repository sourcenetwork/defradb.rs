# Vera ACP provider

Build the Defra CLI with `--features vera` and select
`--document-acp-type verars`. Configure the service endpoint with
`DEFRA_VERA_RS_ADDRESS`, and supply both `--vera-consensus-key` (hex) and
`--vera-deployment-id` from the deployment's operator configuration. Their
corresponding environment variables are `DEFRA_VERA_CONSENSUS_KEY` and
`DEFRA_VERA_DEPLOYMENT_ID`.

The node's secp256k1 identity owns its policies. Each Defra data directory has
an independent native submission worker whose private key is stored in the
configured keyring. An enabled, persistent keyring is required. The worker
journal lives in `<rootdir>/vera-worker`; preserve it together with the keyring
when moving or restoring a node, and run only one process per worker directory.

Writes use native signed requests. Each worker retains one unresolved request,
retries its exact signed bytes when the submission outcome is unknown, and
advances its sequence only after verifying a finalized receipt against the
configured consensus key. Startup resolves a pending request before accepting
new writes. Confirmation timeouts retain that request. Do not delete the journal
to bypass a timeout: its sequence and signing identity belong together.

Policy creation returns the ID from the verified creation event and checks the
policy record's actor, worker, submission ID, and creation revision. Policy,
permission, access-decision and object-owner reads verify native evidence.
Owner lookup proves the complete set of owner relations; missing or archived
objects have no live owner. Unavailable evidence remains an error.
Policy lookup accepts plain or `0x`-prefixed IDs and returns the canonical ID,
certified name and original definition. A malformed stored record is an error,
not absence. Definitions retain actor roles, descriptions, metadata and Defra
specification rules when reconstructed through the document ACP provider.

Deleting a document archives its object on the deployment: the delete's
transaction records the archive as an owed change, settled after the commit in
order behind anything still in flight on the object, and Vera then removes every
relationship on it and names no owner. Only the object's owner may archive, so a
delete by anyone else (a writer the policy lets delete) is refused before it
commits and nothing is deleted. Changes on the object not yet sent go with the
delete, each with a note for its actor; from the commit until the archive
settles, the object denies everyone, and only the owner that deleted the
document still reads its commit history. A node that merges a replicated delete
archives nothing. Vera keeps an archived registration: the same document
identifier cannot be registered again, so a document re-created with identical
content stays readable by nobody.

Receipt recovery is distinct from caller-level operation idempotency. Retrying
a completed operation can issue a new request. vera.rs derives a policy's ID
from its definition, its creator (the node's actor) and a 32-byte nonce, so a
caller that supplies the nonce (`VeraProvider::create_policy`,
`Node::add_dac_policy_with_nonce_as`, `?nonce=` on the add-policy route,
`policy add --nonce`) knows the ID beforehand (`vera_rs::predict_policy_id`):
a create sent again with that nonce is refused as already existing, and the
provider returns the existing policy when its definition, owner and nonce are
the ones asked for, so no second policy is made. The ID comes from the node's
actor, so a nonce from a caller that is not that actor is refused. Without a nonce the provider
draws one per call, and a retried create makes another policy. Replicas sharing
an actor identity retain independent worker identities and submission
sequences.

The native provider module is `vera::vera_rs`, and its integration suite is
`cargo test -p integration-test --test verars`. Configure it with the `verars`
provider selector, `--vera-rs-address`, `DEFRA_VERA_RS_ADDRESS`, or the
`vera_rs_address` config key.

The `Native consumer` workflow tests the provider against its pinned Vera runtime.
Manual runs accept a full commit SHA in `vera_revision`; this selects the service
without changing the SDK dependencies. The driver checks the runtime checkout,
all seven SDK declarations and staged binary hashes. Its summary records both
source revisions, SDK revision and results for document access, revocation,
replication and restart.
