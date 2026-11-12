//! Where a paged reindex resumes: its stage and cursor, committed with every
//! page under the token of the owed record it settles. A record under another
//! token belongs to a reindex a later schema change superseded, so it reads as
//! the start. So does one written while other indexes were deferred: the
//! pages so far kept them the other way (`proofs/tla/DeferredIndexMode.tla`).

use datastore::NamespaceView;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Start of every reindex progress key in the system store; the collection ID
/// follows. Kept apart from the owed record, which writers read, so a page
/// moving the cursor conflicts with no writer.
const PROGRESS_PREFIX: &[u8] = b"/reindex/progress/";

/// The stage a reindex reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Stage {
    /// Removing the entries of every index that takes writes, in index ID
    /// order: the index with the lowest ID at or above `next_index`, from the
    /// key after `after`.
    Clear {
        next_index: u64,
        after: Option<Vec<u8>>,
    },
    /// Indexing the documents again in short ID order: those after `last`,
    /// below `fence` once one is taken after the clear committed.
    Walk { fence: Option<u64>, last: u64 },
}

impl Stage {
    pub(super) fn start() -> Self {
        Self::Clear {
            next_index: 0,
            after: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Progress {
    token: u64,
    stage: Stage,
    /// The IDs of the indexes the pages so far kept deferred, in order.
    #[serde(default)]
    deferred: Vec<u32>,
}

fn progress_key(collection_id: &str) -> Vec<u8> {
    [PROGRESS_PREFIX, collection_id.as_bytes()].concat()
}

/// The stage the reindex owed under `token` reached, with the indexes in
/// `deferred` kept deferred: its start until a page of it commits.
pub(super) async fn read(
    systemstore: &NamespaceView,
    collection_id: &str,
    token: u64,
    deferred: &[u32],
) -> Result<Stage> {
    let Some(bytes) = systemstore
        .get(&progress_key(collection_id))
        .await
        .map_err(Error::Storage)?
    else {
        return Ok(Stage::start());
    };
    let progress: Progress = serde_json::from_slice(&bytes)
        .map_err(|error| Error::Other(format!("invalid reindex progress record: {error}")))?;
    Ok(
        if progress.token == token && progress.deferred == deferred {
            progress.stage
        } else {
            Stage::start()
        },
    )
}

/// Records, in the page's transaction, that the reindex owed under `token`
/// reached `stage`, with the indexes in `deferred` kept deferred.
pub(super) async fn write(
    systemstore: &NamespaceView,
    collection_id: &str,
    token: u64,
    deferred: &[u32],
    stage: Stage,
) -> Result<()> {
    let bytes = serde_json::to_vec(&Progress {
        token,
        stage,
        deferred: deferred.to_vec(),
    })
    .map_err(|error| Error::Other(format!("failed to encode reindex progress: {error}")))?;
    systemstore
        .set(&progress_key(collection_id), &bytes)
        .await
        .map_err(Error::Storage)
}

/// Drops the progress of `collection_id`'s reindex, with its owed record.
pub(super) async fn clear(systemstore: &NamespaceView, collection_id: &str) -> Result<()> {
    systemstore
        .delete(&progress_key(collection_id))
        .await
        .map_err(Error::Storage)
}
