# DAC and NAC

## DAC: document access control

The `DocumentACP` trait ([`crates/engine/acp/src/dac.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/engine/acp/src/dac.rs)) is the document-scope interface:
register a document's owner, check access, add and remove relationships. Implementations:

- **Zanzibar-backed local ACP** (`ZanzibarDocumentACP`, `--document-acp-type local`):
  tuples in the local store, evaluated by the engine above. `defradb start` runs with
  `none`, no document ACP, unless told otherwise (`AcpDocumentType::None`).
- **External ACP** (Vera): decisions from a Vera deployment, wrapped in
  `ExternalDocumentAcp` so the database still gates every read ([Vera](vera.md)).
- An in-memory variant for tests (`LocalDocumentACP` over `MemoryAcpStore`).

The contract every implementation honours:

- **Registration at create**: a document created with an identity has its owner
  relationship written *in the create transaction*. Between the document's rows and its
  owner tuple there is no commit boundary, hence no window where the document exists but
  is public. A document created without an identity is public
  (`go.acp.dac.ownership`).
- **Unregistration at delete**, in the delete transaction, for the local ACP. Anyone the
  policy lets delete may delete, not only the owner (`go.acp.dac.relationship.deleter`).
  On Vera the delete archives the object instead ([Vera](vera.md#replicated-documents)).
  Keeping a deleted document's object under the local ACP too is proposed in
  [#3399](https://github.com/sourcenetwork/defradb.rs/pull/3399).
- **Exactly one owner**: registering a document that has one fails with
  `DocumentAlreadyRegistered`; the `owner` relation itself can neither be added nor
  removed by sharing.
- **Read checks are unions**: `read` is answered by any permission that implies read,
  matching the Go implementation's `ImplyDocumentReadPerm`.

Sharing is `add_relationship` (grant), revocation is `delete_relationship`; the
requestor must hold the relation's managing permission. DAC decisions run per document
inside the query's permission filter node, per mutation, per `_commits` row, and per
arrival entry: anything that reveals a document's data goes through the same check.

## NAC: node access control

NAC is one policy over a single singleton object (the node), always local (no Vera
option), with a lifecycle: not configured -> enabled -> (optionally) temporarily
disabled -> purged. It gates administrative surface: collection and index management,
backup, P2P configuration, lens, views, and every management HTTP route (the route
permission map lives in [`crates/surfaces/http/src/route_permissions.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/surfaces/http/src/route_permissions.rs)). 51 node
permissions exist (`NodePermission::all`), each held by the owner and every admin and
grantable on its own (`this + owner + admin`, `crates/engine/acp/src/nac/policy.rs`). The
node's own identity passes every check.

Two properties worth knowing:

- **While temporarily disabled, nothing is gated but NAC itself is frozen**: every node
  operation is allowed, as when NAC is not configured, but adding or removing NAC
  relationships is refused, even for the node's own identity (`operation requires ACP,
  but ACP not available`, `go.acp.nac.relationship.add`, `go.acp.nac.relationship.delete`),
  and re-enabling still needs `re-enable-nac` (`go.acp.nac.lifecycle.disable`,
  `go.acp.nac.lifecycle.re-enable`). A disabled NAC that still accepted relationship
  changes would let anyone make themselves admin before it is re-enabled.
- **`bypass-dac`**: a privilege on the NAC object that lets an authenticated identity
  skip per-document DAC checks (service accounts, indexers). It is checked first on
  every read path, and it exists only when NAC is enabled.

## Where identities come from

Callers identify with decentralized identities (DID-key over Ed25519, secp256k1 or
secp256r1 keys; `crates/building-blocks/identity`), presented as JWT bearer tokens (Go-`jwx`-compatible
JOSE, with bounded clock skew). Anonymous requests map to the wildcard subject: whatever
the policy grants `*`. Identity travels on the request context, never in a thread-local.

> **Why two scopes.** The questions are different: "who may read this document" is data
> (it belongs with the document, replicates with it, and is answered by its owner's
> policy), while "who may reconfigure this node" is infrastructure (it does not
> replicate, and it must keep working when documents are being replicated in). Splitting
> them keeps each policy small, each check local, and neither a choke point for the
> other.
