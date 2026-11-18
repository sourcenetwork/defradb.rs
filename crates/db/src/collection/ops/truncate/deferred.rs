//! Truncating what a deferred index keeps besides its entries: its position
//! records and its pending log.

use std::sync::Arc;

use async_trait::async_trait;

use super::*;
use crate::index::pages::{Page, PAGE_ITEMS};

impl<S: Store> crate::database::DB<S> {
    /// Removes the position records and the pending log of every index of
    /// `collection`, one page per transaction.
    pub(super) async fn truncate_deferred_indexes(
        self: &Arc<Self>,
        collection: &Collection,
    ) -> Result<()>
    where
        S: 'static,
    {
        let short_id = collection.resolved_root_id();
        for index in collection.write_indexes() {
            let (mut size, mut after) = (PAGE_ITEMS, None);
            loop {
                let page = DeferredPage {
                    short_id,
                    index_id: index.id,
                    after: after.take(),
                };
                let (exhausted, last) = self.run_page(page, &mut size).await?;
                if exhausted {
                    break;
                }
                after = last;
            }
        }
        Ok(())
    }
}

/// One page of position records after `after`, and the log once none is left.
struct DeferredPage {
    short_id: u32,
    index_id: u32,
    after: Option<Vec<u8>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Page for DeferredPage {
    type Output = (bool, Option<Vec<u8>>);

    async fn run(
        &mut self,
        txn: &mut datastore::BasicTxn,
        size: usize,
    ) -> Result<(bool, Option<Vec<u8>>)> {
        let (removed, last) = crate::index::deferred::remove_page(
            &txn.datastore(),
            self.short_id,
            self.index_id,
            self.after.as_deref(),
            size,
        )
        .await?;
        Ok((removed < size, last))
    }
}
