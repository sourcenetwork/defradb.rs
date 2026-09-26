use events::{Bus, ChannelBus, EventName, Message, ReplicatorCompletedData};

#[tokio::test]
async fn completion_keeps_each_peers_replay_outcome() {
    let bus = ChannelBus::new();
    let mut subscription = bus.subscribe(&[EventName::ReplicatorCompleted]);
    let outcomes = [
        ReplicatorCompletedData {
            peer_id: "first".into(),
            collections: vec!["Note".into()],
            skipped: false,
            error: None,
        },
        ReplicatorCompletedData {
            peer_id: "second".into(),
            collections: vec!["User".into()],
            skipped: true,
            error: None,
        },
        ReplicatorCompletedData {
            peer_id: "third".into(),
            collections: vec!["Note".into()],
            skipped: false,
            error: Some("replay failed".into()),
        },
    ];
    for data in &outcomes {
        bus.publish(Message::replicator_completed_with_data(data.clone()));
    }
    for expected in &outcomes {
        let message = subscription.recv().await.unwrap();
        assert_eq!(message.name, EventName::ReplicatorCompleted);
        assert_eq!(message.as_replicator_completed(), Some(expected));
    }
    let legacy = Message::replicator_completed();
    assert_eq!(legacy.name, EventName::ReplicatorCompleted);
    assert!(legacy.as_replicator_completed().is_none());
}
