use super::source_name;

#[test]
fn source_names_round_trip_their_hex_objects() {
    assert_eq!(
        source_name("source.746f6f6c733a6f6e65").as_deref(),
        Some("tools:one")
    );
    assert_eq!(source_name("singleton"), None);
    assert_eq!(source_name("source.7"), None);
}
