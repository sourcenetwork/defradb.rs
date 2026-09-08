use events::{Bus, ChannelBus, ChannelBusConfig, EventName, Message, Update};

fn update(collection: &str, document: &str, relay: bool) -> Message {
    Message::update(Update::new(
        document.into(),
        cid::Cid::default(),
        collection.into(),
        vec![7; 100],
        false,
        relay,
    ))
}

#[tokio::test]
async fn history_revisions_coalesce_without_changing_raw_events() {
    let bus = ChannelBus::with_config(ChannelBusConfig::new().with_event_buffer_size(4));
    let mut changes = bus.subscribe_document_changes();
    let mut raw = bus.subscribe(&[EventName::Update]);
    for revision in 0..12_000 {
        let msg = update("collection", "document", revision != 100);
        bus.publish(msg);
        let raw_update = raw.recv().await.unwrap();
        assert_eq!(raw_update.as_update().unwrap().block.len(), 100);
    }
    let batch = changes.recv().await.unwrap();
    assert_eq!(batch.updates, 12_000);
    assert_eq!(batch.changes.len(), 1);
    assert!(batch.changes[0].has_local_write);
    assert!(!batch.resync_required);
    assert_eq!(raw.dropped_count(), 0);
    assert!(changes.try_recv().is_err());
}

#[tokio::test]
async fn distinct_document_overflow_is_one_bounded_resync_then_recovers() {
    let bus = ChannelBus::with_config(ChannelBusConfig::new().with_event_buffer_size(2));
    let mut changes = bus.subscribe_document_changes();
    for id in 0..12_000 {
        bus.publish(update("c", &id.to_string(), true));
    }
    let batch = changes.recv().await.unwrap();
    assert!(batch.resync_required);
    assert!(batch.changes.is_empty());
    assert_eq!(batch.updates, 12_000);
    // Arrival during the consumer's snapshot read must not be cleared by that read.
    bus.publish(update("c", "after-drain", true));
    let next = changes.recv().await.unwrap();
    assert!(!next.resync_required);
    assert_eq!(next.changes[0].doc_id, "after-drain");
    assert!(changes.try_recv().is_err());
}

#[tokio::test]
async fn collection_identity_close_unsubscribe_and_other_events() {
    let bus = ChannelBus::new();
    let mut changes = bus.subscribe_document_changes();
    bus.publish(Message::merge());
    assert!(changes.try_recv().is_err());
    bus.publish(update("first", "same-id", true));
    bus.publish(update("second", "same-id", true));
    assert_eq!(changes.recv().await.unwrap().changes.len(), 2);
    bus.unsubscribe(changes.id());
    assert!(changes.recv().await.is_none());
    let mut changes = bus.subscribe_document_changes();
    bus.close();
    assert!(changes.recv().await.is_none());
    assert!(bus.subscribe_document_changes().recv().await.is_none());
    assert!(events::NoOpBus::new()
        .subscribe_document_changes()
        .recv()
        .await
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publishing_races_with_draining_without_losing_a_wake() {
    let bus = std::sync::Arc::new(ChannelBus::new());
    let mut changes = bus.subscribe_document_changes();
    let publisher = bus.clone();
    let task = tokio::spawn(async move {
        for _ in 0..10_000 {
            publisher.publish(update("c", "doc", true));
            tokio::task::yield_now().await;
        }
        publisher.close();
    });
    let mut count = 0;
    while let Some(batch) = changes.recv().await {
        assert_eq!(batch.changes.len(), 1);
        count += batch.updates;
    }
    task.await.unwrap();
    assert_eq!(count, 10_000);
}

#[test]
fn dropped_observers_are_cleaned_up() {
    let bus = ChannelBus::new();
    drop(bus.subscribe_document_changes());
    bus.publish(update("c", "d", true));
    assert_eq!(bus.subscriber_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_batch_wakes_state_reader_once_and_preserves_raw_order() {
    let bus = std::sync::Arc::new(ChannelBus::with_config(
        ChannelBusConfig::new().with_event_buffer_size(12_000),
    ));
    let mut changes = bus.subscribe_document_changes();
    let mut raw = bus.subscribe(&[EventName::Update]);
    let publisher = bus.clone();
    let task = tokio::spawn(async move {
        publisher.publish_batch(
            (0_u32..12_000)
                .map(|revision| {
                    Message::update(Update::new(
                        "doc".into(),
                        cid::Cid::default(),
                        "c".into(),
                        revision.to_be_bytes().to_vec(),
                        false,
                        true,
                    ))
                })
                .collect(),
        );
    });
    let batch = changes.recv().await.unwrap();
    assert_eq!(batch.updates, 12_000);
    assert_eq!(batch.changes.len(), 1);
    assert!(!batch.resync_required);
    task.await.unwrap();
    assert!(changes.try_recv().is_err());
    for revision in 0_u32..12_000 {
        let message = raw.try_recv().unwrap();
        assert_eq!(
            message.as_update().unwrap().block.as_ref(),
            &revision.to_be_bytes()
        );
    }
    assert!(raw.try_recv().is_err());
    assert_eq!(raw.dropped_count(), 0);
}
