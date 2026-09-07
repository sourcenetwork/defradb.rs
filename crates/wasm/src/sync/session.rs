use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use defra_core::browser_sync::{
    BrowserSyncDocument, BrowserSyncPull, BrowserSyncRequest, BrowserSyncResponse,
    MAX_SYNC_BODY_BYTES, MAX_SYNC_DOCUMENTS_PER_REQUEST, MAX_SYNC_PAGE_SIZE, MAX_SYNC_PULL_DOC_IDS,
};
use events::Bus;
use futures::channel::oneshot;
use futures::future::{AbortHandle, Abortable};
use futures::lock::Mutex;
use storage::RegolithStore;
use wasm_bindgen_futures::spawn_local;

use crate::error::{Result, WasmError};

use super::http::SyncHttpClient;
use super::sse::SseStream;

const INITIAL_RECONNECT_DELAY_MS: u32 = 1_000;
const MAX_RECONNECT_DELAY_MS: u32 = 30_000;
const EMPTY_PUSH_REQUEST_BYTES: usize = b"{\"documents\":[]}".len();

/// What an exchange costs before a document or a pull ID is in it.
/// `incremental_sync_size_matches_serialized_request` holds it against a real
/// serialization.
const EMPTY_SYNC_REQUEST_BYTES: usize =
    b"{\"documents\":[],\"pull\":{\"doc_ids\":[],\"limit\":64}}".len();

pub(crate) struct SyncTask {
    abort: AbortHandle,
    finished: oneshot::Receiver<()>,
}

impl SyncTask {
    pub(crate) async fn stop(mut self) {
        self.abort.abort();
        let _ = (&mut self.finished).await;
    }
}

impl Drop for SyncTask {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

pub(crate) async fn start(
    database: Arc<db::DB<RegolithStore>>,
    event_bus: &Arc<events::ChannelBus>,
    server_url: &str,
    auth_token: Option<String>,
) -> Result<SyncTask> {
    let session = Rc::new(SyncSession {
        engine: db::merge::BrowserSyncEngine::new(database),
        http: SyncHttpClient::new(server_url, auth_token)?,
        exchange_lock: Mutex::new(()),
        full_sync_lock: Mutex::new(()),
    });
    let subscription = event_bus.subscribe(&[events::EventName::Update]);
    let subscription_id = subscription.id();
    let events = match session.http.events().await {
        Ok(events) => events,
        Err(error) => {
            event_bus.unsubscribe(subscription_id);
            return Err(error);
        }
    };
    if let Err(error) = session.full_sync().await {
        event_bus.unsubscribe(subscription_id);
        return Err(error);
    }

    let (abort, registration) = AbortHandle::new_pair();
    let (finished_tx, finished) = oneshot::channel();
    let event_bus = Arc::clone(event_bus);
    spawn_local(async move {
        let future = async move {
            futures::future::join(
                Rc::clone(&session).run_local(subscription),
                session.run_remote(events),
            )
            .await;
        };
        let _ = Abortable::new(future, registration).await;
        event_bus.unsubscribe(subscription_id);
        let _ = finished_tx.send(());
    });
    Ok(SyncTask { abort, finished })
}

struct SyncSession {
    engine: db::merge::BrowserSyncEngine<RegolithStore>,
    http: SyncHttpClient,
    exchange_lock: Mutex<()>,
    full_sync_lock: Mutex<()>,
}

impl SyncSession {
    async fn full_sync(&self) -> Result<()> {
        let _guard = self.full_sync_lock.lock().await;
        self.push_all_documents().await?;
        self.pull_all_documents().await
    }

