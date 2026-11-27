#![cfg(feature = "vera")]

use clap::{CommandFactory, Parser};
use cli::cli::Cli;
use cli::config::{AcpConfig, AcpDocumentType, Config};

#[test]
fn help_and_environment_names_use_vera() {
    let mut command = Cli::command();
    command.clone().debug_assert();
    let help = command.render_long_help().to_string();
    assert!(help.contains("--vera-address"));
    assert!(help.contains("DEFRA_VERA_ADDRESS"));
    for removed in [
        "sourcehub",
        "source-hub",
        "hub-rs",
        "HUB_RS",
        "verars",
        "vera-rs",
        "VERA_RS",
    ] {
        assert!(!help.contains(removed), "{removed}");
    }
}

#[test]
fn the_vera_selector_and_flags_configure_the_provider() {
    let cli = Cli::try_parse_from([
        "defradb",
        "--document-acp-type",
        "vera",
        "--vera-address",
        "http://localhost:8545",
        "--vera-consensus-key",
        "aabb",
        "--vera-deployment-id",
        "9001",
        "version",
    ])
    .unwrap();
    let mut config = Config::default();
    config.apply_cli_flags(&cli).unwrap();
    assert_eq!(config.acp.document_type, AcpDocumentType::Vera);
    assert_eq!(config.acp.vera_address, "http://localhost:8545");
    assert_eq!(config.acp.vera_consensus_key, "aabb");
    assert_eq!(config.acp.vera_deployment_id, Some(9001));
    let serialized = serde_json::to_value(&config.acp).unwrap();
    assert_eq!(serialized["document_type"], "vera");
    assert_eq!(serialized["vera_address"], "http://localhost:8545");
    assert_eq!(config.acp.document_type.to_string(), "vera");
    let restored: AcpConfig = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.document_type, AcpDocumentType::Vera);
}

#[test]
fn old_names_are_not_accepted() {
    for selector in [
        "verars",
        "vera-rs",
        "verars-embedded",
        "vera-rs-embedded",
        "hubrs",
        "hub-rs",
        "sourcehub",
        "source-hub",
        "remote",
    ] {
        assert!(selector.parse::<AcpDocumentType>().is_err(), "{selector}");
        let mut value = serde_json::to_value(AcpConfig::default()).unwrap();
        value["document_type"] = selector.into();
        assert!(
            serde_json::from_value::<AcpConfig>(value).is_err(),
            "{selector}"
        );
        assert!(
            Cli::try_parse_from(["defradb", "--document-acp-type", selector, "version"]).is_err(),
            "{selector}"
        );
    }
    for flag in [
        "--vera-rs-address",
        "--hub-rs-address",
        "--vera-grpc-address",
        "--vera-comet-address",
        "--vera-events-ws",
        "--vera-chain-id",
        "--source-hub-address",
        "--source-hub-chain-id",
    ] {
        assert!(
            Cli::try_parse_from(["defradb", flag, "value", "version"]).is_err(),
            "{flag}"
        );
    }
}

#[test]
fn removed_config_keys_load_with_a_warning_each() {
    let removed = [
        ("vera_rs_address", "set acp.vera_address instead"),
        ("hub_rs_address", "set acp.vera_address instead"),
        ("sourcehub_address", "the Go-chain ACP provider was removed"),
        (
            "sourcehub_chain_id",
            "the Go-chain ACP provider was removed",
        ),
        ("vera_grpc_address", "the Go-chain ACP provider was removed"),
        (
            "vera_comet_address",
            "the Go-chain ACP provider was removed",
        ),
        ("vera_events_ws", "the Go-chain ACP provider was removed"),
        ("vera_chain_id", "the Go-chain ACP provider was removed"),
        (
            "circuit_breaker_threshold",
            "the Go-chain ACP provider was removed",
        ),
        (
            "circuit_breaker_reset_timeout",
            "the Go-chain ACP provider was removed",
        ),
        ("cache_ttl", "the Go-chain ACP provider was removed"),
    ];
    for (key, reason) in removed {
        let rootdir = tempfile::tempdir().unwrap();
        std::fs::write(
            rootdir.path().join("config.yaml"),
            format!("acp:\n  {key}: \"http://obsolete:8545\"\n"),
        )
        .unwrap();
        let cli = Cli::try_parse_from([
            "defradb",
            "--rootdir",
            rootdir.path().to_str().unwrap(),
            "version",
        ])
        .unwrap();
        let config = Config::load(&cli).unwrap();
        assert!(config.acp.vera_address.is_empty(), "{key}");
        assert_eq!(
            config.ignored_keys.len(),
            1,
            "{key}: {:?}",
            config.ignored_keys
        );
        let warning = &config.ignored_keys[0];
        assert!(
            warning.starts_with(&format!("acp.{key} is ignored: ")),
            "{warning}"
        );
        assert!(warning.contains(reason), "{warning}");
    }
}

#[test]
fn the_environment_uses_only_the_vera_name() {
    if let Ok(expected) = std::env::var("VERA_NAMING_TEST_EXPECTED") {
        let cli = Cli::try_parse_from(["defradb", "version"]).unwrap();
        let mut config = Config::default();
        config.apply_cli_flags(&cli).unwrap();
        assert_eq!(config.acp.vera_address, expected);
        return;
    }
    const OLD: [&str; 3] = [
        "DEFRA_VERA_RS_ADDRESS",
        "DEFRA_HUB_RS_ADDRESS",
        "DEFRA_SOURCE_HUB_ADDRESS",
    ];
    for (canonical, old, expected) in [
        (Some("canonical"), false, "canonical"),
        (None, true, ""),
        (Some("canonical"), true, "canonical"),
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "the_environment_uses_only_the_vera_name"])
            .env("VERA_NAMING_TEST_EXPECTED", expected)
            .env_remove("DEFRA_VERA_ADDRESS");
        for name in OLD {
            command.env_remove(name);
            if old {
                command.env(name, "old");
            }
        }
        if let Some(value) = canonical {
            command.env("DEFRA_VERA_ADDRESS", value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
