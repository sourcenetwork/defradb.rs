//! The channel documents flow through between the stages of one call: JSON
//! bytes, bounded by bytes, since documents vary in size.

use std::sync::Arc;

use regolith::sync::{unbounded, Semaphore, UnboundedReceiver, UnboundedSender};

/// The bytes one channel may hold. A document larger than this still passes,
/// alone.
pub(super) const CHANNEL_BYTES: usize = 1 << 20;

/// A channel of JSON documents holding at most `CHANNEL_BYTES` bytes; a send
/// waits for room. Every document counts as at least one byte, so the count
/// of documents is bounded too.
pub(super) fn channel() -> (ByteSender, ByteReceiver) {
    let (tx, rx) = unbounded();
    let room = Arc::new(Semaphore::new(CHANNEL_BYTES));
    (
        ByteSender {
            tx,
            room: Arc::clone(&room),
        },
        ByteReceiver { rx, room },
    )
}

/// What a document takes of a channel's room.
fn charge(document: &[u8]) -> usize {
    document.len().clamp(1, CHANNEL_BYTES)
}

/// The sending half: dropping every sender ends the stream.
pub(super) struct ByteSender {
    tx: UnboundedSender<Vec<u8>>,
    room: Arc<Semaphore>,
}

impl ByteSender {
    /// Waits for room, then sends `document`.
    pub(super) async fn send(&self, document: Vec<u8>) {
        self.room.acquire(charge(&document)).await.forget();
        self.tx.send_async(document).await;
    }
}

/// The receiving half.
#[derive(Clone)]
pub(super) struct ByteReceiver {
    rx: UnboundedReceiver<Vec<u8>>,
    room: Arc<Semaphore>,
}

impl ByteReceiver {
    /// The next document, or `None` once every sender is gone and the channel
    /// is empty.
    pub(super) async fn recv(&self) -> Option<Vec<u8>> {
        let document = self.rx.recv_async().await?;
        self.room.add_permits(charge(&document));
        Some(document)
    }
}
