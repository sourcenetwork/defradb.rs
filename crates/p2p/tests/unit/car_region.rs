use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use rapidhash::RapidHashSet;
use storage::RegolithStore;

use super::collect_missing_region;
use crate::sync::car::collect_dag_blocks_within;
use crate::sync::car::doc_dag::{updates, DocDag};

type Store = DefraBlockstore<RegolithStore>;

fn store() -> Store {
    DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true)
}

async fn put_all(store: &Store, blocks: &[(Cid, Vec<u8>)]) {
    for (cid, data) in blocks {
        store.put(cid, data).await.unwrap();
    }
}

fn cids(blocks: &[(Cid, Vec<u8>)]) -> RapidHashSet<Cid> {
    blocks.iter().map(|(cid, _)| *cid).collect()
}

async fn served(
    provider: &Store,
    wants: &[Cid],
    haves: &[Cid],
    max_depth: Option<u32>,
) -> RapidHashSet<Cid> {
    let outcome = collect_missing_region(provider, wants, haves, max_depth)
        .await
        .unwrap();
    let set = cids(
        &outcome
            .blocks
            .iter()
            .map(|(cid, data)| (*cid, data.to_vec()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(set.len(), outcome.blocks.len(), "a block served twice");
    set
}

/// The receiver holds 500 updates and misses the 501st: it is served that
/// update's blocks and nothing else, in one request.
#[nuclei::test(threads = 0)]
async fn a_new_head_over_held_history_is_served_alone() {
    let provider = store();
    let mut dag = DocDag::default();
    let history = updates(&mut dag, 500, 1);
    let held_heads = dag.heads();
    let new = updates(&mut dag, 1, 2);
    put_all(&provider, &history).await;
    put_all(&provider, &new).await;

    let got = served(&provider, &dag.heads(), &held_heads, Some(2)).await;

    assert_eq!(got, cids(&new));
}

/// A branch 300 updates deep, forked below a head the receiver made itself
/// and the provider has not seen. The receiver names its head and ancestors;
/// the provider holds the ancestors, so it serves exactly the branch.
#[nuclei::test(threads = 0)]
async fn a_concurrent_branch_deeper_than_any_cut_is_served_exactly() {
    let provider = store();
    let mut base = DocDag::default();
    let history = updates(&mut base, 100, 1);
    let (mut theirs, mut ours) = (base.clone(), base.clone());
    let branch = updates(&mut theirs, 300, 2);
    let local = updates(&mut ours, 1, 3);
    put_all(&provider, &history).await;
    put_all(&provider, &branch).await;
    let receiver = store();
    put_all(&receiver, &history).await;
    put_all(&receiver, &local).await;
    let haves = crate::sync::coordinator::haves::haves_from_heads(&receiver, ours.heads()).await;
    assert!(haves.len() > 1);

    let got = served(&provider, &theirs.heads(), &haves, Some(2)).await;

    assert_eq!(got, cids(&branch));
}

/// Two branches the provider merged: the receiver holds one of them and is
/// served the other branch and the merge.
#[nuclei::test(threads = 0)]
async fn a_merge_of_a_held_and_a_missing_branch_serves_the_missing_side() {
    let provider = store();
    let mut base = DocDag::default();
    let history = updates(&mut base, 50, 1);
    let (mut left, mut right) = (base.clone(), base.clone());
    let held = updates(&mut left, 40, 2);
    let missing = updates(&mut right, 70, 3);
    let mut merged = left.merged(&right);
    let merge = merged.update(&["name"], 4);
    for blocks in [&history, &held, &missing, &merge] {
        put_all(&provider, blocks).await;
    }

    let got = served(&provider, &merged.heads(), &left.heads(), None).await;

    let mut expected = cids(&missing);
    expected.extend(cids(&merge));
    assert_eq!(got, expected);
}

/// A provider holding none of the haves serves what the depth cut serves.
#[nuclei::test(threads = 0)]
async fn haves_the_provider_lacks_keep_the_depth_cut() {
    let provider = store();
    let mut dag = DocDag::default();
    put_all(&provider, &updates(&mut dag, 30, 1)).await;
    let mut elsewhere = DocDag::default();
    let unknown = updates(&mut elsewhere, 1, 9);

    let got = served(
        &provider,
        &dag.heads(),
        &cids(&unknown).into_iter().collect::<Vec<_>>(),
        Some(2),
    )
    .await;

    let cut = collect_dag_blocks_within(&provider, &dag.heads(), Some(2))
        .await
        .unwrap();
    assert_eq!(got, cut.blocks.iter().map(|(cid, _)| *cid).collect());
}

/// A wanted block is served even when a have reaches it: the receiver lost
/// it (a hole in held history), and only an exact ask brings it back.
#[nuclei::test(threads = 0)]
async fn a_wanted_block_inside_held_history_is_still_served() {
    let provider = store();
    let mut dag = DocDag::default();
    let history = updates(&mut dag, 20, 1);
    put_all(&provider, &history).await;
    // Update 4 sets two fields, so blocks 10 and 11 are its field blocks
    // and block 12 its composite.
    let hole = history[12].0;

    let got = served(&provider, &[hole], &dag.heads(), Some(2)).await;

    assert_eq!(got, cids(&history[10..13]));
}