    async fn push_all_documents(&self) -> Result<()> {
        let refs = self.engine.document_refs().await.map_err(engine_error)?;
        let mut documents = Vec::new();
        let mut serialized_size = EMPTY_PUSH_REQUEST_BYTES;
        for document_ref in refs {
            let loaded = match self.engine.load_document(&document_ref).await {
                Ok(loaded) => loaded,
                // Cannot be represented as a sync payload — too large, or too
                // many blocks or roots — so it can never be pushed. Failing
                // here would abort the whole push and leave recover_full_sync
                // retrying forever; skip it like the request-size check below.
                Err(error @ db::merge::browser_sync::BrowserSyncError::TooLarge(_)) => {
                    warn(&format!(
                        "browser sync skipped document {} because it cannot be represented as a sync payload: {error}",
                        document_ref.doc_id
                    ));
                    continue;
                }
                Err(error) => return Err(engine_error(error)),
            };
            let Some(document) = loaded else {
                continue;
            };

            let document_size = serde_json::to_vec(&document)?.len();
            let single_document_size = EMPTY_PUSH_REQUEST_BYTES + document_size;
            if single_document_size > MAX_SYNC_BODY_BYTES {
                warn(&format!(
                    "browser sync skipped document {} because it exceeds the sync request limit",
                    document.doc_id
                ));
                continue;
            }

            let next_size = serialized_size + document_size + usize::from(!documents.is_empty());
            if next_size > MAX_SYNC_BODY_BYTES {
                self.push_documents(std::mem::take(&mut documents)).await?;
                serialized_size = EMPTY_PUSH_REQUEST_BYTES;
            }

            serialized_size += document_size + usize::from(!documents.is_empty());
            documents.push(document);
            if documents.len() == MAX_SYNC_DOCUMENTS_PER_REQUEST {
                self.push_documents(std::mem::take(&mut documents)).await?;
                serialized_size = EMPTY_PUSH_REQUEST_BYTES;
            }
        }
        if !documents.is_empty() {
            self.push_documents(documents).await?;
        }
        Ok(())
    }

    async fn push_documents(&self, documents: Vec<BrowserSyncDocument>) -> Result<()> {
        self.exchange(BrowserSyncRequest {
            documents,
            pull: None,
        })
        .await?;
        Ok(())
    }

    async fn pull_all_documents(&self) -> Result<()> {
        let mut cursor = None;
        loop {
            let response = self
                .exchange(BrowserSyncRequest {
                    documents: Vec::new(),
                    pull: Some(BrowserSyncPull {
                        doc_ids: Vec::new(),
                        cursor: cursor.clone(),
                        limit: Some(MAX_SYNC_PAGE_SIZE as u16),
                    }),
                })
                .await?;
            match response.next_cursor {
                Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
                Some(_) => {
                    return Err(WasmError::Sync(
                        "server returned a non-advancing sync cursor".into(),
                    ))
                }
                None => return Ok(()),
            }
        }
    }

    /// Push what is local and pull what is not, in as few exchanges as the wire
    /// limits allow. A burst of writes raises one update event per document and
    /// they arrive together, so one exchange each would cost a round trip per
    /// document.
    async fn sync_documents(&self, doc_ids: &[String]) -> Result<()> {
        let mut batch = SyncBatch::default();
        for doc_id in doc_ids {
            let id_bytes = serde_json::to_vec(doc_id)?.len();
            let mut document = self.load_push_document(doc_id).await?;
            let mut document_bytes = match document.as_ref() {
                Some(document) => Some(serde_json::to_vec(document)?.len()),
                None => None,
            };

            // Over the limit alone, so no request can carry it. Drop it from
            // the push; the pull still names it.
            if !SyncBatch::default().fits(id_bytes, document_bytes) {
                warn(&format!(
                    "browser sync skipped document {doc_id} because it exceeds the sync request limit"
                ));
                document = None;
                document_bytes = None;
            }

            if !batch.is_empty() && !batch.fits(id_bytes, document_bytes) {
                self.exchange_batch(batch.take()).await?;
            }
            batch.accept(doc_id.clone(), document, id_bytes, document_bytes);
        }
        if !batch.is_empty() {
            self.exchange_batch(batch.take()).await?;
        }
        Ok(())
    }

    /// The push payload for a document, or `None` when there is nothing local
    /// to push: it is not held here, or it is too large to represent. Failing
    /// instead would force a full sync on every update touching it.
    async fn load_push_document(&self, doc_id: &str) -> Result<Option<BrowserSyncDocument>> {
        let Some(document_ref) = self
            .engine
            .document_ref(doc_id)
            .await
            .map_err(engine_error)?
        else {
            return Ok(None);
        };
        match self.engine.load_document(&document_ref).await {
            Ok(document) => Ok(document),
            Err(error @ db::merge::browser_sync::BrowserSyncError::TooLarge(_)) => {
                warn(&format!(
                    "browser sync skipped document {doc_id} because it cannot be represented as a sync payload: {error}"
                ));
                Ok(None)
            }
            Err(error) => Err(engine_error(error)),
        }
    }

