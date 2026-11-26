//! Tests for configuration section structs

use cli::config::{
    ApiConfig, DatastoreConfig, DatastoreType, KeyringBackend, KeyringConfig, LogConfig, LogFormat,
    LogLevel, LogOutput, NetConfig,
};
use cli::error::Error;
use storage::backends::DurabilityMode;

#[test]
fn test_log_config_defaults() {
    let config = LogConfig::default();
    assert_eq!(config.level, LogLevel::Info);
    assert_eq!(config.output, LogOutput::Stderr);
    assert_eq!(config.format, LogFormat::Text);
    assert!(!config.stacktrace);
    assert!(!config.source);
    assert!(!config.color_disabled);
    assert!(config.overrides.is_empty());
}

#[test]
fn test_api_config_defaults() {
    let config = ApiConfig::default();
    assert_eq!(config.address, "127.0.0.1:9181");
    assert!(config.allowed_origins.is_empty());
    assert!(config.pubkey_path.is_empty());
    assert!(config.privkey_path.is_empty());
    assert_eq!(config.query_max_depth, query::DEFAULT_MAX_QUERY_DEPTH);
    assert_eq!(config.query_max_width, query::DEFAULT_MAX_QUERY_WIDTH);
    assert_eq!(
        config.query_max_filter_depth,
        query::DEFAULT_MAX_FILTER_DEPTH
    );
    assert_eq!(
        config.query_max_fragment_nesting,
        query::DEFAULT_MAX_FRAGMENT_NESTING
    );
}

#[test]
fn test_datastore_config_defaults() {
    let config = DatastoreConfig::default();
    assert_eq!(config.store, DatastoreType::Regolith);
    assert_eq!(config.path, "data");
    assert_eq!(config.max_txn_retries, 5);
    assert_eq!(config.valuelogfilesize, None);
    assert!(!config.no_encryption);
    assert!(!config.no_signing);
    assert_eq!(config.default_key_type, "secp256k1");
    assert_eq!(config.durability, DurabilityMode::Immediate);
}

#[test]
fn test_net_config_defaults() {
    let config = NetConfig::default();
    assert!(!config.p2p_disabled);
    assert!(config.peers.is_empty());
    assert_eq!(config.pinc_bind_addr, None);
    assert_eq!(config.pinc_bind_port, None);
    assert!(config.pinc_public_addrs.is_empty());
    // Unset, the endpoint binds loopback, as Go's default does.
    assert_eq!(
        config.pinc_bind(),
        std::net::SocketAddr::from(([127, 0, 0, 1], 0))
    );
    assert!(config.pinc_allowed_peers.is_empty());
    assert!(config.pinc_hubs.is_empty());
}

#[test]
fn test_keyring_config_defaults() {
    let config = KeyringConfig::default();
    assert_eq!(config.backend, KeyringBackend::File);
    assert_eq!(config.path, "keys");
    assert_eq!(config.namespace, "defradb");
    assert!(!config.disabled);
}

