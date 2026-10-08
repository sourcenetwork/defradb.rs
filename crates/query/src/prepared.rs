use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use document::{Document, WritePreparation};
use rapidhash::RapidHashMap;

/// Immutable write inputs shared by attempts of one mutation request.
#[derive(Debug, Clone)]
pub struct PreparedMutations {
    pub request_time: DateTime<FixedOffset>,
    pub mutations: RapidHashMap<String, PreparedMutation>,
}

#[derive(Debug, Clone, Default)]
pub struct PreparedMutation {
    pub creates: Vec<Document>,
    pub updates: RapidHashMap<String, Arc<WritePreparation>>,
    pub doc_ids: Option<Vec<String>>,
}

pub(crate) fn prepared_doc_id(write: &WritePreparation) -> document::DocID {
    document::DocID::new_v0_from_seed(&format!(
        "key-preparation/{}/{}",
        write.collection_short_id, write.doc_short_id
    ))
}
