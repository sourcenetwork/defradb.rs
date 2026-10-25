use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use std::sync::Arc;

use acp::DocumentACP;
use bytes::Bytes;
use p2p::message::PushLogRequest;
use p2p::transport::PeerId;
use p2p::P2PTransport;
use storage::corekv::{IterOptions, Reader, Store};

use crate::database::DB;
use crate::merge::push_docs_common::load_latest_composite_head_cids;
use crate::merge::push_docs_creator::resolve_push_creator;
use crate::merge::push_docs_replay::{
    persist_replay_failures, ReplayDocumentFailure, ReplayPushConfig, ReplayPushGate,
};

enum PushOwnership {
    Replicator,
    Selected(RapidHashSet<(String, String)>),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PushExistingDocsSeOptions<'a> {
    pub encryption_key: Option<&'a [u8]>,
    pub identity_pubkey: Option<&'a [u8]>,
}

async fn document_matches_filter<R: Reader + ?Sized>(
    datastore: &R,
    collection: &crate::collection::Collection,
    doc_short_id: u64,
    filter: &p2p::ReplicationFilter,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
) -> Result<bool, String> {
    let mut doc_key = format!("/d/{}/", collection.collection_id()).into_bytes();
    doc_key.extend_from_slice(&storage::keys::doc_id_index::encode_doc_short_id(
        doc_short_id,
    ));
    let Some(doc_data) = datastore
        .get(&doc_key)
        .await
        .map_err(|e| format!("failed to read document for replication filter: {}", e))?
    else {
        return Ok(false);
    };

    let doc =
        crdt::stored::document_from_storage(&doc_data, collection.schema_arc(), collection.crdts())
            .map_err(|e| format!("failed to decode document for replication filter: {}", e))?;
    let document_json = serde_json::Value::Object(
        doc.to_map()
            .map_err(|e| format!("failed to encode document for replication filter: {}", e))?
            .into_iter()
            .collect(),
    );
    Ok(matcher.matches("", filter, &document_json))
}

/// Index an optional `(collection_name, doc_id)` allowlist.
///
/// `None` means every document in the scanned collections should be considered.
fn allowlist_index(
    allowlist: Option<&[(String, String)]>,
) -> Option<RapidHashMap<&str, RapidHashSet<&str>>> {
    let docs = allowlist?;
    let mut index: RapidHashMap<&str, RapidHashSet<&str>> = RapidHashMap::new();
    for (collection, doc_id) in docs {
        index
            .entry(collection.as_str())
            .or_default()
            .insert(doc_id.as_str());
    }
    Some(index)
}

fn unique_collection_names(docs: &[(String, String)]) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = RapidHashSet::new();
    for (collection, _) in docs {
        if seen.insert(collection.as_str()) {
            names.push(collection.clone());
        }
    }
    names
}

fn document_is_allowlisted(
    collection_name: &str,
    doc_id: &str,
    allowlist: Option<&RapidHashMap<&str, RapidHashSet<&str>>>,
) -> bool {
    match allowlist {
        None => true,
        Some(index) => index
            .get(collection_name)
            .is_some_and(|ids| ids.contains(doc_id)),
    }
}

/// Push existing documents to a replicator peer through the configured transport.
#[allow(clippy::too_many_arguments)]
pub async fn push_existing_docs<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    document_acp: Option<&dyn DocumentACP>,
    peer_id: &PeerId,
    collections: &[String],
    filters: &p2p::ReplicationFilters,
    se_options: PushExistingDocsSeOptions<'_>,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
    car_authority: &p2p::sync::HeadHintCarAuthority,
) -> Result<(), String> {
    push_existing_docs_with_config(
        transport,
        db,
        document_acp,
        peer_id,
        collections,
        filters,
        se_options,
        ReplayPushConfig::default(),
        matcher,
        car_authority,
    )
    .await
}