    /// One batch: its documents pushed once, then its pull followed to the end.
    /// A pull naming many documents can be cut short by the page limit, where a
    /// pull naming one never could, so later requests carry no documents.
    async fn exchange_batch(&self, batch: SyncBatch) -> Result<()> {
        let SyncBatch {
            mut documents,
            doc_ids,
            ..
        } = batch;
        let mut cursor = None;
        loop {
            let response = self
                .exchange(BrowserSyncRequest {
                    documents: std::mem::take(&mut documents),
                    pull: Some(BrowserSyncPull {
                        doc_ids: doc_ids.clone(),
                        cursor: cursor.clone(),
                        limit: Some(MAX_SYNC_PAGE_SIZE as u16),
                    }),
                })
                .await?;
            match response.next_cursor {
                Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
                Some(_) => {
                    return Err(WasmError::Sync(
                        "server returned a non-advancing sync cursor".into(),
                    ))
                }
                None => return Ok(()),
            }
        }
    }

    async fn exchange(&self, request: BrowserSyncRequest) -> Result<BrowserSyncResponse> {
        let _guard = self.exchange_lock.lock().await;
        let response = self.http.sync(&request).await?;
        for document in &response.documents {
            self.engine
                .apply_document(document, "server")
                .await
                .map_err(engine_error)?;
        }
        Ok(response)
    }

    async fn run_local(self: Rc<Self>, mut subscription: events::Subscription) {
        while let Some(message) = subscription.recv().await {
            if subscription.check_and_reset_dropped() > 0 {
                if let Err(error) = self.full_sync().await {
                    self.recover_full_sync("recovering dropped local events", error)
                        .await;
                }
                continue;
            }

            let mut doc_ids = BTreeSet::new();
            collect_local_update(&message, &mut doc_ids);
            while let Ok(message) = subscription.try_recv() {
                collect_local_update(&message, &mut doc_ids);
            }
            let doc_ids = Vec::from_iter(doc_ids);
            if let Err(error) = self.sync_documents(&doc_ids).await {
                self.recover_full_sync(
                    &format!("syncing {} local document(s)", doc_ids.len()),
                    error,
                )
                .await;
            }
        }
    }

    async fn run_remote(self: Rc<Self>, mut events: SseStream) {
        loop {
            loop {
                match events.next_document_id().await {
                    Ok(Some(doc_id)) => {
                        // A burst arrives as one stream chunk, so what was
                        // decoded alongside this ID belongs in the same
                        // exchange.
                        let mut doc_ids = BTreeSet::from([doc_id]);
                        doc_ids.extend(events.take_decoded_document_ids());
                        let doc_ids = Vec::from_iter(doc_ids);
                        if let Err(error) = self.sync_documents(&doc_ids).await {
                            self.recover_full_sync(
                                &format!("syncing {} remote document(s)", doc_ids.len()),
                                error,
                            )
                            .await;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        warn(&format!("browser sync event stream closed: {error}"));
                        break;
                    }
                }
            }
            events = self.reconnect().await;
        }
    }

    async fn recover_full_sync(&self, context: &str, mut error: WasmError) {
        let mut delay = INITIAL_RECONNECT_DELAY_MS;
        loop {
            warn(&format!("browser sync failed while {context}: {error}"));
            gloo_timers::future::TimeoutFuture::new(delay).await;
            match self.full_sync().await {
                Ok(()) => return,
                Err(next_error) => error = next_error,
            }
            delay = delay.saturating_mul(2).min(MAX_RECONNECT_DELAY_MS);
        }
    }

    async fn reconnect(&self) -> SseStream {
        let mut delay = INITIAL_RECONNECT_DELAY_MS;
        loop {
            gloo_timers::future::TimeoutFuture::new(delay).await;
            match self.http.events().await {
                Ok(events) => match self.full_sync().await {
                    Ok(()) => return events,
                    Err(error) => warn(&format!("browser sync reconnect failed: {error}")),
                },
                Err(error) => warn(&format!("browser sync reconnect failed: {error}")),
            }
            delay = delay.saturating_mul(2).min(MAX_RECONNECT_DELAY_MS);
        }
    }
}

/// One exchange's worth of work, packed while documents are loaded one at a
/// time. The limits are the server's, and packing is kept apart from loading so
/// the rules can be checked without a node or a network.
struct SyncBatch {
    documents: Vec<BrowserSyncDocument>,
    doc_ids: Vec<String>,
    bytes: usize,
}

impl Default for SyncBatch {
    fn default() -> Self {
        Self {
            documents: Vec::new(),
            doc_ids: Vec::new(),
            bytes: EMPTY_SYNC_REQUEST_BYTES,
        }
    }
}

impl SyncBatch {
    fn is_empty(&self) -> bool {
        self.doc_ids.is_empty()
    }

