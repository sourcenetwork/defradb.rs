//! A local missing-link walk yields after every page, records its length,
//! pages and time, and a fetch past its quantum stops it only at a page
//! boundary and only once its attempt fetched a block (#2599).

use super::*;
use crate::sync::manager::links::{
    find_all_missing_links, walk_missing_links, WalkEnd, MISSING_LINK_WALK_PAGE,
};

/// A root linking `held` stored, unmerged children and `absent` children that
/// are not stored. Returns the root's bytes and the absent CIDs.
async fn wide_dag(
    blockstore: &DefraBlockstore<RegolithStore>,
    held: usize,
    absent: usize,
) -> (Vec<u8>, Vec<Cid>) {
    let mut links = Vec::with_capacity(held + absent);
    for i in 0..held {
        let data = encode_ipld(ipld!({ "held": i as u64 }));
        let cid = make_cid(&data);
        blockstore.put(&cid, &data).await.unwrap();
        links.push(cid);
    }
    let mut missing = Vec::with_capacity(absent);
    for i in 0..absent {
        let cid = make_cid(&encode_ipld(ipld!({ "absent": i as u64 })));
        links.push(cid);
        missing.push(cid);
    }
    let root_data = encode_ipld(ipld!({
        "children": links.into_iter().map(Ipld::Link).collect::<Vec<_>>()
    }));
    (root_data, missing)
}

fn store() -> DefraBlockstore<RegolithStore> {
    DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true)
}

/// Polls `future` to completion, counting the polls that returned pending.
async fn count_pendings<F>(future: F) -> (F::Output, usize)
where
    F: std::future::Future,
{
    let mut future = std::pin::pin!(future);
    let mut pendings = 0;
    let output = std::future::poll_fn(|cx| match future.as_mut().poll(cx) {
        std::task::Poll::Ready(output) => std::task::Poll::Ready(output),
        std::task::Poll::Pending => {
            pendings += 1;
            std::task::Poll::Pending
        }
    })
    .await;
    (output, pendings)
}

#[test]
fn a_walk_without_a_deadline_yields_each_page_and_answers_the_same() {
    workers::block_on(async {
        let blockstore = store();
        let held = 2 * MISSING_LINK_WALK_PAGE as usize + 40;
        let (root_data, absent) = wide_dag(&blockstore, held, 2).await;
        let diagnostics = SyncDiagnostics::default();

        let (missing, pendings) = count_pendings(find_all_missing_links(
            &blockstore,
            &root_data,
            &diagnostics,
        ))
        .await;

        assert_eq!(missing.unwrap(), absent, "paging changed the frontier");
        let visits = (held + 2) as u64;
        let pages = visits / MISSING_LINK_WALK_PAGE;
        assert!(
            pendings as u64 >= pages,
            "the walk gave its core away {pendings} times over {pages} pages"
        );
        let walks = diagnostics.snapshot().missing_link_walks;
        assert_eq!(walks.completed, 1);
        assert_eq!(walks.preempted, 0);
        assert_eq!(walks.errors, 0);
        assert_eq!(walks.blocks, visits);
        assert_eq!(walks.blocks_max, visits);
        assert_eq!(walks.pages, pages);
        assert!(walks.micros_max <= walks.micros);
    })
    .unwrap();
}

#[test]
fn a_walk_past_its_deadline_stops_at_a_page_boundary() {
    workers::block_on(async {
        let blockstore = store();
        let held = 3 * MISSING_LINK_WALK_PAGE as usize;
        let (root_data, _) = wide_dag(&blockstore, held, 1).await;
        let diagnostics = SyncDiagnostics::default();
        let deadline = crate::rt::time::Instant::now();

        let walk = walk_missing_links(&blockstore, &root_data, &diagnostics, Some(deadline)).await;

        assert_eq!(walk.unwrap(), WalkEnd::Preempted);
        let walks = diagnostics.snapshot().missing_link_walks;
        assert_eq!(walks.preempted, 1);
        assert_eq!(walks.completed, 0);
        assert_eq!(
            walks.blocks, MISSING_LINK_WALK_PAGE,
            "the walk stopped off its first page boundary"
        );
    })
    .unwrap();
}

#[test]
fn a_walk_shorter_than_a_page_never_stops() {
    workers::block_on(async {
        let blockstore = store();
        let held = MISSING_LINK_WALK_PAGE as usize - 2;
        let (root_data, absent) = wide_dag(&blockstore, held, 1).await;
        let diagnostics = SyncDiagnostics::default();
        let deadline = crate::rt::time::Instant::now();

        let walk = walk_missing_links(&blockstore, &root_data, &diagnostics, Some(deadline)).await;

        assert_eq!(walk.unwrap(), WalkEnd::Frontier(absent));
        assert_eq!(diagnostics.snapshot().missing_link_walks.completed, 1);
    })
    .unwrap();
}

#[test]
fn a_failed_walk_records_its_length_but_not_an_error_count() {
    workers::block_on(async {
        let blockstore = store();
        let corrupt = encode_ipld(ipld!({ "x": 1 }));
        let cid = make_cid(&corrupt);
        blockstore.put(&cid, b"not dag-cbor \xff").await.unwrap();
        let root_data = encode_ipld(ipld!({ "child": cid }));
        let diagnostics = SyncDiagnostics::default();

        let walk = walk_missing_links(&blockstore, &root_data, &diagnostics, None).await;

        assert!(walk.is_err(), "{walk:?}");
        let walks = diagnostics.snapshot().missing_link_walks;
        assert_eq!((walks.completed, walks.preempted), (0, 0));
        assert_eq!(walks.blocks, 1);
        assert_eq!(walks.errors, 0, "the caller that acts on it counts it");
    })
    .unwrap();
}

/// Before the attempt fetched a block its walks run whole, however spent
/// the quantum is, so every dispatch makes progress; afterwards the quantum
/// bounds them.
#[test]
fn the_quantum_bounds_a_walk_only_after_the_attempt_fetched() {
    let diagnostics = SyncDiagnostics::default();
    let mut providers = ProviderRotation::new(vec![PeerId::new("provider".to_string())]);
    providers.begin_attempt();
    let mut state = ProviderFetchState::new(&mut providers, None, &diagnostics);
    let spent = crate::rt::time::Instant::now();
    state.quantum_ends = Some(spent);
    assert_eq!(state.walk_deadline(), None);

    state.providers.record_progress();
    assert_eq!(state.walk_deadline(), Some(spent));

    state.quantum_ends = None;
    assert_eq!(
        state.walk_deadline(),
        None,
        "a reply-driven fetch runs whole"
    );
}
