# `defra.cli.config.go_layout`: A Go-written `config.yaml` loads with every key it sets

- **Parents:** ["go.node"]
- **Scenarios:** ["node::go_config::a_go_config_sets_the_p2p_addresses", "node::go_config::a_go_config_disables_p2p"]

A rootdir whose `config.yaml` Go DefraDB wrote starts a node with the settings in that file. Go writes its keys through viper, which lowercases them, so each key Go spells differently sets its Rust equivalent: `net.p2pdisabled` → `net.p2p_disabled`, `net.p2paddresses` → `net.p2p_addresses`, `net.pubsubenabled` → `net.pubsub_enabled`, `net.relay` → `net.relay_enabled`, `api.pubkeypath` → `api.pubkey_path`, `api.privkeypath` → `api.privkey_path`, `api.allowed-origins` → `api.allowed_origins`, `datastore.maxtxnretries` → `datastore.max_txn_retries`, `datastore.noencryption` → `datastore.no_encryption`, `datastore.nosearchableencryption` → `datastore.no_searchable_encryption`, `datastore.nosigning` → `datastore.no_signing`, `datastore.defaultkeytype` → `datastore.default_key_type`, `log.colordisabled` → `log.color_disabled`, `secretfile` → `secret_file`, `telemetry.disabled` → `telemetry_disabled` and `replicator.retryintervals` → `replicator_retry_intervals`. Section names match case-insensitively and keys within a section are lowercased first, as viper reads them. Relative `datastore.path`, `keyring.path`, `api.pubkeypath` and `api.privkeypath` resolve against the rootdir, as Go's `ConfigPaths` do.

A node started from a Go config with `net.p2paddresses: [/ip4/127.0.0.1/tcp/<port>]` lists an address on that port in its peer info; one with `net.p2pdisabled: true` starts without P2P and its peer info is an empty list.

A Rust key that is present in the same section wins over its Go spelling. Keys only Go has (`api.audience`, `log.caller`, `net.p2pblocksynctimeout`) are ignored, with one warning each when the value is not Go's default (`""`, `false`, `30`). `datastore.badger.valuelogfilesize` and viper's `""` rootdir key are dropped without a warning. `log.format: csv` still fails to load, with a parse error naming `csv`: defradb.rs has no CSV log format.

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