/// Push an explicit `(collection_name, doc_id)` set without adding a subscription.
/// Success requires acknowledgements for every selected document's unchanged
/// heads, independently of continuous replication to the same peer.
#[allow(clippy::too_many_arguments)]
pub async fn push_existing_docs_by_id<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    document_acp: Option<&dyn DocumentACP>,
    peer_id: &PeerId,
    docs: &[(String, String)],
    filters: &p2p::ReplicationFilters,
    se_options: PushExistingDocsSeOptions<'_>,
    replay_config: ReplayPushConfig,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
    car_authority: &p2p::sync::HeadHintCarAuthority,
) -> Result<(), String> {
    let collections = unique_collection_names(docs);
    push_existing_docs_with_config_and_allowlist(
        transport,
        db,
        document_acp,
        peer_id,
        &collections,
        filters,
        se_options,
        replay_config,
        matcher,
        car_authority,
        Some(docs),
    )
    .await
}

/// Push existing documents with explicit replay limits.
#[allow(clippy::too_many_arguments)]
pub async fn push_existing_docs_with_config<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    document_acp: Option<&dyn DocumentACP>,
    peer_id: &PeerId,
    collections: &[String],
    filters: &p2p::ReplicationFilters,
    se_options: PushExistingDocsSeOptions<'_>,
    replay_config: ReplayPushConfig,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
    car_authority: &p2p::sync::HeadHintCarAuthority,
) -> Result<(), String> {
    push_existing_docs_with_config_and_allowlist(
        transport,
        db,
        document_acp,
        peer_id,
        collections,
        filters,
        se_options,
        replay_config,
        matcher,
        car_authority,
        None,
    )
    .await
}

