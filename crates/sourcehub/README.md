# Vera ACP provider

Build the Defra CLI with `--features sourcehub` and select
`--document-acp-type hub-rs`. Configure the service endpoint with
`DEFRA_HUB_RS_ADDRESS`, and supply both `--vera-consensus-key` (hex) and
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
permission, and access-decision reads verify native evidence. Object-owner
lookup still uses the legacy read endpoint.

Receipt recovery is distinct from caller-level operation idempotency. Retrying
a completed operation can issue a new request, including creating another
policy. Replicas sharing an actor identity retain independent worker identities
and submission sequences.
