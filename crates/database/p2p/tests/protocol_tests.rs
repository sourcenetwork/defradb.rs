use p2p::{BASE_PROTOCOL_ID, CODE, MESSAGE_VERSION, NAME, VERSION};

#[test]
fn base_protocol_id_uses_name_and_version() {
    assert_eq!(BASE_PROTOCOL_ID, format!("/{NAME}/{VERSION}"));
}

#[test]
fn multicodec_code_matches_go() {
    assert_eq!(CODE, 961);
}

#[test]
fn message_version_matches_go() {
    assert_eq!(MESSAGE_VERSION, "/defradb/0.0.1");
}