/// Push existing documents, optionally restricted to an allowlist of
/// `(collection_name, doc_id)` pairs. `None` scans every document in
/// `collections`.
#[allow(clippy::too_many_arguments)]
async fn push_existing_docs_with_config_and_allowlist<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    document_acp: Option<&dyn DocumentACP>,
    peer_id: &PeerId,
    collections: &[String],
    filters: &p2p::ReplicationFilters,
    se_options: PushExistingDocsSeOptions<'_>,
    replay_config: ReplayPushConfig,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
    car_authority: &p2p::sync::HeadHintCarAuthority,
    allowlist: Option<&[(String, String)]>,
) -> Result<(), String> {
    let network = db.network();
    super::push_docs_network::wait_for_peer(&network, transport, peer_id).await?;

    let local_peer_id = transport.local_peer_id().to_string();
    let peerstore = db
        .peerstore()
        .with_retry_schedule(replay_config.retry_schedule.clone());
    let mut ownership = if let Some(docs) = allowlist {
        PushOwnership::Selected(docs.iter().cloned().collect())
    } else {
        if peerstore
            .get_replicator(peer_id.as_str())
            .await
            .map_err(|error| format!("failed to load replicator: {error}"))?
            .is_none()
        {
            return Err(format!(
                "replicator {peer_id} removed before existing-document replay"
            ));
        }
        PushOwnership::Replicator
    };
    let txn = db
        .new_txn(true)
        .await
        .map_err(|e| format!("failed to create transaction: {}", e))?;

    let headstore = txn
        .headstore()
        .map_err(|e| format!("failed to get headstore: {}", e))?;
    let blockstore_view = txn
        .blockstore()
        .map_err(|e| format!("failed to get blockstore: {}", e))?;
    let datastore = txn
        .datastore()
        .map_err(|e| format!("failed to get datastore: {}", e))?;
    let systemstore = txn
        .systemstore()
        .map_err(|e| format!("failed to get systemstore: {}", e))?;

    let mut push_handles = Vec::new();
    let replay_gate = Arc::new(ReplayPushGate::new(replay_config).on(network.clone()));
    let allowlist_by_collection = allowlist_index(allowlist);

    for col_name in collections {
        if let Some(ref allowed) = allowlist_by_collection {
            if !allowed.contains_key(col_name.as_str()) {
                continue;
            }
        }

        let collection = match db
            .get_collection(col_name)
            .map_err(|e| format!("failed to get collection: {}", e))?
        {
            Some(c) => c,
            None => continue,
        };

        let col_prefix = format!("/d/{}/", collection.collection_id()).into_bytes();
        let doc_prefix_len = col_prefix.len();
        let opts = IterOptions::new()
            .with_prefix(col_prefix)
            .with_keys_only(true);
        let mut doc_iter = datastore
            .iterator(opts)
            .await
            .map_err(|e| format!("failed to iterate datastore: {}", e))?;

        let mut doc_short_ids = Vec::new();
        while let Some(pair) = doc_iter
            .next()
            .await
            .map_err(|e| format!("datastore iteration error: {}", e))?
        {
            if let Ok(short_id) =
                storage::keys::doc_id_index::decode_doc_short_id(&pair.key[doc_prefix_len..])
            {
                doc_short_ids.push(short_id);
            }
        }
        doc_iter
            .close()
            .await
            .map_err(|e| format!("datastore close error: {}", e))?;

        let mut doc_ids = Vec::new();
        for short_id in doc_short_ids {
            match crate::docid::map::get_doc_id(&systemstore, short_id).await {
                Ok(Some(doc_id)) => doc_ids.push((short_id, doc_id)),
                Ok(None) => {}
                Err(e) => return Err(format!("doc-ID mapping lookup failed: {}", e)),
            }
        }

        'documents: for (doc_short_id, doc_id) in &doc_ids {
            if !document_is_allowlisted(col_name, doc_id, allowlist_by_collection.as_ref()) {
                continue;
            }

            if let Some(filter) = filters.get(collection.collection_id()) {
                if !document_matches_filter(&datastore, &collection, *doc_short_id, filter, matcher)
                    .await?
                {
                    continue;
                }
            }

            let creator =
                resolve_push_creator(document_acp, &collection, doc_id, &local_peer_id).await;
            let mut doc_blocks = Vec::new();
            for head_cid in
                load_latest_composite_head_cids(&headstore, &blockstore_view, *doc_short_id).await
            {
                let block_key = head_cid.to_bytes();
                let block_data = match blockstore_view.get(&block_key).await {
                    Ok(Some(data)) => data,
                    _ => continue,
                };

                doc_blocks.push((head_cid, block_data));
            }

            let mut replay_head_cids: Vec<_> = doc_blocks.iter().map(|(cid, _)| *cid).collect();
            replay_head_cids.sort_unstable();

            if doc_blocks.is_empty() {
                continue;
            }

            // Tracked replay registers before the fallible serving capability,
            // so a refused grant or signature leaves its durable retry owner intact.
            let marker_generation = if matches!(&ownership, PushOwnership::Replicator) {
                let Some(generation) = peerstore
                    .observe_push_head_with_generation(
                        peer_id.as_str(),
                        doc_id,
                        collection.collection_id(),
                    )
                    .await
                    .map_err(|error| format!("failed to register replay marker: {error}"))?
                else {
                    let mut error =
                        format!("replicator {peer_id} removed during existing-document replay");
                    for (_, _, _, _, _, _, handle) in push_handles.drain(..) {
                        let outcome: Option<bool> = handle.await;
                        if outcome.is_none() {
                            error.push_str("; replay push task panicked or its runtime stopped");
                        }
                    }
                    return Err(error);
                };
                Some(generation)
            } else {
                None
            };

            let mut requests = Vec::new();
            for (block_cid, block_data) in doc_blocks {
                let Some(grant) = car_authority.register(peer_id.clone(), block_cid) else {
                    tracing::warn!(
                        %peer_id,
                        %doc_id,
                        %block_cid,
                        "Selective CAR authority full"
                    );
                    continue 'documents;
                };
                let mut request = PushLogRequest::new(
                    doc_id.clone(),
                    Bytes::from(block_cid.to_bytes()),
                    collection.collection_id().to_string(),
                    creator.clone(),
                    block_data,
                );
                if let Err(e) = p2p::signing::sign_with_transport(transport, &mut request) {
                    tracing::warn!(error = %e, "Failed to sign PushLog request");
                    continue 'documents;
                }
                requests.push((grant, request));
            }

            if !requests.is_empty() {
                let t = transport.clone();
                let pid = peer_id.clone();
                let gate = replay_gate.clone();
                let peer_key = pid.clone();
                let retry_store = db.peerstore();
                let replay_doc_id = doc_id.clone();
                let replay_collection_id = collection.collection_id().to_string();
                let total_blocks = requests.len();
                let permit = replay_gate
                    .acquire_document_task()
                    .await
                    .map_err(|e| format!("replay gate closed before scheduling push: {e}"))?;
                push_handles.push((
                    col_name.clone(),
                    replay_doc_id,
                    replay_collection_id,
                    *doc_short_id,
                    replay_head_cids,
                    marker_generation,
                    network.run(async move {
                        let _permit = permit;
                        let mut completed_blocks = 0usize;
                        for (_car_grant, req) in requests {
                            let cid = req.cid.clone();
                            match gate
                                .send_pushlog_with_admission(
                                    &peer_key,
                                    super::push_docs_replay::check_retry_admission(&retry_store, &pid),
                                    t.send_two_stream_request(&pid, req),
                                )
                                .await
                            {
                                Ok(reply) if reply.err_message.is_some() => {
                                    if let Err(error) = super::push_docs_replay::persist_retry_after(
                                        &retry_store, &pid, &reply,
                                    ).await {
                                        tracing::warn!(%error, "Failed to persist replay admission deadline");
                                    }
                                    tracing::warn!(
                                        peer_id = %pid,
                                        completed_blocks,
                                        total_blocks,
                                        cid_len = cid.len(),
                                        error = %reply.err_message.as_deref().unwrap_or("unknown pushlog error"),
                                        "Existing document replay PushLog was rejected; deferring document to persisted retry"
                                    );
                                    return false;
                                }
                                Ok(_) => {
                                    completed_blocks += 1;
                                }
                                Err(e) => {
                                    if e.is_connection_like() {
                                        tracing::debug!(
                                            peer_id = %pid,
                                            completed_blocks,
                                            total_blocks,
                                            cid_len = cid.len(),
                                            error = %e,
                                            "Existing document replay stopped because the connection became unavailable"
                                        );
                                    } else {
                                        tracing::warn!(
                                            peer_id = %pid,
                                            completed_blocks,
                                            total_blocks,
                                            cid_len = cid.len(),
                                            error = %e,
                                            "Existing document replay PushLog failed; deferring document to persisted retry"
                                        );
                                    }
                                    return false;
                                }
                            }
                        }
                        true
                    }),
                ));
            }
        }
    }

    tracing::debug!(task_count = push_handles.len(), "awaiting push tasks");
    let mut replay_failures = Vec::new();
    for (
        collection_name,
        doc_id,
        collection_id,
        doc_short_id,
        attempted_heads,
        marker_generation,
        jh,
    ) in push_handles
    {
        match jh.await {
            Some(true) => {
                let verify_txn = db
                    .new_txn(true)
                    .await
                    .map_err(|error| format!("replay head verification transaction: {error}"))?;
                let verify_heads = verify_txn.headstore().map_err(|error| error.to_string())?;
                let verify_blocks = verify_txn.blockstore().map_err(|error| error.to_string())?;
                let mut current_heads =
                    load_latest_composite_head_cids(&verify_heads, &verify_blocks, doc_short_id)
                        .await;
                current_heads.sort_unstable();
                if current_heads != attempted_heads {
                    tracing::debug!(%doc_id, "Document changed during replay; retaining dirty marker");
                    replay_failures.push(ReplayDocumentFailure {
                        doc_id,
                        collection_id,
                    });
                    continue;
                }
                if let Some(marker_generation) = marker_generation {
                    peerstore
                        .complete_retry_scope(
                            peer_id.as_str(),
                            &doc_id,
                            &collection_id,
                            false,
                            marker_generation,
                            &current_heads
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>(),
                        )
                        .await
                        .map_err(|error| format!("failed to clear replay marker: {error}"))?;
                }
                if let PushOwnership::Selected(pending) = &mut ownership {
                    pending.remove(&(collection_name, doc_id));
                }
            }
            Some(false) => replay_failures.push(ReplayDocumentFailure {
                doc_id,
                collection_id,
            }),
            None => {
                tracing::error!(%doc_id, "Replay push task panicked or was cancelled");
                replay_failures.push(ReplayDocumentFailure {
                    doc_id,
                    collection_id,
                });
            }
        }
    }
    tracing::debug!("all push tasks completed");
    match &ownership {
        PushOwnership::Replicator => {
            persist_replay_failures(&peerstore, peer_id, &replay_failures).await?;
        }
        PushOwnership::Selected(pending) if !pending.is_empty() => {
            return Err(format!(
                "selected document push did not acknowledge all requested heads ({} documents incomplete)",
                pending.len()
            ));
        }
        PushOwnership::Selected(_) => {}
    }

    if let Some(se_key) = se_options.encryption_key {
        let coordinator = match se_options.identity_pubkey {
            Some(pubkey) => crate::merge::se::SECoordinator::with_key_and_identity(
                se_key.to_vec(),
                pubkey.to_vec(),
            ),
            None => crate::merge::se::SECoordinator::with_key(se_key.to_vec()),
        };

        for col_name in collections {
            if let Some(ref allowed) = allowlist_by_collection {
                if !allowed.contains_key(col_name.as_str()) {
                    continue;
                }
            }

            let collection = match db.get_collection(col_name) {
                Ok(Some(c)) => c,
                _ => continue,
            };

            let encrypted_indexes = &collection.schema().encrypted_indexes;
            if encrypted_indexes.is_empty() {
                continue;
            }

            tracing::debug!(collection = %col_name, index_count = encrypted_indexes.len(), "generating SE artifacts");

            let col_prefix = format!("/d/{}/", collection.collection_id()).into_bytes();
            let doc_prefix_len = col_prefix.len();
            let opts = IterOptions::new()
                .with_prefix(col_prefix)
                .with_keys_only(true);
            let mut doc_iter = datastore
                .iterator(opts)
                .await
                .map_err(|e| format!("SE: failed to iterate datastore: {}", e))?;

            let mut se_doc_short_ids = Vec::new();
            while let Some(pair) = doc_iter
                .next()
                .await
                .map_err(|e| format!("SE: datastore iteration error: {}", e))?
            {
                if let Ok(short_id) =
                    storage::keys::doc_id_index::decode_doc_short_id(&pair.key[doc_prefix_len..])
                {
                    se_doc_short_ids.push(short_id);
                }
            }
            doc_iter
                .close()
                .await
                .map_err(|e| format!("SE: datastore close error: {}", e))?;

            let mut se_doc_ids = Vec::new();
            for short_id in se_doc_short_ids {
                match crate::docid::map::get_doc_id(&systemstore, short_id).await {
                    Ok(Some(doc_id)) => se_doc_ids.push((short_id, doc_id)),
                    Ok(None) => {}
                    Err(e) => return Err(format!("SE: doc-ID mapping lookup failed: {}", e)),
                }
            }

            let mut all_artifacts = Vec::new();
            for (doc_short_id, doc_id) in &se_doc_ids {
                if !document_is_allowlisted(col_name, doc_id, allowlist_by_collection.as_ref()) {
                    continue;
                }

                if let Some(filter) = filters.get(collection.collection_id()) {
                    if !document_matches_filter(
                        &datastore,
                        &collection,
                        *doc_short_id,
                        filter,
                        matcher,
                    )
                    .await?
                    {
                        continue;
                    }
                }

                let doc_key = storage::keys::doc_key(collection.collection_id(), *doc_short_id);
                let doc_data = match datastore.get(&doc_key).await {
                    Ok(Some(data)) => data,
                    _ => continue,
                };

                let doc = match crdt::stored::document_from_storage(
                    &doc_data,
                    collection.schema_arc(),
                    collection.crdts(),
                ) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(doc_id = %doc_id, error = %e, "SE: failed to deserialize document");
                        continue;
                    }
                };

                let field_values: rapidhash::RapidHashMap<String, document::NormalValue> = doc
                    .values()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.value().clone()))
                    .collect();

                match coordinator.generate_artifacts(
                    collection.collection_id(),
                    doc_id,
                    encrypted_indexes,
                    &[],
                    &field_values,
                ) {
                    Ok(artifacts) => {
                        for a in artifacts {
                            all_artifacts.push(p2p::message::SEArtifact::new(
                                &a.doc_id,
                                &a.index_id,
                                a.search_tag,
                            ));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(doc_id = %doc_id, error = %e, "SE: failed to generate artifacts");
                    }
                }
            }

            if !all_artifacts.is_empty() {
                tracing::debug!(artifact_count = all_artifacts.len(), collection = %col_name, peer_id = %peer_id, "sending SE artifacts");

                let se_request = p2p::message::PushSEArtifactsRequest::new(
                    collection.collection_id().to_string(),
                    all_artifacts,
                );

                let sent = super::push_docs_network::send_se_artifacts(
                    &network, transport, peer_id, se_request,
                )
                .await;
                if let Err(e) = sent {
                    tracing::warn!(
                        peer_id = %peer_id,
                        collection = %col_name,
                        error = %e,
                        "Failed to send SE artifacts to replicator"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Retry pushing a single document's composite heads to a replicator peer.
#[allow(clippy::too_many_arguments)]
pub async fn retry_doc<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    document_acp: Option<&dyn DocumentACP>,
    peer_id: &PeerId,
    doc_id: &str,
    collection_id: &str,
    marker_generation: u64,
    filters: &p2p::ReplicationFilters,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
    car_authority: &p2p::sync::HeadHintCarAuthority,
) -> Result<(), String> {
    let network = db.network();
    let local_peer_id = transport.local_peer_id().to_string();
    let resolved_collection;
    let collection_id = if collection_id.is_empty() {
        resolved_collection =
            crate::merge::push_docs_common::resolve_collection_id_for_doc(db, doc_id)
                .await?
                .ok_or_else(|| format!("collection for document '{doc_id}' not found"))?;
        resolved_collection.as_str()
    } else {
        collection_id
    };
    let collection = db
        .find_collection_by_id(collection_id)
        .map_err(|e| format!("failed to get collection: {}", e))?
        .ok_or_else(|| format!("collection '{}' not found", collection_id))?;
    let doc_short_id = {
        let txn = db
            .new_txn(true)
            .await
            .map_err(|e| format!("failed to create identity transaction: {}", e))?;
        let systemstore = txn
            .systemstore()
            .map_err(|e| format!("failed to get systemstore: {}", e))?;
        match crate::docid::map::get_doc_ref(&systemstore, doc_id)
            .await
            .map_err(|e| format!("doc-ID mapping lookup failed: {}", e))?
        {
            Some(doc_ref) => doc_ref.doc_short_id,
            None => {
                return crate::merge::push_docs_common::complete_document_retry_if_absent(
                    db,
                    peer_id.as_str(),
                    doc_id,
                    collection_id,
                    marker_generation,
                )
                .await;
            }
        }
    };
    if let Some(filter) = filters.get(collection_id) {
        let peerstore = db.peerstore();
        let txn = db
            .new_txn(true)
            .await
            .map_err(|e| format!("failed to create filter transaction: {}", e))?;
        let datastore = txn
            .datastore()
            .map_err(|e| format!("failed to get datastore: {}", e))?;
        if !document_matches_filter(&datastore, &collection, doc_short_id, filter, matcher).await? {
            peerstore
                .complete_retry_scope(
                    peer_id.as_str(),
                    doc_id,
                    collection_id,
                    false,
                    marker_generation,
                    &[],
                )
                .await
                .map_err(|error| format!("failed to clear filtered retry marker: {error}"))?;
            return Ok(());
        }
    }
    let creator = resolve_push_creator(document_acp, &collection, doc_id, &local_peer_id).await;

    let headstore = storage::stores::Headstore::new(db.store().clone());
    let head_txn = headstore
        .new_txn(true)
        .await
        .map_err(|e| format!("headstore txn: {}", e))?;

    let blockstore_view = storage::stores::Blockstore::new(db.store().clone(), true);
    let block_txn = blockstore_view
        .new_txn(true)
        .await
        .map_err(|e| format!("blockstore txn: {}", e))?;
    let mut attempted_heads =
        load_latest_composite_head_cids(&*head_txn, &*block_txn, doc_short_id).await;
    attempted_heads.sort_unstable();
    let mut successful_blocks = 0usize;
    for head_cid in attempted_heads.iter().copied() {
        let block_data = match block_txn.get(&head_cid.to_bytes()).await {
            Ok(Some(data)) => data,
            _ => continue,
        };

        {
            let block_cid = head_cid;
            let _car_grant = car_authority
                .register(peer_id.clone(), head_cid)
                .ok_or_else(|| format!("selective CAR authority full for retry head {head_cid}"))?;
            let mut request = PushLogRequest::new(
                doc_id.to_string(),
                Bytes::from(block_cid.to_bytes()),
                collection_id.to_string(),
                creator.clone(),
                block_data,
            );

            if p2p::signing::sign_with_transport(transport, &mut request).is_err() {
                return Err(format!(
                    "failed to sign replay block after {successful_blocks} successful block(s)"
                ));
            }

            if super::push_docs_replay::remaining_retry_after(&db.peerstore(), peer_id)
                .await?
                .is_some()
            {
                return Err(p2p::error::RATE_LIMITED_MESSAGE.to_string());
            }
            let sent =
                super::push_docs_network::send_pushlog(&network, transport, peer_id, request);
            match sent.await {
                Ok(reply) if reply.err_message.is_some() => {
                    super::push_docs_replay::persist_retry_after(&db.peerstore(), peer_id, &reply)
                        .await?;
                    return Err(format!(
                        "peer rejected replay after {successful_blocks} successful block(s): {}",
                        reply
                            .err_message
                            .as_deref()
                            .unwrap_or("unknown pushlog error")
                    ));
                }
                Ok(_) => {
                    successful_blocks += 1;
                }
                Err(error) => {
                    let prefix = if error.is_connection_like() {
                        p2p::error::TRANSPORT_UNAVAILABLE_PREFIX
                    } else {
                        "replay push failed"
                    };
                    return Err(format!(
                        "{prefix} after {successful_blocks} successful block(s): {error}"
                    ));
                }
            }
        }
    }
    drop(block_txn);
    drop(head_txn);
    crate::merge::push_docs_common::complete_document_retry_if_current(
        db,
        peer_id.as_str(),
        doc_id,
        collection_id,
        doc_short_id,
        &attempted_heads,
        marker_generation,
    )
    .await
}

/// Rederive and announce every current collection head for a dirty collection
/// scope. No CID is retained in durable sender state.
pub async fn retry_collection_commit<S: Store + 'static, T: P2PTransport>(
    transport: &T,
    db: &DB<S>,
    peer_id: &PeerId,
    collection_id: &str,
    marker_generation: u64,
    car_authority: &p2p::sync::HeadHintCarAuthority,
) -> Result<(), String> {
    let network = db.network();
    let creator = transport.local_peer_id().to_string();
    let txn = db
        .new_txn(true)
        .await
        .map_err(|error| format!("collection retry txn: {error}"))?;
    let systemstore = txn.systemstore().map_err(|error| error.to_string())?;
    let headstore = txn.headstore().map_err(|error| error.to_string())?;
    let block_txn = txn.blockstore().map_err(|error| error.to_string())?;
    let short_id =
        crate::collection::require_persisted_collection_short_id(&systemstore, collection_id)
            .await
            .map_err(|error| format!("collection retry short id: {error}"))?;
    let mut heads =
        crate::merge::push_docs_common::load_collection_head_cids(&headstore, short_id).await?;
    heads.sort_unstable();
    for cid in heads.iter().copied() {
        let block_data = block_txn
            .get(&cid.to_bytes())
            .await
            .map_err(|error| format!("collection head read: {error}"))?
            .ok_or_else(|| format!("current collection head {cid} is missing"))?;
        let _car_grant = car_authority
            .register(peer_id.clone(), cid)
            .ok_or_else(|| format!("selective CAR authority full for collection head {cid}"))?;
        let mut request = PushLogRequest::new(
            String::new(),
            Bytes::from(cid.to_bytes()),
            collection_id.to_string(),
            creator.clone(),
            block_data,
        );
        if p2p::signing::sign_with_transport(transport, &mut request).is_err() {
            return Err(format!("failed to sign current collection head {cid}"));
        }
        if super::push_docs_replay::remaining_retry_after(&db.peerstore(), peer_id)
            .await?
            .is_some()
        {
            return Err(p2p::error::RATE_LIMITED_MESSAGE.to_string());
        }
        let sent = super::push_docs_network::send_pushlog(&network, transport, peer_id, request);
        match sent.await {
            Ok(reply) if reply.err_message.is_some() => {
                super::push_docs_replay::persist_retry_after(&db.peerstore(), peer_id, &reply)
                    .await?;
                return Err(format!(
                    "peer rejected current collection head {cid}: {}",
                    reply
                        .err_message
                        .as_deref()
                        .unwrap_or("unknown pushlog error")
                ));
            }
            Ok(_) => {}
            Err(error) => return Err(format!("collection head {cid} push failed: {error}")),
        }
    }
    drop(txn);
    crate::merge::push_docs_common::complete_collection_retry_if_current(
        db,
        peer_id.as_str(),
        collection_id,
        &heads,
        marker_generation,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanned_docs() -> Vec<(String, String)> {
        vec![
            ("Users".to_string(), "doc-a".to_string()),
            ("Users".to_string(), "doc-b".to_string()),
            ("Posts".to_string(), "doc-c".to_string()),
        ]
    }

    fn retain_allowlisted(
        scanned: &[(String, String)],
        allowlist: Option<&[(String, String)]>,
    ) -> Vec<(String, String)> {
        let index = allowlist_index(allowlist);
        scanned
            .iter()
            .filter(|(collection, doc_id)| {
                document_is_allowlisted(collection, doc_id, index.as_ref())
            })
            .cloned()
            .collect()
    }

    #[test]
    fn allowlist_path_only_considers_named_docs() {
        let allowlist = [
            ("Users".to_string(), "doc-a".to_string()),
            ("Posts".to_string(), "missing-doc".to_string()),
        ];
        let considered = retain_allowlisted(&scanned_docs(), Some(&allowlist));
        assert_eq!(considered, vec![("Users".to_string(), "doc-a".to_string())]);
        assert_eq!(
            unique_collection_names(&allowlist),
            vec!["Users".to_string(), "Posts".to_string()]
        );
    }

    #[test]
    fn missing_allowlist_considers_every_scanned_doc() {
        let considered = retain_allowlisted(&scanned_docs(), None);
        assert_eq!(considered, scanned_docs());
    }
}
