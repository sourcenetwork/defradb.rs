//! Bounded channels between the KMS's tasks: a closed receiver hands a sent
//! value back, and a sender can wait for every receiver to close. The same
//! queue on every target, runtime-free.

/// Sending side of a bounded channel.
pub struct Sender<T> {
    inner: async_channel::Sender<T>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Send a value, returning it when the receiver is closed.
    pub async fn send(&self, value: T) -> Result<(), T> {
        self.inner.send(value).await.map_err(|error| error.0)
    }

    /// Wait until every receiver has been dropped.
    pub async fn closed(&self) {
        self.inner.closed().await;
    }
}

/// Receiving side of a bounded channel.
pub struct Receiver<T> {
    inner: async_channel::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive the next value, or `None` once every sender is closed.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await.ok()
    }
}

pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = async_channel::bounded(capacity);
    (Sender { inner: sender }, Receiver { inner: receiver })
}
