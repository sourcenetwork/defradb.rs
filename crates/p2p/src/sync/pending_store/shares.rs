//! Each announcing peer's share of the durable registrations.
//!
//! A peer's count, `/p2p/pending_dag_peer/{peer}` (`/p2p/pending_dag_peer`
//! for a record with no announcing peer), is how many records hold a charge
//! to it, and a charged record's `/p2p/pending_dag_charge/{root}` holds the
//! key of the count it is in. A root new to the store that retires none reads
//! its peer's count in its registration transaction, is refused at the share
//! and charged otherwise. The read is validated, so of two registrations of
//! one peer that read the same count only one commits: the engine re-runs the
//! other, which reads the count the first wrote. A root that retires the older
//! root of its sender scope takes over that root's charge, and a discharged
//! root gives its charge back in the transaction that deletes its record.
//!
//! A record written before shares existed holds no charge and is in no
//! count; each leaves when its root merges or is quarantined.

use cid::Cid;
use datastore::NamespaceView;
use storage::corekv::Result;

const COUNT_KEY: &[u8] = b"/p2p/pending_dag_peer";
const CHARGE_PREFIX: &str = "/p2p/pending_dag_charge/";

/// How many records hold a charge to `source_peer`.
pub(super) async fn held(systemstore: &NamespaceView, source_peer: Option<&str>) -> Result<u64> {
    count(systemstore, &count_key(source_peer)).await
}

/// Charges `root` to `source_peer`, which `held` records hold a charge to.
pub(super) async fn charge(
    systemstore: &NamespaceView,
    root: &Cid,
    source_peer: Option<&str>,
    held: u64,
) -> Result<()> {
    let count = count_key(source_peer);
    systemstore
        .set(&count, &held.saturating_add(1).to_be_bytes())
        .await?;
    systemstore.set(&charge_key(root), &count).await
}

/// Hands the charge `older` holds, if any, to `root`.
pub(super) async fn hand_over(systemstore: &NamespaceView, older: &Cid, root: &Cid) -> Result<()> {
    let key = charge_key(older);
    let Some(count) = systemstore.get(&key).await? else {
        return Ok(());
    };
    systemstore.delete(&key).await?;
    systemstore.set(&charge_key(root), &count).await
}

/// Gives the charge `root` holds, if any, back to its count.
pub(super) async fn give_back(systemstore: &NamespaceView, root: &Cid) -> Result<()> {
    let key = charge_key(root);
    let Some(count_key) = systemstore.get(&key).await? else {
        return Ok(());
    };
    systemstore.delete(&key).await?;
    match count(systemstore, &count_key).await? {
        0 | 1 => systemstore.delete(&count_key).await,
        held => systemstore.set(&count_key, &(held - 1).to_be_bytes()).await,
    }
}

async fn count(systemstore: &NamespaceView, key: &[u8]) -> Result<u64> {
    Ok(systemstore
        .get(key)
        .await?
        .and_then(|bytes| <[u8; 8]>::try_from(bytes.as_ref()).ok())
        .map_or(0, u64::from_be_bytes))
}

fn count_key(source_peer: Option<&str>) -> Vec<u8> {
    let mut key = COUNT_KEY.to_vec();
    if let Some(peer) = source_peer {
        key.push(b'/');
        key.extend_from_slice(peer.as_bytes());
    }
    key
}

fn charge_key(root: &Cid) -> Vec<u8> {
    format!("{CHARGE_PREFIX}{root}").into_bytes()
}
