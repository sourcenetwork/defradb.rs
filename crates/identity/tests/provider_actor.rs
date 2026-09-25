use identity::Did;

#[test]
fn provider_actor_round_trips_without_becoming_a_signing_key() {
    let value = format!("did:opk:{}", "ab".repeat(32));
    let actor = Did::new(&value).unwrap();
    assert_eq!(actor.as_str(), value);
    assert_eq!(actor.key_portion(), None);
    assert!(!actor.is_wildcard());
    assert_eq!(value.parse::<Did>().unwrap(), actor);
    assert_eq!(
        serde_json::from_str::<Did>(&serde_json::to_string(&actor).unwrap()).unwrap(),
        actor
    );
    assert_eq!(Did::wildcard().key_portion(), None);
    assert_eq!(
        Did::new("did:key:zExample").unwrap().key_portion(),
        Some("zExample")
    );
}

#[test]
fn provider_actor_rejects_noncanonical_identifiers_on_every_parse_path() {
    for value in [
        "did:opk:".to_string(),
        format!("did:opk:{}", "a".repeat(63)),
        format!("did:opk:{}", "a".repeat(65)),
        format!("did:opk:{}", "A".repeat(64)),
        format!("did:opk:{}", "g".repeat(64)),
        format!("did:opk:{}#key", "a".repeat(64)),
        format!("did:opk:{} ", "a".repeat(64)),
        format!("did:other:{}", "a".repeat(64)),
        "*".to_string(),
    ] {
        assert!(Did::new(&value).is_err(), "{value}");
        assert!(value.parse::<Did>().is_err(), "{value}");
        assert!(
            serde_json::from_str::<Did>(&serde_json::to_string(&value).unwrap()).is_err(),
            "{value}"
        );
    }
}
