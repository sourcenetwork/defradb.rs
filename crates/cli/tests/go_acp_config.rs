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
