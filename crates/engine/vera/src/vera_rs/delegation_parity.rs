//! What `vera_delegation` defines in place of vera.rs's client is the
//! client's own: the ACP precompile's address and how a policy identifier is
//! parsed.

const POLICY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

#[test]
fn the_acp_precompile_is_the_one_vera_rs_client_names() {
    assert_eq!(vera_delegation::abi::ACP_ADDRESS, vera_client::ACP_ADDRESS);
}

#[test]
fn a_policy_identifier_parses_as_vera_rs_client_parses_it() {
    let prefixed = format!("0x{POLICY}");
    let long = format!("{POLICY}00");
    for text in [POLICY, prefixed.as_str()] {
        assert_eq!(
            vera_delegation::parse_policy_id(text).unwrap(),
            vera_client::parse_policy_id(text).unwrap(),
            "{text}"
        );
    }
    for text in ["", "0x", "not a policy", "0x0", &POLICY[2..], long.as_str()] {
        assert_eq!(
            vera_delegation::parse_policy_id(text).map_err(|e| e.to_string()),
            vera_client::parse_policy_id(text).map_err(|e| e.to_string()),
            "{text}"
        );
    }
}