#[test]
fn test_api_config_validate_valid_address() {
    let config = ApiConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_api_config_validate_invalid_address() {
    let config = ApiConfig {
        address: "not-an-address".to_string(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(Error::InvalidApiAddress(addr, _)) if addr == "not-an-address"));
}

#[test]
fn test_api_config_validate_incomplete_tls_pubkey_only() {
    let config = ApiConfig {
        pubkey_path: "/path/to/pub.key".to_string(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(Error::IncompleteTlsConfig)));
}

#[test]
fn test_api_config_validate_incomplete_tls_privkey_only() {
    let config = ApiConfig {
        privkey_path: "/path/to/priv.key".to_string(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(Error::IncompleteTlsConfig)));
}

#[test]
fn test_api_config_validate_complete_tls() {
    let config = ApiConfig {
        pubkey_path: "/path/to/pub.key".to_string(),
        privkey_path: "/path/to/priv.key".to_string(),
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_api_config_tls_enabled_false_by_default() {
    let config = ApiConfig::default();
    assert!(!config.tls_enabled());
}

#[test]
fn test_api_config_tls_enabled_true_when_configured() {
    let config = ApiConfig {
        pubkey_path: "/path/to/pub.key".to_string(),
        privkey_path: "/path/to/priv.key".to_string(),
        ..Default::default()
    };
    assert!(config.tls_enabled());
}

#[test]
fn test_api_config_tls_enabled_false_partial_config() {
    let config = ApiConfig {
        pubkey_path: "/path/to/pub.key".to_string(),
        ..Default::default()
    };
    assert!(!config.tls_enabled());
}

#[test]
fn test_net_config_validate_valid_pinc_addresses() {
    let config = NetConfig {
        peers: vec![format!("{PEER_ID}@127.0.0.1:9171")],
        pinc_hubs: vec![format!("{PEER_ID}@127.0.0.1:9172")],
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_net_config_validate_invalid_pinc_address() {
    for config in [
        NetConfig {
            peers: vec!["/ip4/127.0.0.1/tcp/9171".to_string()],
            ..Default::default()
        },
        NetConfig {
            pinc_hubs: vec!["not-an-address".to_string()],
            ..Default::default()
        },
    ] {
        assert!(matches!(config.validate(), Err(Error::InvalidConfig(_))));
    }
}

#[test]
fn test_net_config_validate_skipped_when_p2p_disabled() {
    let config = NetConfig {
        p2p_disabled: true,
        peers: vec!["not-an-address".to_string()],
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

const PEER_ID: &str = "ae58ff8833241ac82d6ff7611046ed67b5072d142c588d0063e942d9a75502b6";

/// Every node started on a pre-`Option<u64>` binary had the old 1 GiB default
/// serialized into its own `config.yaml` by `create_if_missing`. Reading it
/// back would apply a 16x compaction-target change to regolith for an
/// operator who never passed the flag, so the key must not deserialize at all.
#[test]
fn legacy_valuelogfilesize_in_a_config_file_is_ignored() {
    let yaml = r#"
store: regolith
path: data
max_txn_retries: 5
valuelogfilesize: 1073741824
no_encryption: false
no_searchable_encryption: false
no_signing: false
default_key_type: secp256k1
"#;
    let config: DatastoreConfig = serde_yaml::from_str(yaml).expect("legacy config still loads");
    assert_eq!(
        config.valuelogfilesize, None,
        "a value written by an older binary must not be mistaken for an operator setting it"
    );
}

/// And the key must no longer be written, so the ambiguity cannot recur.
#[test]
fn valuelogfilesize_is_not_serialized_into_generated_config() {
    let yaml = serde_yaml::to_string(&DatastoreConfig::default()).expect("serializes");
    assert!(
        !yaml.contains("valuelogfilesize"),
        "generated config.yaml must not carry the key:\n{yaml}"
    );
}

#[test]
fn test_api_config_reads_query_max_fragment_nesting_from_yaml() {
    let config: ApiConfig =
        serde_yaml::from_str("query_max_fragment_nesting: 64\n").expect("api config loads");
    assert_eq!(config.query_max_fragment_nesting, 64);
}

#[test]
fn test_api_config_refuses_a_fragment_nesting_the_stack_cannot_hold() {
    let safe_max = query::max_fragment_nesting_for_stack(query::query_thread_stack_bytes());
    for refused in [0, safe_max + 1] {
        let config = ApiConfig {
            query_max_fragment_nesting: refused,
            ..ApiConfig::default()
        };
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("--query-max-fragment-nesting"), "{error}");
        assert!(
            error.contains(&format!("set it from 1 to {safe_max}")),
            "{error}"
        );
    }
    let config = ApiConfig {
        query_max_fragment_nesting: safe_max,
        ..ApiConfig::default()
    };
    config.validate().unwrap();
}
