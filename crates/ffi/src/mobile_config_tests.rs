use super::MobileNodeConfig;

#[test]
fn vera_mobile_config_accepts_the_legacy_name() {
    for name in ["vera", "sourcehub"] {
        let value = serde_json::json!({
            (name): {
                "grpcAddress": "http://localhost:9090",
                "cometRpcAddress": "http://localhost:26657",
                "chainId": "vera-test",
                "signerKeyHex": "11".repeat(32)
            }
        });
        let config: MobileNodeConfig = serde_json::from_value(value).unwrap();
        let vera = config.vera.unwrap();
        assert_eq!(vera.grpc_address, "http://localhost:9090");
        assert_eq!(vera.chain_id, "vera-test");
    }
}

#[test]
fn conflicting_mobile_provider_names_are_rejected() {
    let value = serde_json::json!({ "vera": null, "sourcehub": null });
    assert!(serde_json::from_value::<MobileNodeConfig>(value).is_err());
}
