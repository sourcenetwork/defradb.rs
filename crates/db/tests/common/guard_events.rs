//! The trace events `DB::collection_read_guard` and
//! `DB::collection_write_guards` emit around their waits, recorded so a test
//! can prove a task reached a collection lock and was released by a drop,
//! rather than that it was slow.
//!
//! The recorder is the process-wide subscriber, not a thread-local one:
//! tracing caches a callsite's interest once per process, computed on the
//! thread that hits it first, and with a single registered dispatcher that
//! computation uses that thread's default. A thread-local recorder therefore
//! misses any callsite another test in the binary reaches first. Events are
//! keyed by collection id so parallel tests cannot be mistaken for each
//! other.

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

const GUARD_TARGET: &str = "db::collection::locks";

pub const READ_WAITING: &str = "waiting for the collection guard";
pub const READ_HOLDING: &str = "holding the collection guard";
pub const WRITE_WAITING: &str = "waiting for the collection write guard";
pub const WRITE_HOLDING: &str = "holding the collection write guard";

/// How often every guard event was emitted anywhere in this process, counted
/// per (collection id, message).
pub struct Recorder(HopscotchMap<(String, String), Arc<AtomicUsize>, RandomState>);

impl Recorder {
    pub fn count(&self, collection_id: &str, message: &str) -> usize {
        self.0
            .get(&(collection_id.to_string(), message.to_string()))
            .map_or(0, |count| count.load(Ordering::Acquire))
    }
}

#[derive(Default)]
struct Fields {
    collection_id: String,
    message: String,
}

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "collection_id" => self.collection_id = format!("{value:?}"),
            _ => {}
        }
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == GUARD_TARGET
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0
            .get_or_insert(
                (fields.collection_id, fields.message),
                Arc::new(AtomicUsize::new(0)),
            )
            .fetch_add(1, Ordering::AcqRel);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

pub fn recorder() -> Arc<Recorder> {
    static RECORDER: OnceLock<Arc<Recorder>> = OnceLock::new();
    RECORDER
        .get_or_init(|| {
            let recorder = Arc::new(Recorder(HopscotchMap::with_hasher(RandomState::default())));
            tracing::subscriber::set_global_default(recorder.clone())
                .expect("this test binary installs no other global subscriber");
            recorder
        })
        .clone()
}
