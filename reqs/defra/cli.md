# `defra.cli.config.go_layout`: A Go-written `config.yaml` loads with every key it sets

- **Parents:** ["go.node"]
- **Scenarios:** ["node::go_config::a_go_config_sets_the_p2p_addresses", "node::go_config::a_go_config_disables_p2p"]

A rootdir whose `config.yaml` Go DefraDB wrote starts a node with the settings in that file. Go writes its keys through viper, which lowercases them, so each key Go spells differently sets its Rust equivalent: `net.p2pdisabled` → `net.p2p_disabled`, `net.p2paddresses` → `net.p2p_addresses`, `net.pubsubenabled` → `net.pubsub_enabled`, `net.relay` → `net.relay_enabled`, `api.pubkeypath` → `api.pubkey_path`, `api.privkeypath` → `api.privkey_path`, `api.allowed-origins` → `api.allowed_origins`, `datastore.maxtxnretries` → `datastore.max_txn_retries`, `datastore.noencryption` → `datastore.no_encryption`, `datastore.nosearchableencryption` → `datastore.no_searchable_encryption`, `datastore.nosigning` → `datastore.no_signing`, `datastore.defaultkeytype` → `datastore.default_key_type`, `log.colordisabled` → `log.color_disabled`, `secretfile` → `secret_file`, `telemetry.disabled` → `telemetry_disabled` and `replicator.retryintervals` → `replicator_retry_intervals`. Section names match case-insensitively and keys within a section are lowercased first, as viper reads them. Relative `datastore.path`, `keyring.path`, `api.pubkeypath` and `api.privkeypath` resolve against the rootdir, as Go's `ConfigPaths` do.

A node started from a Go config with `net.p2paddresses: [/ip4/127.0.0.1/tcp/<port>]` lists an address on that port in its peer info; one with `net.p2pdisabled: true` starts without P2P and its peer info is an empty list.

A Rust key that is present in the same section wins over its Go spelling. Keys only Go has (`api.audience`, `log.caller`, `net.p2pblocksynctimeout`) are ignored, with one warning each when the value is not Go's default (`""`, `false`, `30`). `datastore.badger.valuelogfilesize` and viper's `""` rootdir key are dropped without a warning. `log.format: csv` still fails to load, with a parse error naming `csv`: defradb.rs has no CSV log format.

# `defra.cli.identity.vera_delegation`: `--identity` acts on a vera.rs node under the identity's own Vera delegation

- **Parents:** ["go.acp.dac.p2p.remote-dac"]
- **Scenarios:** ["acp::dac::p2p::add::acp_p2_p_add_private_document_and_sync_after_adding_relationship_remote_dac", "acp::dac::p2p::update_by_non_creator_on_peer::acp_p2_p_non_creator_updates_indexed_field_on_peer_node_succeeds", "acp::dac::remote_dac_cli::a_cli_ed25519_identity_grants_under_nac_and_reads_dac_status"]

`defradb client --identity <key>` (or `--identity-name`) asks the node for `GET /api/v0/acp/status` first, carrying the identity token as its `Authorization: Bearer` credential. When the answer names a `delegation` audience, the node's document ACP is a vera.rs deployment, and the client sends, as its `Authorization: Bearer` token, a Vera delegation the identity's key signs for that audience's worker and deployment, in place of the identity token it sends any other node. The node authenticates the caller from that delegation and acts on Vera for it: a document actor relationship the identity adds through the CLI is submitted under the identity's authority instead of failing with `no authority to submit ACP changes as <did>: the request's token is not a delegation`. When the status request fails, or its answer names no audience, the client sends the identity token, as before. An Ed25519 identity can therefore read the status and use the delegation on a node with NAC enabled when its identity has `DacStatus`. A secp256k1 key (32 bytes) and an Ed25519 key (64 bytes) both sign delegations.

# `defra.cli.query.error_code`: `client query` prints a GraphQL error's code

- **Parents:** ["go.node"]
- **Scenarios:** ["mutation::error_code::an_unknown_collection_mutation_carries_its_code", "mutation::error_code::a_retried_unknown_collection_mutation_carries_its_code"]

`defradb client query` prints the GraphQL response a request with errors gets, as indented JSON on stdout, in the key order Go's `client query` prints: `errors`, then `data`, then `extensions` when the response has some. Each error keeps the `extensions` the server sent, so its `extensions.code` reaches the user; Go prints only each error's `message`, so the error `extensions` are defradb.rs's own. With a `User` collection and no `Nope` collection, `defradb client query 'mutation { add_Nope(input: {name: "x"}) { _docID } }'` prints

```json
{
  "errors": [
    {
      "message": "parse error: Cannot query field \"add_Nope\" on type \"Mutation\".",
      "extensions": {
        "code": "UNKNOWN_MUTATION_COLLECTION"
      }
    }
  ],
  "data": null
}
```

The in-process node and the Rust HTTP client report the same code on the same error. Go reports no code. As before this requirement, the command still exits non-zero and prints `Error: server returned error: <the messages, joined by "; ">` to stderr when the response has errors; Go exits zero.

# `defra.cli.version`: `defradb version` names the build in Go DefraDB's shape

- **Parents:** ["go.node"]
- **Scenarios:** ["node::version::version_names_the_build_in_go_shape"]

`defradb version` prints one line on stdout, Go DefraDB's line with Rust in place of Go: `defradb <release> (<commit> <commit date>) built with Rust <rustc version> <os>/<arch>`, for example `defradb 0.5.0 (e152b5ca 2026-10-31) built with Rust 1.100.0 macos/aarch64`. `<commit>` is the first 8 characters of the commit the binary was built from, as Go truncates it, and `<commit date>` that commit's date as `YYYY-MM-DD`; a build outside a git checkout prints `unknown` for both. The command prints no log line on stderr; a Go-written config with ignored keys still prints one warning per such key. Go prints the same line on stderr.

`defradb version --full` prints Go's lines, then defradb.rs's own:

```
defradb <release> (<commit> <commit date>)
* HTTP API: v1
* P2P multicodec: /defra/0.0.1
* DocID versions: 1
* Rust: <rustc version> <os>/<arch>
* Commit: <the full commit hash>
* Go compat: <GO_COMPAT_COMMIT> (<GO_COMPAT_BRANCH>, <GO_COMPAT_TAG>)
```

where Go prints `* Go: <go version> <os>/<arch>` in place of the `* Rust:` line and stops there. `, <GO_COMPAT_TAG>` is left out when the tag is empty.

`defradb version --format json` prints, and `GET /api/v1/version` answers, every key Go's `version --format json` prints except `go`, with Go's values for the ones Go fixes and in Go's order: `release`, `commit` (the full hash), `commitDate`, `httpAPI` (`v1`), `docIDVersions` (`1`) and `netProtocol` (`/defra/0.0.1`). After them come defradb.rs's own keys: `version` and `docIdVersions` (the same values as `release` and `docIDVersions`, under the names defradb.rs printed before), `rust` (the `* Rust:` value) and `goCompat` (`{commit, branch, tag}`).
