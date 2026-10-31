use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::{Error, Result};
use cid::Cid;
use datastore::NamespaceView;
use defra_core::Block;

async fn remove_owner(systemstore: &NamespaceView, cid: &Cid, doc_id: &str) -> Result<bool> {
    let cid = cid.to_string();
    crate::docid::map::delete_block_doc_id_mapping(systemstore, &cid, doc_id).await?;
    Ok(!crate::docid::map::block_has_owners(systemstore, &cid).await?)
}

pub(crate) async fn load_block(blockstore: &NamespaceView, cid: &Cid) -> Result<Option<Block>> {
    let Some(bytes) = blockstore
        .get(&cid.to_bytes())
        .await
        .map_err(Error::Storage)?
    else {
        return Ok(None);
    };
    Ok(Block::from_dag_cbor(&bytes).ok())
}

async fn remove_leaf_owner(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    cid: &Cid,
    doc_id: &str,
    can_delete: bool,
    retained: &RapidHashSet<Cid>,
) -> Result<()> {
    let block = load_block(blockstore, cid).await?;
    let unowned = remove_owner(systemstore, cid, doc_id).await?;
    if let Some(block) = block {
        if let Some(encryption_cid) = block
            .encryption
            .filter(|encryption_cid| !retained.contains(encryption_cid))
        {
            let encryption_unowned = remove_owner(systemstore, &encryption_cid, doc_id).await?;
            if can_delete && unowned && encryption_unowned {
                blockstore
                    .delete(&encryption_cid.to_bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
        if can_delete && unowned {
            if let Some(signature_cid) = block.signature {
                blockstore
                    .delete(&signature_cid.to_bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
    }
    if can_delete && unowned {
        blockstore
            .delete(&cid.to_bytes())
            .await
            .map_err(Error::Storage)?;
    }
    Ok(())
}

async fn remove_encryption_owner(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    encryption_cid: &Cid,
    doc_id: &str,
    can_delete: bool,
) -> Result<()> {
    let unowned = remove_owner(systemstore, encryption_cid, doc_id).await?;
    if can_delete && unowned {
        blockstore
            .delete(&encryption_cid.to_bytes())
            .await
            .map_err(Error::Storage)?;
    }
    Ok(())
}

/// The blocks `commits` still need: the commits, the field blocks they link
/// and the encryption blocks either references.
pub async fn referenced_blocks(
    blockstore: &NamespaceView,
    commits: impl IntoIterator<Item = Cid>,
) -> Result<RapidHashSet<Cid>> {
    let mut stack: Vec<Cid> = commits.into_iter().collect();
    let mut referenced = RapidHashSet::new();
    while let Some(cid) = stack.pop() {
        if !referenced.insert(cid) {
            continue;
        }
        let Some(block) = load_block(blockstore, &cid).await? else {
            continue;
        };
        referenced.extend(block.encryption);
        stack.extend(block.links.iter().flatten().map(|link| link.link));
    }
    Ok(referenced)
}

/// Remove one document's ownership of a commit and its direct field blocks.
/// History parents are deliberately retained; the caller chooses which
/// commits to prune. Blocks in `retained` are still referenced by the
/// document's remaining commits, so they keep their owner and their bytes.
/// Encryption blocks are inherited across commits while ownership is recorded
/// once per document, so a pruned commit cannot tell on its own whether
/// another commit still links its encryption block.
pub async fn delete_owned_commit(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    cid: &Cid,
    doc_id: &str,
    retained: &RapidHashSet<Cid>,
) -> Result<()> {
    let block = load_block(blockstore, cid).await?;
    let unowned = remove_owner(systemstore, cid, doc_id).await?;

    if let Some(block) = block {
        if let Some(links) = block.links {
            for link in links.iter().filter(|link| !retained.contains(&link.link)) {
                remove_leaf_owner(
                    blockstore,
                    systemstore,
                    &link.link,
                    doc_id,
                    unowned,
                    retained,
                )
                .await?;
            }
        }
        if let Some(encryption_cid) = block
            .encryption
            .filter(|encryption_cid| !retained.contains(encryption_cid))
        {
            remove_encryption_owner(blockstore, systemstore, &encryption_cid, doc_id, unowned)
                .await?;
        }
        if unowned {
            if let Some(signature_cid) = block.signature {
                blockstore
                    .delete(&signature_cid.to_bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
    }
    if unowned {
        blockstore
            .delete(&cid.to_bytes())
            .await
            .map_err(Error::Storage)?;
    }
    Ok(())
}

/// Remove one document's ownership from every block reachable from `roots`.
/// Shared blocks and everything they still reference remain intact.
pub async fn delete_owned_dag(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    roots: &[Cid],
    doc_id: &str,
) -> Result<()> {
    delete_owned_dag_for_owners(blockstore, systemstore, roots, &[doc_id.to_owned()], false).await
}

/// Remove several aliases' ownership from every block reachable from `roots`.
/// Shared blocks and everything they still reference remain intact. With
/// `collection_dag`, the roots are a branchable collection's documents, whose
/// collection DAG links every version's composite (one collection block per
/// document commit): the collection DAG owns those composites, so they and
/// everything they link are kept, and `_commits` of the collection and its
/// sync stay whole.
pub async fn delete_owned_dag_for_owners(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    roots: &[Cid],
    doc_ids: &[String],
    collection_dag: bool,
) -> Result<()> {
    let DagWalk {
        visited,
        edges,
        signatures,
        composites,
    } = DagWalk::from_roots(blockstore, roots).await?;
    let mut retained = RapidHashSet::new();
    for cid in &visited {
        let key = cid.to_string();
        for doc_id in doc_ids {
            crate::docid::map::delete_block_doc_id_mapping(systemstore, &key, doc_id).await?;
        }
        if (collection_dag && composites.contains(cid))
            || crate::docid::map::block_has_owners(systemstore, &key).await?
        {
            retained.insert(*cid);
        }
    }

    let mut stack: Vec<Cid> = retained.iter().copied().collect();
    while let Some(cid) = stack.pop() {
        if let Some(children) = edges.get(&cid) {
            for child in children {
                if retained.insert(*child) {
                    stack.push(*child);
                }
            }
        }
    }

    for cid in visited.difference(&retained) {
        if let Some(signature_cid) = signatures.get(cid) {
            drop_owners(systemstore, signature_cid, doc_ids).await?;
            blockstore
                .delete(&signature_cid.to_bytes())
                .await
                .map_err(Error::Storage)?;
        }
        blockstore
            .delete(&cid.to_bytes())
            .await
            .map_err(Error::Storage)?;
    }
    Ok(())
}

/// Remove several aliases' ownership from every block reachable from `roots`
/// and from their signature blocks, keeping every block. A purge without
/// history pruning leaves no owner edge naming a document that is gone, so
/// the blocks stay reclaimable.
pub async fn drop_dag_owners(
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    roots: &[Cid],
    doc_ids: &[String],
) -> Result<()> {
    let walk = DagWalk::from_roots(blockstore, roots).await?;
    for cid in walk.visited.iter().chain(walk.signatures.values()) {
        drop_owners(systemstore, cid, doc_ids).await?;
    }
    Ok(())
}

async fn drop_owners(systemstore: &NamespaceView, cid: &Cid, doc_ids: &[String]) -> Result<()> {
    let key = cid.to_string();
    for doc_id in doc_ids {
        crate::docid::map::delete_block_doc_id_mapping(systemstore, &key, doc_id).await?;
    }
    Ok(())
}

/// The blocks reachable from some roots, with each one's links and
/// signature block, and which of them are composites.
struct DagWalk {
    visited: RapidHashSet<Cid>,
    edges: RapidHashMap<Cid, Vec<Cid>>,
    signatures: RapidHashMap<Cid, Cid>,
    composites: RapidHashSet<Cid>,
}

impl DagWalk {
    async fn from_roots(blockstore: &NamespaceView, roots: &[Cid]) -> Result<Self> {
        let mut stack = roots.to_vec();
        let mut walk = Self {
            visited: RapidHashSet::new(),
            edges: RapidHashMap::new(),
            signatures: RapidHashMap::new(),
            composites: RapidHashSet::new(),
        };
        while let Some(cid) = stack.pop() {
            if !walk.visited.insert(cid) {
                continue;
            }
            let Some(block) = load_block(blockstore, &cid).await? else {
                continue;
            };
            let mut children = block.all_links();
            if let Some(encryption_cid) = block.encryption {
                children.push(encryption_cid);
            }
            if let Some(signature_cid) = block.signature {
                walk.signatures.insert(cid, signature_cid);
            }
            if matches!(block.delta, defra_core::CrdtDelta::Composite(_)) {
                walk.composites.insert(cid);
            }
            stack.extend(children.iter().copied());
            walk.edges.insert(cid, children);
        }
        Ok(walk)
    }
}
