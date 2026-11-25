# `defra.cli.config.go_layout`: A Go-written `config.yaml` loads with every key it sets

- **Parents:** ["go.node"]
- **Scenarios:** ["node::go_config::a_go_config_runs_p2p_on_pinc", "node::go_config::a_go_config_disables_p2p"]

A rootdir whose `config.yaml` Go DefraDB wrote starts a node with the settings in that file. Go writes its keys through viper, which lowercases them, so each key Go spells differently sets its Rust equivalent: `net.p2pdisabled` → `net.p2p_disabled`, `api.pubkeypath` → `api.pubkey_path`, `api.privkeypath` → `api.privkey_path`, `api.allowed-origins` → `api.allowed_origins`, `datastore.maxtxnretries` → `datastore.max_txn_retries`, `datastore.noencryption` → `datastore.no_encryption`, `datastore.nosearchableencryption` → `datastore.no_searchable_encryption`, `datastore.nosigning` → `datastore.no_signing`, `datastore.defaultkeytype` → `datastore.default_key_type`, `log.colordisabled` → `log.color_disabled`, `secretfile` → `secret_file`, `telemetry.disabled` → `telemetry_disabled` and `replicator.retryintervals` → `replicator_retry_intervals`. Section names match case-insensitively and keys within a section are lowercased first, as viper reads them. Relative `datastore.path`, `keyring.path`, `api.pubkeypath` and `api.privkeypath` resolve against the rootdir, as Go's `ConfigPaths` do.

A node started from a Go config with `net.p2pdisabled: false` runs P2P on pinc and lists its pinc address (`<peer id>@<ip>:<port>`) in its peer info; one with `net.p2pdisabled: true` starts without P2P and its peer info is an empty list.

A Rust key that is present in the same section wins over its Go spelling. Keys only Go has (`api.audience`, `log.caller`, `net.p2pblocksynctimeout`, and libp2p's `net.p2paddresses`, `net.pubsubenabled` and `net.relay`) are ignored, with one warning each when the value is not Go's default (`""`, `false`, `30`, `[/ip4/127.0.0.1/tcp/9171]`, `true`, `false`). `datastore.badger.valuelogfilesize` and viper's `""` rootdir key are dropped without a warning. `log.format: csv` still fails to load, with a parse error naming `csv`: defradb.rs has no CSV log format.

# `defra.cli.pinc`: A CLI node's pinc settings

- **Parents:** ["defra"]
- **Scenarios:** ["hub::nodes_replicate_a_subscribed_collection_through_a_cli_hub"]

A node started with `defradb start` binds its pinc endpoint on `net.pinc_bind_addr` (`--pinc-bind-addr`, every interface when unset) and `net.pinc_bind_port` (`--pinc-bind-port`, a port the OS picks when unset), runs topics through the hubs `net.pinc_hubs` names (`--pinc-hubs`), admits inbound connections only from the peer ids `net.pinc_allowed_peers` lists (`--pinc-allowed-peers`, every peer when empty), and dials each of `net.peers` (`--peers`) once started, without waiting for it. A hub or peer address is `<peer id>@<udp address>`; any other fails the start with `invalid config: invalid pinc address <address>: …`.

# `defra.cli.hub`: `defradb hub` runs a pinc hub

- **Parents:** ["defra"]
- **Scenarios:** ["hub::nodes_replicate_a_subscribed_collection_through_a_cli_hub"]

`defradb hub` binds a pinc hub on `--bind` (`127.0.0.1:9172` by default), prints its address `<peer id>@<ip>:<port>` on one line of standard output, and runs until SIGINT or SIGTERM, then exits 0. Its key lives in `--key-file` (`<rootdir>/hub.key` by default), created when missing, so the hub keeps its peer id across restarts. Two nodes naming the hub in `--pinc-hubs`, connected and subscribed to the same collection, replicate a document created on one to the other.
