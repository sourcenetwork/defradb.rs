//! A trained engine's fixed state, loaded once and copied with the engine.

use std::ops::Deref;

use regolith::sync::OnceCell;

/// A write-once cell whose clone starts with the value the original holds.
#[derive(Debug)]
pub(crate) struct TrainedCache<T>(OnceCell<T>);

impl<T> TrainedCache<T> {
    pub(crate) const fn new() -> Self {
        Self(OnceCell::new())
    }
}

impl<T> Deref for TrainedCache<T> {
    type Target = OnceCell<T>;

    fn deref(&self) -> &OnceCell<T> {
        &self.0
    }
}

impl<T: Clone> Clone for TrainedCache<T> {
    fn clone(&self) -> Self {
        let cell = OnceCell::new();
        if let Some(value) = self.0.get() {
            let _ = cell.set(value.clone());
        }
        Self(cell)
    }
}