    /// Whether a document and its pull ID can still join this batch. An ID with
    /// no document of its own still takes a place in the pull.
    fn fits(&self, id_bytes: usize, document_bytes: Option<usize>) -> bool {
        if self.doc_ids.len() == MAX_SYNC_PULL_DOC_IDS {
            return false;
        }
        if document_bytes.is_some() && self.documents.len() == MAX_SYNC_DOCUMENTS_PER_REQUEST {
            return false;
        }
        self.bytes + self.added_bytes(id_bytes, document_bytes) <= MAX_SYNC_BODY_BYTES
    }

    fn added_bytes(&self, id_bytes: usize, document_bytes: Option<usize>) -> usize {
        let id = id_bytes + usize::from(!self.doc_ids.is_empty());
        let document =
            document_bytes.map_or(0, |bytes| bytes + usize::from(!self.documents.is_empty()));
        id + document
    }

    fn accept(
        &mut self,
        doc_id: String,
        document: Option<BrowserSyncDocument>,
        id_bytes: usize,
        document_bytes: Option<usize>,
    ) {
        self.bytes += self.added_bytes(id_bytes, document_bytes);
        if let Some(document) = document {
            self.documents.push(document);
        }
        self.doc_ids.push(doc_id);
    }

    fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

fn collect_local_update(message: &events::Message, doc_ids: &mut BTreeSet<String>) {
    if let Some(update) = message.as_update() {
        if !update.is_relay && !update.doc_id.is_empty() {
            doc_ids.insert(update.doc_id.clone());
        }
    }
}

fn engine_error(error: db::merge::BrowserSyncError) -> WasmError {
    WasmError::Sync(error.to_string())
}

fn warn(message: &str) {
    web_sys::console::warn_1(&message.into());
}

#[cfg(test)]
mod tests {
    use defra_core::browser_sync::{
        BrowserSyncBlock, BrowserSyncDocument, BrowserSyncPull, BrowserSyncRequest,
        MAX_SYNC_BODY_BYTES, MAX_SYNC_DOCUMENTS_PER_REQUEST, MAX_SYNC_PAGE_SIZE,
        MAX_SYNC_PULL_DOC_IDS,
    };

    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{SyncBatch, EMPTY_PUSH_REQUEST_BYTES};

    fn document(doc_id: &str, data: &str) -> BrowserSyncDocument {
        BrowserSyncDocument {
            doc_id: doc_id.into(),
            collection_id: "collection".into(),
            roots: vec!["root".into()],
            blocks: vec![BrowserSyncBlock {
                cid: "block".into(),
                data: data.into(),
            }],
            relationships: Vec::new(),
        }
    }

    /// Offer a document the way `sync_documents` does, returning the batch its
    /// arrival flushed.
    fn offer(batch: &mut SyncBatch, document: BrowserSyncDocument) -> Option<SyncBatch> {
        let id_bytes = serde_json::to_vec(&document.doc_id).unwrap().len();
        let document_bytes = serde_json::to_vec(&document).unwrap().len();
        let flushed = (!batch.is_empty() && !batch.fits(id_bytes, Some(document_bytes)))
            .then(|| batch.take());
        batch.accept(
            document.doc_id.clone(),
            Some(document),
            id_bytes,
            Some(document_bytes),
        );
        flushed
    }

