mod common;

use clap::Parser;
use cli::cli::Cli;
use cli::config::Config;

fn load_dir(root: &std::path::Path, extra: &[&str]) -> Config {
    let mut args = vec!["defradb", "--rootdir", root.to_str().unwrap()];
    args.extend_from_slice(extra);
    args.push("version");
    Config::load(&Cli::try_parse_from(args).unwrap()).unwrap()
}

fn load(acp_yaml: &str, extra: &[&str]) -> Config {
    let root = tempfile::tempdir().unwrap();
    let indented: String = acp_yaml.lines().map(|line| format!("  {line}\n")).collect();
    std::fs::write(root.path().join("config.yaml"), format!("acp:\n{indented}")).unwrap();
    load_dir(root.path(), extra)
}

#[cfg(feature = "vera")]
mod vera {
    use super::*;
    use cli::config::AcpDocumentType;

    #[test]
    fn remote_selects_vera_from_flag_and_yaml() {
        let cli =
            Cli::try_parse_from(["defradb", "--document-acp-type", "remote", "version"]).unwrap();
        let mut config = Config::default();
        config.apply_cli_flags(&cli).unwrap();
        assert_eq!(config.acp.document_type, AcpDocumentType::Vera);

        let config = load("document:\n  type: remote\n", &[]);
        assert_eq!(config.acp.document_type, AcpDocumentType::Vera);
    }

    #[test]
    fn go_nested_remote_keys_fill_the_vera_fields_case_insensitively() {
        let config = load(
            "Document:\n  Type: remote\n  Remote:\n    LogID: chain-1\n    GRPCAddress: http://grpc:9090\n    CometRPCAddress: http://comet:26657\n    KeyName: ignored\n    Address: acct-1\n",
            &[],
        );
        assert_eq!(config.acp.document_type, AcpDocumentType::Vera);
        assert_eq!(config.acp.vera_chain_id, "chain-1");
        assert_eq!(config.acp.vera_grpc_address, "http://grpc:9090");
        assert_eq!(config.acp.vera_comet_address, "http://comet:26657");
        assert_eq!(config.acp.remote_address, "acct-1");
    }

    #[test]
    fn rust_keys_win_over_go_keys() {
        let config = load(
            "document_type: verars\nvera_chain_id: rust-chain\ndocument:\n  type: remote\n  remote:\n    logid: go-chain\n",
            &[],
        );
        assert_eq!(config.acp.document_type, AcpDocumentType::VeraRs);
        assert_eq!(config.acp.vera_chain_id, "rust-chain");
    }
}

#[test]
fn remote_dac_address_flag_and_go_key_set_the_remote_address() {
    let config = load(
        "document_type: none\n",
        &["--remote-dac-address", "acct-flag"],
    );
    assert_eq!(config.acp.remote_address, "acct-flag");

    let config = load(
        "document:\n  remote:\n    address: acct-yaml\n  type: none\n",
        &[],
    );
    assert_eq!(config.acp.remote_address, "acct-yaml");
}

#[test]
fn go_keyname_is_ignored_and_the_rest_loads() {
    let with_keyname = load(
        "document_type: none\ndocument:\n  remote:\n    keyname: signer\n    address: acct\n",
        &[],
    );
    let without = load(
        "document_type: none\ndocument:\n  remote:\n    address: acct\n",
        &[],
    );
    assert_eq!(with_keyname.acp.remote_address, without.acp.remote_address);
    assert_eq!(with_keyname.acp.document_type, without.acp.document_type);
}

#[test]
fn acp_only_config_loads_every_other_section_from_defaults() {
    let root = tempfile::tempdir().unwrap();
    let defaults = load_dir(root.path(), &[]);
    std::fs::write(
        root.path().join("config.yaml"),
        "acp:\n  document:\n    type: none\n    remote:\n      address: acct\n",
    )
    .unwrap();
    let loaded = load_dir(root.path(), &[]);

    assert_eq!(loaded.acp.remote_address, "acct");
    assert_eq!(loaded.acp.node_enable, defaults.acp.node_enable);
    let without_acp = |config: &Config| {
        let mut value = serde_yaml::to_value(config).unwrap();
        value.as_mapping_mut().unwrap().remove("acp");
        value
    };
    assert_eq!(without_acp(&loaded), without_acp(&defaults));
}

