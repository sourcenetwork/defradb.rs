use p2p::MESSAGE_VERSION;

#[test]
fn message_version_matches_go() {
    assert_eq!(MESSAGE_VERSION, "/defradb/0.0.1");
}
