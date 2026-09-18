//! Channel-based event bus implementation.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_channel::{Sender, TrySendError};
use kovan::Atom;
use rapidhash::{HashMapExt, RapidHashMap};

use crate::bus::Bus;
use crate::document_changes::{ChangePublisher, DocumentChangeSubscription};
use crate::event::{EventName, Message};
use crate::subscription::Subscription;

/// Configuration for the channel-based event bus.
#[derive(Debug, Clone)]
pub struct ChannelBusConfig {
    /// Buffer size for raw event channels, and the maximum number of distinct
    /// pending documents for current-state observers.
    /// When the buffer is full, new messages are dropped with a warning.
    /// Default: 4096
    pub event_buffer_size: usize,
    /// Whether to send a resync signal when messages are dropped due to buffer overflow.
    /// When enabled, a special "resync_needed" flag is tracked per subscriber.
    /// Default: true
    pub signal_resync_on_overflow: bool,
}

impl Default for ChannelBusConfig {
    fn default() -> Self {
        Self {
            event_buffer_size: 4096,
            signal_resync_on_overflow: true,
        }
    }
}

impl ChannelBusConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the event buffer size.
    pub fn with_event_buffer_size(mut self, size: usize) -> Self {
        self.event_buffer_size = size;
        self
    }
}

/// Subscriber entry with channel and event filter.
#[derive(Clone)]
struct Subscriber {
    /// Sender channel for messages.
    sender: Sender<Message>,
    /// Events this subscriber is interested in.
    events: Vec<EventName>,
    /// Shared count of messages dropped due to buffer overflow.
    /// Used to signal clients that they may need to resync.
    dropped_count: std::sync::Arc<AtomicU64>,
}

/// Channel-based event bus using runtime-neutral async channels.
///
/// This implementation uses bounded channels per subscriber.
/// Messages are fan-out to all matching subscribers.
/// When a subscriber's buffer is full, messages are dropped (non-blocking).
pub struct ChannelBus {
    /// Counter for generating unique subscription IDs.
    next_id: AtomicU64,
    /// Active subscribers indexed by ID.
    subscribers: Atom<RapidHashMap<u64, Subscriber>>,
    document_observers: Atom<RapidHashMap<u64, ChangePublisher>>,
    /// Whether the bus is closed.
    closed: AtomicBool,
    /// Configuration for the bus.
    config: ChannelBusConfig,
}

impl ChannelBus {
    /// Create a new channel-based event bus with default configuration.
    pub fn new() -> Self {
        Self::with_config(ChannelBusConfig::default())
    }

    /// Create a new channel-based event bus with custom configuration.
    pub fn with_config(config: ChannelBusConfig) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            subscribers: Atom::new(RapidHashMap::new()),
            document_observers: Atom::new(RapidHashMap::new()),
            closed: AtomicBool::new(false),
            config,
        }
    }

    /// Get the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.load().len() + self.document_observers.load().len()
    }

    /// Get the current configuration.
    pub fn config(&self) -> &ChannelBusConfig {
        &self.config
    }
}

impl Default for ChannelBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for ChannelBus {
    fn publish(&self, msg: Message) {
        if self.closed.load(Ordering::Acquire) {
            tracing::debug!(event = %msg.name, "Bus closed, dropping message");
            return;
        }

        if let Some(update) = msg.as_update() {
            self.publish_to_observers(|observer| observer.publish(update));
        }

        self.publish_raw(msg);
    }

