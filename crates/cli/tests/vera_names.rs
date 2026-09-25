#![cfg(feature = "vera")]

use clap::{CommandFactory, Parser};
use cli::cli::Cli;
use cli::config::{AcpConfig, AcpDocumentType, Config};

#[test]
fn vera_flags_and_legacy_aliases_configure_the_same_provider() {
    for prefix in ["vera", "source-hub"] {
        let args = [
            "defradb".to_string(),
            "--document-acp-type".to_string(),
            prefix.to_string(),
            format!("--{prefix}-address"),
            "http://localhost:1317".to_string(),
            format!("--{prefix}-grpc-address"),
            "http://localhost:9090".to_string(),
            format!("--{prefix}-comet-address"),
            "http://localhost:26657".to_string(),
            format!("--{prefix}-events-ws"),
            "ws://localhost:26657/websocket".to_string(),
            format!("--{prefix}-chain-id"),
            "vera-test".to_string(),
            "version".to_string(),
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        let mut config = Config::default();
        config.apply_cli_flags(&cli).unwrap();
        assert_eq!(config.acp.document_type, AcpDocumentType::Vera);
        assert_eq!(config.acp.vera_address, "http://localhost:1317");
        assert_eq!(config.acp.vera_grpc_address, "http://localhost:9090");
        assert_eq!(config.acp.vera_comet_address, "http://localhost:26657");
        assert_eq!(config.acp.vera_events_ws, "ws://localhost:26657/websocket");
        assert_eq!(config.acp.vera_chain_id, "vera-test");
    }
}

#[test]
fn legacy_config_deserializes_but_serializes_as_vera() {
    let mut value = serde_json::to_value(AcpConfig::default()).unwrap();
    let config = value.as_object_mut().unwrap();
    config.insert("document_type".into(), "sourcehub".into());
    for suffix in [
        "address",
        "grpc_address",
        "comet_address",
        "events_ws",
        "chain_id",
    ] {
        config.remove(&format!("vera_{suffix}"));
        config.insert(
            format!("sourcehub_{suffix}"),
            format!("test-{suffix}").into(),
        );
    }
    let config: AcpConfig = serde_json::from_value(value).unwrap();
    assert_eq!(config.document_type, AcpDocumentType::Vera);
    let output = serde_json::to_value(config).unwrap();
    assert_eq!(output["document_type"], "vera");
    for suffix in [
        "address",
        "grpc_address",
        "comet_address",
        "events_ws",
        "chain_id",
    ] {
        assert_eq!(output[format!("vera_{suffix}")], format!("test-{suffix}"));
        assert!(output.get(format!("sourcehub_{suffix}")).is_none());
    }
}

#[test]
fn help_and_environment_names_use_vera() {
    let mut command = Cli::command();
    command.clone().debug_assert();
    let help = command.render_long_help().to_string();
    assert!(help.contains("--vera-address"));
    assert!(help.contains("DEFRA_VERA_ADDRESS"));
    assert!(!help.contains("sourcehub"));
    assert!(!help.contains("source-hub"));
}

#[test]
fn environment_values_use_vera_with_legacy_fallback() {
    if let Ok(expected) = std::env::var("VERA_NAMING_TEST_EXPECTED") {
        let cli = Cli::try_parse_from(["defradb", "version"]).unwrap();
        let mut config = Config::default();
        config.apply_cli_flags(&cli).unwrap();
        assert_eq!(config.acp.vera_address, expected);
        return;
    }
    for (canonical, legacy, expected) in [
        (Some("canonical"), None, "canonical"),
        (None, Some("legacy"), "legacy"),
        (Some("canonical"), Some("legacy"), "canonical"),
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "environment_values_use_vera_with_legacy_fallback",
            ])
            .env("VERA_NAMING_TEST_EXPECTED", expected)
            .env_remove("DEFRA_VERA_ADDRESS")
            .env_remove("DEFRA_SOURCE_HUB_ADDRESS");
        if let Some(value) = canonical {
            command.env("DEFRA_VERA_ADDRESS", value);
        }
        if let Some(value) = legacy {
            command.env("DEFRA_SOURCE_HUB_ADDRESS", value);
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

#[test]
fn native_provider_flags_work_with_the_vera_feature() {
    for provider in ["hub-rs", "vera-rs"] {
        let cli = Cli::try_parse_from([
            "defradb",
            "--document-acp-type",
            provider,
            "--hub-rs-address",
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
        assert_eq!(config.acp.document_type, AcpDocumentType::VeraRs);
        assert_eq!(config.acp.hub_rs_address, "http://localhost:8545");
        assert_eq!(config.acp.vera_consensus_key, "aabb");
        assert_eq!(config.acp.vera_deployment_id, Some(9001));
        let serialized = serde_json::to_value(&config.acp).unwrap();
        let restored: AcpConfig = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.document_type, AcpDocumentType::VeraRs);
    }
}
