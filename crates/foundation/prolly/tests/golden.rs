//! Roots pinned from before the chunk byte cap (`node::MAX_CHUNK_BYTES`): an
//! input whose chunks all close on a boundary under the cap keeps every chunk,
//! so its root, and every node hash under it, is the same.

mod common;

use std::num::NonZeroU32;

use common::run;
use prolly::{MemStore, Tree};

fn root(target: u32, count: u32) -> String {
    let entries = (0..count).map(|n| (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes()));
    let target = NonZeroU32::new(target).unwrap();
    let tree = run(Tree::build(&mut MemStore::new(), target, entries)).unwrap();
    tree.root().unwrap().to_string()
}

#[test]
fn chunks_under_the_cap_keep_their_hashes() {
    let pinned = [
        (
            32,
            20_000,
            "a87291f7380622867115566187ea7aa84d33c73e3715bc692d894c656515c860",
        ),
        (
            4,
            2_000,
            "fca015a2f47339d3e58912224647ca03193f0231abb990b937dfec6b4143ddc4",
        ),
        (
            1,
            300,
            "b9ca685c5950d65d7e2cafacd2bd385d4a92bce6959617bb014f45ceb27df221",
        ),
        (
            32,
            1,
            "0e03ec5b24357da7e0ee46bcde1df496b1e63df8fc5dea4c92a903f16aec768e",
        ),
    ];
    for (target, count, hash) in pinned {
        assert_eq!(
            root(target, count),
            hash,
            "target {target}, {count} entries"
        );
    }
}
