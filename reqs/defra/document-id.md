# `defra.document.id`: A document's ID is known before it commits

- **Parents:** ["defra.document"]

A document's ID is `bae-` and a UUIDv5 of the CID of its genesis composite block. That block carries the creator's signature when the create is signed, and its links are the CIDs of the field blocks, each signed in turn. So whoever builds or verifies a signed create knows its ID before it commits, and no path gives the committed document a different one. There are two exceptions. Backup import is one, below. The other is the store upgrade that runs on open: it maps a legacy document whose stored ID is not its genesis's ID to the genesis's ID, and keeps the old ID as an alias. A document created under the current derivation already has its genesis's ID, so the upgrade leaves it alone.

# `defra.document.id.node_signed`: A node-signed create commits the ID it returned

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["txn::signed_doc_id::a_node_signed_create_in_a_txn_commits_the_id_it_returned_ed25519", "txn::signed_doc_id::a_node_signed_create_in_a_txn_commits_the_id_it_returned_secp256k1", "txn::signed_doc_id::an_autocommit_node_signed_create_stores_the_id_it_returned"]

On a node started with signing on, a create made inside an explicit transaction returns a `_docID` before the commit. After the commit, a query in a new request returns the document under that same `_docID`. A create made in autocommit is stored under the `_docID` its response names. This holds through every client.

# `defra.document.id.client_signed`: A client-signed create commits the precomputed ID or is refused

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["signature::client_signed::a_client_signed_create_commits_the_precomputed_id", "signature::client_signed::a_client_signed_create_is_queryable_under_the_precomputed_id"]

A client that signs a create itself, over the graph it built from its own trusted schema and document (`ClientCreateSignatures`), knows the document's ID before it sends anything: `graph.doc_id()`. Embedded (`VerifiedClientCreate` staged with `execute_verified_create_in_txn`) and over rust-http (`client_signed_create::execute`), the create either commits a document whose `_docID` is `graph.doc_id()` or is refused; it never commits another ID. Over rust-http, the guarantee is the client's: it refuses a signing wave outside its approved graph (`server requested a block outside the approved graph`) and a success that names a different ID (`server returned a different document ID`). The server's websocket signer verifies each signature against the session key and compares no ID. go-http and cli have no client-signed create.

# `defra.document.id.replicated`: A peer shows the creator's ID

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["signature::doc_id_determinism::a_peer_shows_the_creators_doc_id", "signature::peer::doc_signature_with_peers_an_different_key_types_should_sync"]

A signed document replicated to a peer is stored there under the `_docID` the creator's node returned, whichever key type signed it: the peer derives it from the genesis block it received.

# `defra.document.id.signature_determinism`: Same key and content give the same ID

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["signature::doc_id_determinism::the_same_key_and_content_give_the_same_doc_id_ed25519", "signature::doc_id_determinism::the_same_key_and_content_give_the_same_doc_id_secp256k1", "signature::doc_id_determinism::a_different_key_gives_a_different_doc_id"]

Ed25519 and Secp256k1 (RFC 6979) signatures are deterministic: two nodes with the same signing key that create the same document in the same collection return the same `_docID`. A different key gives a different `_docID`, because the signature block names its public key. Secp256r1 signs only remotely or in the Secure Enclave, with randomized ECDSA, and BLS through Orbis is not tested; neither is covered here.

# `defra.document.id.collision`: A create whose ID is stored is refused with Go's text

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["mutation::add::signed_duplicate::a_secp256k1_signed_duplicate_create_is_refused_with_go_text", "mutation::add::signed_duplicate::a_client_signed_duplicate_create_is_refused"]

A signed create whose genesis block gives an ID that is already stored is refused with `a document with the given ID already exists. DocID: <id>`, and nothing is written. When the stored document has been deleted, it is refused with `a document with the given ID has been deleted. DocID: <id>`, and the document stays deleted. A client-signed create of a document that is already stored is refused the same way.

# `defra.document.id.collision_code`: The collision refusals carry stable codes

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["mutation::add::signed_duplicate::a_node_signed_duplicate_create_is_refused_with_its_code", "mutation::add::signed_duplicate::a_request_duplicate_create_is_refused_with_its_code", "mutation::add::signed_duplicate::a_deleted_document_created_again_is_refused_as_deleted", "mutation::add::signed_duplicate::an_upsert_on_a_deleted_document_is_refused_as_deleted"]

The two refusals of `defra.document.id.collision` carry a code beside Go's text: `DOCUMENT_ALREADY_EXISTS` for `a document with the given ID already exists. DocID: <id>`, and `DOCUMENT_DELETED` for `a document with the given ID has been deleted. DocID: <id>`. In-process, the GraphQL error's `extensions.code` and the documents API's error code carry it; over rust-http, the GraphQL error's `extensions.code` and the REST error body's `code` field (`{"error": "<text>", "code": "<code>"}`). The REST status is 409 for an existing document and 400 for a deleted one. The text is the same through all four clients; cli prints the text alone, as Go's CLI does, and a Go node sends no code.

# `defra.document.id.import_alias`: Backup import re-creates documents under new IDs

- **Parents:** ["defra.document.id"]
- **Scenarios:** ["backup::simple::import_alias::an_imported_document_gets_a_new_id_and_its_file_id_resolves_to_it"]

An exception to `defra.document.id`, and a stopgap until an ID-preserving migration replaces it. Importing a backup creates each document again, as a new create on the importing node, so it gets the ID of its new genesis block. That ID is new unless the importing node rebuilds the same genesis: the same signing key with a signer that ignores its nonce (or no signing at all), the same collection version and the same field values give the file's `_docID` back. A different key, a nonce-dependent signer, another collection version or other values give a new ID. The `_docID` the file names, when it differs, is kept as an alias: a query filtered on it returns the imported document, which reports its new `_docID`.