fn load_yaml(root: &std::path::Path, yaml: &str) -> Config {
    std::fs::write(root.join("config.yaml"), yaml).unwrap();
    load_dir(root, &[])
}

/// Every Go key set to something other than Rust's default, as viper writes them.
const GO_NON_DEFAULTS: &str = "\
net:
    p2pdisabled: true
    p2paddresses:
        - /ip4/0.0.0.0/tcp/9999
    pubsubenabled: false
    relay: false
api:
    pubkeypath: certs/server.crt
    privkeypath: certs/server.key
    allowed-origins:
        - https://example.com
datastore:
    maxtxnretries: 9
    noencryption: true
    nosearchableencryption: true
    nosigning: true
    defaultkeytype: ed25519
log:
    colordisabled: true
secretfile: other.env
telemetry:
    disabled: true
replicator:
    retryintervals:
        - 1
        - 2
";

#[test]
fn every_go_key_sets_its_rust_key() {
    let root = tempfile::tempdir().unwrap();
    let config = load_yaml(root.path(), GO_NON_DEFAULTS);
    assert!(config.net.p2p_disabled);
    let in_root = |path: &str| root.path().join(path).display().to_string();
    assert_eq!(config.api.pubkey_path, in_root("certs/server.crt"));
    assert_eq!(config.api.privkey_path, in_root("certs/server.key"));
    assert_eq!(config.api.allowed_origins, ["https://example.com"]);
    assert_eq!(config.datastore.max_txn_retries, 9);
    assert!(config.datastore.no_encryption);
    assert!(config.datastore.no_searchable_encryption);
    assert!(config.datastore.no_signing);
    assert_eq!(config.datastore.default_key_type, "ed25519");
    assert!(config.log.color_disabled);
    assert_eq!(config.secret_file, "other.env");
    assert!(config.telemetry_disabled);
    assert_eq!(config.replicator_retry_intervals, [1, 2]);
}

#[test]
fn a_rust_key_wins_over_its_go_spelling() {
    let root = tempfile::tempdir().unwrap();
    let config = load_yaml(
        root.path(),
        "net:\n  p2p_disabled: false\n  p2pdisabled: true\ndatastore:\n  no_signing: false\n  nosigning: true\n\
         secret_file: rust.env\nsecretfile: go.env\ntelemetry_disabled: false\ntelemetry:\n  disabled: true\n",
    );
    assert!(!config.net.p2p_disabled);
    assert!(!config.datastore.no_signing);
    assert_eq!(config.secret_file, "rust.env");
    assert!(!config.telemetry_disabled);
}

#[test]
fn go_sections_and_keys_match_in_any_case() {
    let root = tempfile::tempdir().unwrap();
    let config = load_yaml(
        root.path(),
        "Net:\n  P2PDisabled: true\nDataStore:\n  MaxTxnRetries: 3\n  NoSigning: true\n\
         SecretFile: mixed.env\nTelemetry:\n  Disabled: true\n",
    );
    assert!(config.net.p2p_disabled);
    assert!(config.datastore.no_signing);
    assert_eq!(config.datastore.max_txn_retries, 3);
    assert_eq!(config.secret_file, "mixed.env");
    assert!(config.telemetry_disabled);
}

#[test]
fn go_only_keys_and_badger_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    let sections_only = load_yaml(root.path(), "api: {}\nlog: {}\nnet: {}\ndatastore: {}\n");
    let config = load_yaml(
        root.path(),
        "\"\": ./root\napi:\n  audience: someone\nlog:\n  caller: true\nnet:\n  p2pblocksynctimeout: 5\n\
         \x20 p2paddresses: [/ip4/0.0.0.0/tcp/9999]\n  pubsubenabled: false\n  relay: true\n\
         datastore:\n  badger:\n    valuelogfilesize: 1073741824\n",
    );
    assert_eq!(
        serde_yaml::to_value(&config).unwrap(),
        serde_yaml::to_value(&sections_only).unwrap()
    );
}