    fn publish_batch(&self, messages: Vec<Message>) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        self.publish_to_observers(|observer| {
            observer.publish_batch(messages.iter().filter_map(Message::as_update))
        });
        for message in messages {
            self.publish_raw(message);
        }
    }

    fn subscribe(&self, events: &[EventName]) -> Subscription {
        self.subscribe_raw(events)
    }

    fn unsubscribe(&self, sub_id: u64) {
        // Explicitly close the channel: removal only drops this map's clone,
        // and the map's own epoch reclamation can keep the original sender
        // alive well after this call returns, so a waiting `recv` would
        // never otherwise see it close.
        self.document_observers.rcu(|old| {
            let mut m = old.clone();
            if let Some(observer) = m.remove(&sub_id) {
                observer.close();
            }
            m
        });
        self.subscribers.rcu(|old| {
            let mut m = old.clone();
            if let Some(subscriber) = m.remove(&sub_id) {
                subscriber.sender.close();
            }
            m
        });
        tracing::debug!(sub_id, "Unsubscribed");
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // Same reasoning as `unsubscribe`: close every channel explicitly
        // before dropping the maps that held them.
        let old_subscribers = self.subscribers.swap(RapidHashMap::new());
        for subscriber in old_subscribers.values() {
            subscriber.sender.close();
        }
        let old_observers = self.document_observers.swap(RapidHashMap::new());
        for observer in old_observers.values() {
            observer.close();
        }
        tracing::info!("Event bus closed");
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn subscribe_document_changes(&self) -> DocumentChangeSubscription {
        if self.closed.load(Ordering::Acquire) {
            return DocumentChangeSubscription::closed();
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (publisher, subscription) =
            DocumentChangeSubscription::new(id, self.config.event_buffer_size);

        // Re-checked inside the closure: if `close()` clears the map between
        // our check above and this commit, the retry sees `closed` true and
        // skips the insert, so no observer survives a closed bus.
        let inserted = Cell::new(false);
        self.document_observers.rcu(|old| {
            if self.closed.load(Ordering::Acquire) {
                inserted.set(false);
                return old.clone();
            }
            inserted.set(true);
            let mut m = old.clone();
            m.insert(id, publisher.clone());
            m
        });

        if inserted.get() {
            subscription
        } else {
            DocumentChangeSubscription::closed()
        }
    }
}

impl ChannelBus {
    /// Deliver messages to document-change observers, pruning any found closed.
    fn publish_to_observers(&self, mut deliver: impl FnMut(&ChangePublisher)) {
        let observers = self.document_observers.load();
        let mut dead: Vec<u64> = Vec::new();
        for (id, observer) in observers.iter() {
            if observer.is_closed() {
                dead.push(*id);
                continue;
            }
            deliver(observer);
        }
        drop(observers);

        if !dead.is_empty() {
            self.document_observers.rcu(|old| {
                let mut m = old.clone();
                for id in &dead {
                    m.remove(id);
                }
                m
            });
        }
    }

    fn publish_raw(&self, msg: Message) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        // Collect dead subscriber IDs for lazy cleanup
        let mut dead_subs: Vec<u64> = Vec::new();

        let subscribers = self.subscribers.load();
        let sub_count = subscribers.len();
        let mut delivered = 0;
        let mut dropped = 0;
        let mut buffer_full = 0;

        for (id, subscriber) in subscribers.iter() {
            // Check if subscriber is interested in this event
            let interested = subscriber.events.iter().any(|e| e.matches(&msg.name));
            if !interested {
                continue;
            }

            // Try to send (non-blocking) - use try_send to avoid blocking
            match subscriber.sender.try_send(msg.clone()) {
                Ok(()) => delivered += 1,
                Err(TrySendError::Full(_)) => {
                    // Buffer full - track dropped count for resync signaling
                    let prev_dropped = subscriber.dropped_count.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        sub_id = *id,
                        event = %msg.name,
                        total_dropped = prev_dropped + 1,
                        "Subscriber buffer full, dropping message (client may need to resync)"
                    );
                    buffer_full += 1;
                }
                Err(TrySendError::Closed(_)) => {
                    // Subscriber channel closed, mark for cleanup
                    tracing::debug!(
                        sub_id = *id,
                        "Subscriber channel closed, marking for cleanup"
                    );
                    dead_subs.push(*id);
                    dropped += 1;
                }
            }
        }

        // Release the snapshot before the cleanup RCU below.
        drop(subscribers);

        // Lazy cleanup: remove dead subscribers
        if !dead_subs.is_empty() {
            self.subscribers.rcu(|old| {
                let mut m = old.clone();
                for id in &dead_subs {
                    m.remove(id);
                }
                m
            });
            tracing::info!(
                cleaned_up = dead_subs.len(),
                remaining = self.subscribers.load().len(),
                "Cleaned up dead subscribers"
            );
        }

        if matches!(
            msg.name,
            EventName::MergeComplete | EventName::ReplicatorCompleted
        ) {
            tracing::debug!(
                event = %msg.name,
                sub_count,
                delivered,
                dropped,
                buffer_full,
                "Published event"
            );
        }
    }

    fn subscribe_raw(&self, events: &[EventName]) -> Subscription {
        if self.closed.load(Ordering::Acquire) {
            // Return a subscription with a closed channel
            let (_tx, rx) = async_channel::bounded(1);
            return Subscription::new(0, rx);
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = async_channel::bounded(self.config.event_buffer_size);

        // Create shared dropped counter for both Subscriber and Subscription
        let dropped_count = std::sync::Arc::new(AtomicU64::new(0));

        let subscriber = Subscriber {
            sender: tx,
            events: events.to_vec(),
            dropped_count: dropped_count.clone(),
        };

        // Re-checked inside the closure: see `subscribe_document_changes`.
        let inserted = Cell::new(false);
        self.subscribers.rcu(|old| {
            if self.closed.load(Ordering::Acquire) {
                inserted.set(false);
                return old.clone();
            }
            inserted.set(true);
            let mut m = old.clone();
            m.insert(id, subscriber.clone());
            m
        });

        if !inserted.get() {
            let (_tx, rx) = async_channel::bounded(1);
            return Subscription::new(0, rx);
        }

        tracing::debug!(
            sub_id = id,
            events = ?events,
            buffer_size = self.config.event_buffer_size,
            "New subscription"
        );

        Subscription::with_dropped_counter(id, rx, dropped_count)
    }
}