    #[wasm_bindgen_test]
    fn incremental_push_size_matches_serialized_request() {
        let documents = vec![
            BrowserSyncDocument {
                doc_id: "doc-one".into(),
                collection_id: "collection".into(),
                roots: vec!["root-one".into()],
                blocks: vec![BrowserSyncBlock {
                    cid: "block-one".into(),
                    data: "data-one".into(),
                }],
                relationships: Vec::new(),
            },
            BrowserSyncDocument {
                doc_id: "doc-two".into(),
                collection_id: "collection".into(),
                roots: vec![],
                blocks: vec![],
                relationships: Vec::new(),
            },
        ];
        let incremental_size = documents.iter().enumerate().fold(
            EMPTY_PUSH_REQUEST_BYTES,
            |size, (index, document)| {
                size + serde_json::to_vec(document).unwrap().len() + usize::from(index > 0)
            },
        );

        let request = BrowserSyncRequest {
            documents,
            pull: None,
        };
        assert_eq!(
            incremental_size,
            serde_json::to_vec(&request).unwrap().len()
        );
    }
    #[wasm_bindgen_test]
    fn incremental_sync_size_matches_serialized_request() {
        let mut batch = SyncBatch::default();
        for index in 0..3 {
            offer(&mut batch, document(&format!("doc-{index}"), "data"));
        }
        // The pull names every document in the batch and always asks for a
        // full page.
        let request = BrowserSyncRequest {
            documents: batch.documents.clone(),
            pull: Some(BrowserSyncPull {
                doc_ids: batch.doc_ids.clone(),
                cursor: None,
                limit: Some(MAX_SYNC_PAGE_SIZE as u16),
            }),
        };
        assert_eq!(batch.bytes, serde_json::to_vec(&request).unwrap().len());
    }

    #[wasm_bindgen_test]
    fn a_pull_id_without_a_document_costs_only_its_id() {
        let mut batch = SyncBatch::default();
        let doc_id = "doc-remote".to_string();
        let id_bytes = serde_json::to_vec(&doc_id).unwrap().len();
        batch.accept(doc_id.clone(), None, id_bytes, None);

        let request = BrowserSyncRequest {
            documents: Vec::new(),
            pull: Some(BrowserSyncPull {
                doc_ids: vec![doc_id],
                cursor: None,
                limit: Some(MAX_SYNC_PAGE_SIZE as u16),
            }),
        };
        assert_eq!(batch.bytes, serde_json::to_vec(&request).unwrap().len());
    }

    #[wasm_bindgen_test]
    fn a_batch_ends_at_the_document_limit() {
        let mut batch = SyncBatch::default();
        for index in 0..MAX_SYNC_DOCUMENTS_PER_REQUEST {
            assert!(offer(&mut batch, document(&format!("doc-{index}"), "data")).is_none());
        }
        let flushed = offer(&mut batch, document("doc-over", "data")).expect("batch is full");
        assert_eq!(flushed.documents.len(), MAX_SYNC_DOCUMENTS_PER_REQUEST);
        assert_eq!(batch.documents.len(), 1);
    }

    /// A document held only by the server has no push payload, so the pull
    /// limit binds before the document limit.
    #[wasm_bindgen_test]
    fn a_batch_ends_at_the_pull_id_limit() {
        let mut batch = SyncBatch::default();
        for index in 0..MAX_SYNC_PULL_DOC_IDS {
            let doc_id = format!("doc-{index}");
            let id_bytes = serde_json::to_vec(&doc_id).unwrap().len();
            assert!(batch.fits(id_bytes, None));
            batch.accept(doc_id, None, id_bytes, None);
        }
        assert!(!batch.fits(serde_json::to_vec("doc-over").unwrap().len(), None));
    }

    #[wasm_bindgen_test]
    fn a_batch_ends_at_the_body_limit() {
        let heavy = "x".repeat(MAX_SYNC_BODY_BYTES / 4);
        let mut batch = SyncBatch::default();
        for index in 0..3 {
            assert!(
                offer(&mut batch, document(&format!("doc-{index}"), &heavy)).is_none(),
                "three quarters of the body still fits"
            );
        }
        let flushed = offer(&mut batch, document("doc-over", &heavy)).expect("body is full");
        assert!(flushed.bytes <= MAX_SYNC_BODY_BYTES);
        assert_eq!(flushed.documents.len(), 3);
        assert_eq!(batch.documents.len(), 1);
    }
}