#[test]
fn a_rust_written_config_loads_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let defaults = load_dir(root.path(), &[]);
    let fresh = Config {
        rootdir: root.path().to_owned(),
        ..Config::default()
    };
    fresh.create_if_missing().unwrap();
    let mut written = fresh.clone();
    written.net.p2p_disabled = true;
    written.net.pinc_bind_port = Some(9171);
    written.secret_file = "rust.env".into();
    written.telemetry_disabled = true;
    let loaded = load_dir(root.path(), &[]);
    assert_eq!(
        serde_yaml::to_value(&loaded).unwrap(),
        serde_yaml::to_value(&defaults).unwrap()
    );
    std::fs::write(
        root.path().join("config.yaml"),
        serde_yaml::to_string(&written).unwrap(),
    )
    .unwrap();
    let loaded = load_dir(root.path(), &[]);
    assert!(loaded.net.p2p_disabled);
    assert_eq!(loaded.net.pinc_bind_port, Some(9171));
    assert_eq!(loaded.secret_file, "rust.env");
    assert!(loaded.telemetry_disabled);
}

#[test]
fn a_go_csv_log_format_still_fails() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.yaml"), "log:\n  format: csv\n").unwrap();
    let cli = Cli::try_parse_from([
        "defradb",
        "--rootdir",
        root.path().to_str().unwrap(),
        "version",
    ])
    .unwrap();
    let error = Config::load(&cli).unwrap_err().to_string();
    assert!(error.contains("csv"), "{error}");
}

/// Runs `defradb --rootdir <root> version` and returns everything it printed.
fn run_version(root: &std::path::Path) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_defradb"))
        .arg("--rootdir")
        .arg(root)
        .arg("version")
        .output()
        .expect("run defradb");
    assert!(
        output.status.success(),
        "defradb version failed: {output:?}"
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn the_binary_warns_about_each_ignored_go_key() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("config.yaml"),
        "api:\n  audience: someone\nlog:\n  caller: true\nnet:\n  p2pblocksynctimeout: 5\n\
         \x20 p2paddresses: [/ip4/0.0.0.0/tcp/9999]\n  pubsubenabled: false\n  relay: true\n\
         acp:\n  document:\n    remote:\n      keyname: signer\n",
    )
    .unwrap();
    let printed = run_version(root.path());
    for key in [
        "api.audience",
        "log.caller",
        "net.p2pblocksynctimeout",
        "acp.document.remote.keyname",
    ] {
        assert_eq!(
            printed.matches(&format!("{key} is ignored")).count(),
            1,
            "one warning for {key} in:\n{printed}"
        );
    }
}

#[test]
fn the_binary_says_nothing_about_go_only_keys_at_go_defaults() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("config.yaml"),
        "api:\n  audience: \"\"\nlog:\n  caller: false\nnet:\n  p2pblocksynctimeout: 30\n",
    )
    .unwrap();
    let printed = run_version(root.path());
    assert!(
        !printed.contains("is ignored"),
        "no warning expected in:\n{printed}"
    );
}

/// `defradb start` on a Go-written config runs P2P on pinc, and logs each
/// Go-only key it ignores: Go's libp2p listen addresses among them.
#[nuclei::test]
async fn start_runs_a_go_config() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("config.yaml"),
        "keyring:\n    disabled: true\nnet:\n    p2paddresses:\n        - /ip4/127.0.0.1/tcp/9999\n    \
         p2pdisabled: false\n    p2pblocksynctimeout: 5\n    relay: false\n",
    )
    .unwrap();
    let (log, output) = common::NodeLog::create(root.path().join("node.log"));
    let _child = KillOnDrop(
        std::process::Command::new(env!("CARGO_BIN_EXE_defradb"))
            .arg("--rootdir")
            .arg(root.path())
            .args(["--url", common::ANY_PORT])
            .args(common::LOG_FLAGS)
            .args(["start", "--store", "memory"])
            .env("RUST_LOG", "info")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(output)
            .spawn()
            .expect("start defradb"),
    );
    let http = log.http_address(std::time::Duration::from_secs(15));
    let client = net::http::Client::builder(edge::testing::edge())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let info: serde_json::Value = client
        .get(format!("http://{http}/api/v0/p2p/info"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let listed = info.to_string();
    assert!(
        listed.contains('@') && !listed.contains("9999"),
        "peer info {listed} lists no pinc address"
    );
    let text = log.text();
    for key in ["net.p2pblocksynctimeout", "net.p2paddresses"] {
        assert_eq!(
            text.matches(&format!("{key} is ignored")).count(),
            1,
            "one warning for {key} in:\n{text}"
        );
    }
}

/// Kills the child when the test ends, whether it passed or panicked.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
