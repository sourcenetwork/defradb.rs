//! The configuration of `defradb start` with a Vera node of its own.

use clap::Parser as _;
use cli::cli::Cli;
use cli::config::{AcpConfig, AcpDocumentType, Config};

#[test]
fn the_embedded_type_and_its_settings_come_from_flags_and_survive_a_round_trip() {
    let cli = Cli::try_parse_from([
        "defradb",
        "--document-acp-type",
        "verars-embedded",
        "--vera-deployment-id",
        "9407",
        "--vera-embedded-operator",
        "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp",
        "--vera-embedded-operator",
        "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        "--vera-embedded-threshold",
        "2",
        "--vera-embedded-listen",
        "127.0.0.1:9777",
        "version",
    ])
    .unwrap();
    let mut config = Config::default();
    config.apply_cli_flags(&cli).unwrap();
    assert_eq!(config.acp.document_type, AcpDocumentType::VeraRsEmbedded);
    assert!(config.acp.document_type.is_vera_rs());
    assert_eq!(config.acp.vera_deployment_id, Some(9407));
    assert_eq!(config.acp.vera_embedded_operators.len(), 2);
    assert_eq!(config.acp.vera_embedded_threshold, Some(2));
    assert_eq!(config.acp.vera_embedded_listen, "127.0.0.1:9777");

    let serialized = serde_json::to_value(&config.acp).unwrap();
    assert_eq!(serialized["document_type"], "verars-embedded");
    assert_eq!(config.acp.document_type.to_string(), "verars-embedded");
    let restored: AcpConfig = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.document_type, AcpDocumentType::VeraRsEmbedded);
    assert_eq!(restored.vera_embedded_threshold, Some(2));
}

#[test]
fn the_defaults_are_one_operator_approval_on_a_loopback_port_the_system_picks() {
    let acp = AcpConfig::default();
    assert!(acp.vera_embedded_operators.is_empty());
    assert_eq!(acp.vera_embedded_threshold, None);
    assert_eq!(acp.vera_embedded_listen, "127.0.0.1:0");
    assert!(!AcpDocumentType::Local.is_vera_rs());
    assert!(AcpDocumentType::VeraRs.is_vera_rs());
    for spelling in ["verars-embedded", "VeraRs-Embedded", "verarsembedded"] {
        assert_eq!(
            spelling.parse::<AcpDocumentType>().unwrap(),
            AcpDocumentType::VeraRsEmbedded
        );
    }
}
