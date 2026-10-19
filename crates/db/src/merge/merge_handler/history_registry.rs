use super::*;

const IDLE_SECONDS: u64 = 3600;
const PREFIX: &[u8] = b"/merge-history/v1/admissions/";

pub(super) fn admission_key(root: &Cid) -> Vec<u8> {
    [PREFIX, root.to_string().as_bytes()].concat()
}

/// Only admission and release change this range; history pages do not.
pub(super) async fn live_roots(
    store: &NamespaceView,
) -> Result<Vec<(Cid, Option<String>)>, MergeError> {
    let mut iter = store
        .iterator(
            IterOptions::new()
                .with_prefix(PREFIX.to_vec())
                .with_validation(true),
        )
        .await
        .map_err(storage_error)?;
    let mut roots = Vec::new();
    while let Some(entry) = iter.next().await.map_err(storage_error)? {
        if roots.len() >= MAX_ROOTS {
            return Err(storage_error("invalid history admission index"));
        }
        let cid = std::str::from_utf8(&entry.key[PREFIX.len()..])
            .map_err(storage_error)?
            .parse()
            .map_err(storage_error)?;
        roots.push((
            cid,
            serde_json::from_slice(&entry.value).map_err(storage_error)?,
        ));
    }
    iter.close().await.map_err(storage_error)?;
    Ok(roots)
}

pub(super) async fn reclaim_idle(
    store: &NamespaceView,
    roots: &[(Cid, Option<String>)],
    budget: usize,
) -> Result<bool, MergeError> {
    let now = now_unix();
    let mut updated = false;
    for (root, _) in roots {
        let Some(mut progress) = read::<Progress>(store, &state_key(root)).await? else {
            return Err(storage_error("history admission has no progress"));
        };
        if progress.last_active_unix == 0 || progress.last_active_unix > now {
            progress.last_active_unix = now;
            write(store, &state_key(root), &progress).await?;
            updated = true;
        } else if now.saturating_sub(progress.last_active_unix) >= IDLE_SECONDS {
            if cleanup(store, root, budget).await? {
                progress.outcome = Some(Completion::Restart);
                write(store, &state_key(root), &progress).await?;
            }
            return Ok(true);
        }
    }
    Ok(updated)
}
