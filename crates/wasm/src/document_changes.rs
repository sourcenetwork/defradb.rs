//! Document change notifications, for a page that keeps a view current.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::bindings::to_js;

/// Documents changing in one client, locally or by replication.
///
/// Each batch names documents whose current state changed since the last one,
/// whether by a write in this client or by a merge from a peer. It carries no
/// values: read the documents again. Repeated changes to one document coalesce
/// into one entry, and when too many distinct documents change at once the
/// batch sets `resync_required` instead of naming them, so a view should reload
/// everything it shows.
///
/// # Example (JavaScript)
///
/// ```javascript
/// const changes = client.document_changes();
/// for (let batch; (batch = await changes.next()) !== null; ) {
///   // batch.changes: [{ collection_id, doc_id, local }], batch.resync_required
/// }
/// ```
#[wasm_bindgen]
pub struct DocumentChanges {
    subscription: events::DocumentChangeSubscription,
}

#[derive(Serialize)]
struct Change<'a> {
    collection_id: &'a str,
    doc_id: &'a str,
    /// At least one of the coalesced changes was a write in this client.
    local: bool,
}

#[derive(Serialize)]
struct Batch<'a> {
    changes: Vec<Change<'a>>,
    resync_required: bool,
    updates: u64,
}

impl DocumentChanges {
    pub(crate) fn new(subscription: events::DocumentChangeSubscription) -> Self {
        Self { subscription }
    }
}

#[wasm_bindgen]
impl DocumentChanges {
    /// The next batch of changes, or `null` once the client has closed.
    #[wasm_bindgen(js_name = next)]
    pub async fn next_batch(&mut self) -> std::result::Result<JsValue, JsValue> {
        let Some(batch) = self.subscription.recv().await else {
            return Ok(JsValue::NULL);
        };
        let view = Batch {
            changes: batch
                .changes
                .iter()
                .map(|change| Change {
                    collection_id: &change.collection_id,
                    doc_id: &change.doc_id,
                    local: change.has_local_write,
                })
                .collect(),
            resync_required: batch.resync_required,
            updates: batch.updates,
        };
        to_js(&view).map_err(Into::into)
    }
}
