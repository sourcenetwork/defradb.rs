# `defra.cli.config.go_layout`: A Go-written `config.yaml` loads with every key it sets

- **Parents:** ["go.node"]
- **Scenarios:** ["node::go_config::a_go_config_runs_p2p_on_pinc", "node::go_config::a_go_config_disables_p2p"]

A rootdir whose `config.yaml` Go DefraDB wrote starts a node with the settings in that file. Go writes its keys through viper, which lowercases them, so each key Go spells differently sets its Rust equivalent: `net.p2pdisabled` → `net.p2p_disabled`, `api.pubkeypath` → `api.pubkey_path`, `api.privkeypath` → `api.privkey_path`, `api.allowed-origins` → `api.allowed_origins`, `datastore.maxtxnretries` → `datastore.max_txn_retries`, `datastore.noencryption` → `datastore.no_encryption`, `datastore.nosearchableencryption` → `datastore.no_searchable_encryption`, `datastore.nosigning` → `datastore.no_signing`, `datastore.defaultkeytype` → `datastore.default_key_type`, `log.colordisabled` → `log.color_disabled`, `secretfile` → `secret_file`, `telemetry.disabled` → `telemetry_disabled` and `replicator.retryintervals` → `replicator_retry_intervals`. Section names match case-insensitively and keys within a section are lowercased first, as viper reads them. Relative `datastore.path`, `keyring.path`, `api.pubkeypath` and `api.privkeypath` resolve against the rootdir, as Go's `ConfigPaths` do.

A node started from a Go config with `net.p2pdisabled: false` runs P2P on pinc and lists its pinc address (`<peer id>@<ip>:<port>`) in its peer info; one with `net.p2pdisabled: true` starts without P2P and its peer info is an empty list.

A Rust key that is present in the same section wins over its Go spelling. Keys only Go has (`api.audience`, `log.caller`, `net.p2pblocksynctimeout`, and libp2p's `net.p2paddresses`, `net.pubsubenabled` and `net.relay`) are ignored, with one warning each when the value is not Go's default (`""`, `false`, `30`, `[/ip4/127.0.0.1/tcp/9171]`, `true`, `false`). `datastore.badger.valuelogfilesize` and viper's `""` rootdir key are dropped without a warning. `log.format: csv` still fails to load, with a parse error naming `csv`: defradb.rs has no CSV log format.

# `defra.cli.pinc`: A CLI node's pinc settings

- **Parents:** ["defra"]
- **Scenarios:** ["hub::nodes_replicate_a_subscribed_collection_through_a_cli_hub", "node::go_config::a_go_config_runs_p2p_on_pinc"]

A node started with `defradb start` binds its pinc endpoint on `net.pinc_bind_addr` (`--pinc-bind-addr`, `127.0.0.1` when unset, as Go's CLI listens on loopback by default) and `net.pinc_bind_port` (`--pinc-bind-port`, a port the OS picks when unset), runs topics through the hubs `net.pinc_hubs` names (`--pinc-hubs`), admits inbound connections only from the peer ids `net.pinc_allowed_peers` lists (`--pinc-allowed-peers`, every peer when empty), and dials each of `net.peers` (`--peers`) once started, without waiting for it. A node bound where other hosts can reach it with an empty `net.pinc_allowed_peers` starts and logs one warning that any peer may connect. A hub or peer address is `<peer id>@<udp address>`; any other fails the start with `invalid config: invalid pinc address <address>: …`.

The FFI and `defra-node` embedders bind loopback the same way when they name no bind, `defra.cli.pinc.embedder_bind`.

# `defra.cli.pinc.embedder_bind`: An embedder that names no pinc bind binds loopback

- **Parents:** ["defra.cli.pinc"]
- **Scenarios:** ["net::pinc_bind::a_node_that_names_no_bind_binds_loopback"]

A `defra-node` embedder whose `PincConfig` keeps its default `bind`, and an FFI host whose `listening_addresses` is empty, bind the pinc endpoint on `127.0.0.1` on a port the OS picks, as `defradb start` does, so only this host can connect: the node's peer info lists a loopback address. An embedder that names a bind, `0.0.0.0:<port>` included, binds it.

# `defra.cli.pinc.public_addrs`: A node lists the addresses peers can reach

- **Parents:** ["defra.cli.pinc"]
- **Scenarios:** ["node::go_config::a_configured_public_address_is_listed_first", "addr::tests::a_wildcard_bind_lists_every_interface_but_loopback_and_public_first"]

The addresses in `net.pinc_public_addrs` (`--pinc-public-addrs`, each `<ip>:<port>`: a public address in front of a NAT, a forwarded port) come first in the node's peer info, ahead of the addresses it binds. A node bound on a wildcard address lists the address of each of its interfaces, and never a loopback or IPv6 link-local one; only a node bound on loopback lists loopback. Its shareable address is never loopback unless it bound loopback.

# `defra.cli.hub`: `defradb hub` runs a pinc hub

- **Parents:** ["defra"]
- **Scenarios:** ["hub::nodes_replicate_a_subscribed_collection_through_a_cli_hub"]

`defradb hub` binds a pinc hub on `--bind` (`127.0.0.1:9172` by default), prints its address `<peer id>@<ip>:<port>` on one line of standard output, and runs until SIGINT or SIGTERM, then exits 0. Its key lives in `--key-file` (`<rootdir>/hub.key` by default), created when missing, so the hub keeps its peer id across restarts. Two nodes naming the hub in `--pinc-hubs`, connected and subscribed to the same collection, replicate a document created on one to the other.

Each `--public-addr` (`<ip>:<port>`, repeatable) is listed first in the printed address. A hub bound on a wildcard address lists its interfaces' addresses but never loopback; with none of those and no `--public-addr` it fails to start rather than print an address no node can reach. `--relay` (off by default) adds pinc's relay role, `defra.cli.hub.relay`.

# `defra.cli.hub.relay`: A relaying hub's members reach each other by peer id

- **Parents:** ["defra.cli.hub"]
- **Scenarios:** ["net::pinc_relay::members_of_a_relaying_hub_connect_by_peer_id_alone", "net::pinc_relay::members_of_a_hub_that_does_not_relay_cannot_connect_by_peer_id_alone", "tests::relay::members_of_a_relaying_hub_reach_each_other_by_id_alone"]

Two nodes naming a hub that runs the relay role connect by peer id alone, with no address of each other: `ConnectPeers` naming only the peer id succeeds and `ActivePeers` lists the peer. The relay carries the session until a direct path works. Through a hub without the role the same connect fails. The relay runs over UDP only: a node whose UDP is blocked is not reached through it.
