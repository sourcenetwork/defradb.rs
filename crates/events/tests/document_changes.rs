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

#[test]
fn history_revisions_coalesce_without_changing_raw_events() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn distinct_document_overflow_is_one_bounded_resync_then_recovers() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn collection_identity_close_unsubscribe_and_other_events() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn publishing_races_with_draining_without_losing_a_wake() {
    workers::block_on(async {
        let bus = std::sync::Arc::new(ChannelBus::new());
        let mut changes = bus.subscribe_document_changes();
        let publisher = bus.clone();
        let task = workers::spawn(None, "test", move || async move {
            for _ in 0..10_000 {
                publisher.publish(update("c", "doc", true));
                workers::yield_now().await;
            }
            publisher.close();
        })
        .unwrap();
        let mut count = 0;
        while let Some(batch) = changes.recv().await {
            assert_eq!(batch.changes.len(), 1);
            count += batch.updates;
        }
        task.await.unwrap();
        assert_eq!(count, 10_000);
    })
    .unwrap()
}

#[test]
fn dropped_observers_are_cleaned_up() {
    let bus = ChannelBus::new();
    drop(bus.subscribe_document_changes());
    bus.publish(update("c", "d", true));
    assert_eq!(bus.subscriber_count(), 0);
}

#[test]
fn committed_batch_wakes_state_reader_once_and_preserves_raw_order() {
    workers::block_on(async {
        let bus = std::sync::Arc::new(ChannelBus::with_config(
            ChannelBusConfig::new().with_event_buffer_size(12_000),
        ));
        let mut changes = bus.subscribe_document_changes();
        let mut raw = bus.subscribe(&[EventName::Update]);
        let publisher = bus.clone();
        let task = workers::spawn(None, "test", move || async move {
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
        })
        .unwrap();
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
    })
    .unwrap()
}

#[test]
fn subscription_registration_cannot_escape_close() {
    workers::block_on(async {
        for _ in 0..100 {
            let bus = std::sync::Arc::new(ChannelBus::new());
            let barrier = std::sync::Arc::new(regolith::sync::Barrier::new(2));
            let registering_bus = bus.clone();
            let registering_barrier = barrier.clone();
            let thread = workers::spawn(None, "test", move || async move {
                registering_barrier.wait().await;
                (
                    registering_bus.subscribe(&[EventName::Update]),
                    registering_bus.subscribe_document_changes(),
                )
            })
            .unwrap();
            barrier.wait().await;
            bus.close();
            let (mut raw, mut changes) = thread.await.unwrap();
            assert!(matches!(
                raw.try_recv(),
                Err(events::TryRecvError::Disconnected)
            ));
            assert!(matches!(
                changes.try_recv(),
                Err(events::TryRecvError::Disconnected)
            ));
        }
    })
    .unwrap()
}

#[test]
fn collection_scope_ignores_other_collections() {
    workers::block_on(async {
        let bus = ChannelBus::new();
        let mut changes = bus.subscribe_collection_changes(vec!["watched".into()]);
        bus.publish(update("other", "noise", false));
        assert!(changes.try_recv().is_err());
        bus.publish(update("watched", "doc", false));
        let batch = changes.recv().await.unwrap();
        assert_eq!(batch.updates, 1);
        assert_eq!(batch.changes.len(), 1);
        assert_eq!(batch.changes[0].collection_id, "watched");
        assert_eq!(batch.changes[0].doc_id, "doc");
    })
    .unwrap()
}

#[test]
fn other_collections_do_not_exhaust_scoped_capacity() {
    workers::block_on(async {
        let bus = ChannelBus::with_config(ChannelBusConfig::new().with_event_buffer_size(2));
        let mut changes = bus.subscribe_collection_changes(vec!["watched".into()]);
        for id in 0..100 {
            bus.publish(update("busy", &id.to_string(), true));
        }
        bus.publish(update("watched", "doc", true));
        let batch = changes.recv().await.unwrap();
        assert!(!batch.resync_required);
        assert_eq!(batch.updates, 1);
        assert_eq!(batch.changes[0].doc_id, "doc");
    })
    .unwrap()
}
