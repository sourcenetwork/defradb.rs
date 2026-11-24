#![cfg(feature = "vera")]

use clap::{CommandFactory, Parser};
use cli::cli::Cli;
use cli::config::{AcpConfig, AcpDocumentType, Config};

#[test]
fn help_and_environment_names_use_vera() {
    let mut command = Cli::command();
    command.clone().debug_assert();
    let help = command.render_long_help().to_string();
    assert!(help.contains("--vera-rs-address"));
    assert!(help.contains("DEFRA_VERA_RS_ADDRESS"));
    for removed in [
        "sourcehub",
        "source-hub",
        "hub-rs",
        "HUB_RS",
        "--vera-address",
    ] {
        assert!(!help.contains(removed), "{removed}");
    }
}

#[test]
fn go_chain_names_are_not_accepted() {
    for selector in ["vera", "sourcehub", "source-hub", "remote"] {
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
        "--vera-address",
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
fn native_provider_flags_work_with_the_vera_feature() {
    for (provider, address_flag) in [
        ("verars", "--vera-rs-address"),
        ("vera-rs", "--vera-rs-address"),
    ] {
        let cli = Cli::try_parse_from([
            "defradb",
            "--document-acp-type",
            provider,
            address_flag,
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
        assert_eq!(config.acp.vera_rs_address, "http://localhost:8545");
        assert_eq!(config.acp.vera_consensus_key, "aabb");
        assert_eq!(config.acp.vera_deployment_id, Some(9001));
        let serialized = serde_json::to_value(&config.acp).unwrap();
        assert_eq!(serialized["document_type"], "verars");
        assert_eq!(serialized["vera_rs_address"], "http://localhost:8545");
        assert!(serialized.get("hub_rs_address").is_none());
        assert_eq!(config.acp.document_type.to_string(), "verars");
        let restored: AcpConfig = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.document_type, AcpDocumentType::VeraRs);
    }
}

#[test]
fn old_native_names_are_not_accepted() {
    for selector in ["hubrs", "hub-rs"] {
        assert!(selector.parse::<AcpDocumentType>().is_err());
        let mut value = serde_json::to_value(AcpConfig::default()).unwrap();
        value["document_type"] = selector.into();
        assert!(serde_json::from_value::<AcpConfig>(value).is_err());
        assert!(
            Cli::try_parse_from(["defradb", "--document-acp-type", selector, "version"]).is_err()
        );
    }
    assert!(Cli::try_parse_from([
        "defradb",
        "--hub-rs-address",
        "http://localhost:8545",
        "version"
    ])
    .is_err());
    let mut value = serde_json::to_value(AcpConfig::default()).unwrap();
    value.as_object_mut().unwrap().remove("vera_rs_address");
    value["hub_rs_address"] = "http://obsolete:8545".into();
    let config: AcpConfig = serde_json::from_value(value).unwrap();
    assert!(config.vera_rs_address.is_empty());
}

#[test]
fn native_environment_uses_only_vera_names() {
    if let Ok(expected) = std::env::var("VERARS_NAMING_TEST_EXPECTED") {
        let cli = Cli::try_parse_from(["defradb", "version"]).unwrap();
        let mut config = Config::default();
        config.apply_cli_flags(&cli).unwrap();
        assert_eq!(config.acp.vera_rs_address, expected);
        return;
    }
    for (canonical, legacy, expected) in [
        (Some("canonical"), None, "canonical"),
        (None, Some("legacy"), ""),
        (Some("canonical"), Some("legacy"), "canonical"),
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "native_environment_uses_only_vera_names"])
            .env("VERARS_NAMING_TEST_EXPECTED", expected)
            .env_remove("DEFRA_VERA_RS_ADDRESS")
            .env_remove("DEFRA_HUB_RS_ADDRESS");
        if let Some(value) = canonical {
            command.env("DEFRA_VERA_RS_ADDRESS", value);
        }
        if let Some(value) = legacy {
            command.env("DEFRA_HUB_RS_ADDRESS", value);
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
