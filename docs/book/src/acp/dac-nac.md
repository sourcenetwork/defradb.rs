# DAC and NAC

## DAC: document access control

The `DocumentACP` trait ([`crates/acp/src/dac.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/acp/src/dac.rs)) is the document-scope interface:
register a document's owner, check access, add and remove relationships. Implementations:

- **Zanzibar-backed local ACP** (the default): tuples in the local store, evaluated by
  the engine above.
- **External ACP** (Vera): decisions from an external chain, wrapped so the database
  still gates every read (next chapter).
- An in-memory variant for tests.

The contract every implementation honours:

- **Registration at create**: the document's owner relationship is written *in the
  create transaction*. Between the document's rows and its owner tuple there is no
  commit boundary, hence no window where the document exists but is public.
- **Unregistration at delete**, in the delete transaction.
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
permission map lives in [`crates/http/src/route_permissions.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/http/src/route_permissions.rs). Some 51 node
permissions exist, all admin-only (`owner + admin`).

Two properties worth knowing:

- **While temporarily disabled, all writes are blocked** (including admin changes):
  a disabled NAC that still accepted admin writes would let an attacker re-enable it
  with themselves as admin.
- **`bypass-dac`**: a privilege on the NAC object that lets an authenticated identity
  skip per-document DAC checks (service accounts, indexers). It is checked first on
  every read path, and it exists only when NAC is enabled.

## Where identities come from

Callers identify with decentralized identities (DID-key over Ed25519, secp256k1 or
secp256r1 keys; `crates/identity`), presented as JWT bearer tokens (Go-`jwx`-compatible
JOSE, with bounded clock skew). Anonymous requests map to the wildcard subject: whatever
the policy grants `*`. Identity travels on the request context, never in a thread-local.

> **Why two scopes.** The questions are different: "who may read this document" is data
> (it belongs with the document, replicates with it, and is answered by its owner's
> policy), while "who may reconfigure this node" is infrastructure (it does not
> replicate, and it must keep working when documents are being replicated in). Splitting
> them keeps each policy small, each check local, and neither a choke point for the
> other.
