use super::*;

const IDLE_SECONDS: u64 = 3600;

pub(super) async fn reclaim_idle(store: &NamespaceView, budget: usize) -> Result<bool, MergeError> {
    const PREFIX: &[u8] = b"/merge-history/v1/";
    let now = now_unix();
    let mut roots = store
        .iterator(IterOptions::new().with_prefix(PREFIX.to_vec()))
        .await
        .map_err(storage_error)?;
    let mut candidate = None;
    let mut updated = false;
    for _ in 0..MAX_ROOTS {
        let Some(entry) = roots.next().await.map_err(storage_error)? else {
            break;
        };
        let suffix = &entry.key[PREFIX.len()..];
        let Some(end) = suffix.iter().position(|byte| *byte == b'/') else {
            continue;
        };
        let Ok(root) = std::str::from_utf8(&suffix[..end])
            .unwrap_or_default()
            .parse::<Cid>()
        else {
            continue;
        };
        if let Some(mut progress) = read::<Progress>(store, &state_key(&root)).await? {
            if progress.last_active_unix == 0 || progress.last_active_unix > now {
                progress.last_active_unix = now;
                write(store, &state_key(&root), &progress).await?;
                updated = true;
            } else if now.saturating_sub(progress.last_active_unix) >= IDLE_SECONDS {
                candidate = Some((root, progress));
                break;
            }
        }
        // Skip the root's potentially large data namespace, not each visited node.
        roots
            .seek(format!("/merge-history/v1/{root}0").as_bytes())
            .await
            .map_err(storage_error)?;
    }
    roots.close().await.map_err(storage_error)?;
    let Some((root, mut progress)) = candidate else {
        return Ok(updated);
    };
    let mut data = store
        .iterator(
            IterOptions::new().with_prefix(format!("/merge-history/v1/{root}/data/").into_bytes()),
        )
        .await
        .map_err(storage_error)?;
    let mut keys = Vec::new();
    for _ in 0..budget {
        let Some(entry) = data.next().await.map_err(storage_error)? else {
            break;
        };
        keys.push(entry.key);
    }
    let more = data.next().await.map_err(storage_error)?.is_some();
    data.close().await.map_err(storage_error)?;
    for key in keys {
        store.delete(&key).await.map_err(storage_error)?;
    }
    if more {
        progress.outcome = Some(Completion::Restart);
        write(store, &state_key(&root), &progress).await?;
    } else {
        store
            .delete(&state_key(&root))
            .await
            .map_err(storage_error)?;
        let roots = read::<u64>(store, ROOTS_KEY).await?.unwrap_or(1);
        write(store, ROOTS_KEY, &roots.saturating_sub(1)).await?;
    }
    Ok(true)
}
