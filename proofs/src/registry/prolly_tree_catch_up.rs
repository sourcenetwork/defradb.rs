use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Lean models chunking under any close rule that reads the item and
        // its open chunk, the byte cap that bounds the open chunk
        // (`open_bounded`), `build` over a sorted list, the one-pass
        // `Builder`, the windowed edit level by level and walked up to the
        // root with the single-child `collapse`, `diff`'s walk against a
        // per-key comparison, the injective node encoding, and its decoder,
        // which reads a node's bytes back and refuses bytes after them. SHA-256 is
        // trusted collision-free; an edit's window is any run of chunks that
        // holds the change, where Rust finds it by key.
        family: "Prolly tree catch-up",
        name: "chunks_append / closes_append / chunks_flatten / chunks_nonempty / chunks_shrink / open_bounded / build / sorted_insert / sorted_erase / eq_of_same_entries / build_same_entries / insert_comm / erase_insert_comm / insert_insert / canon_chunks / rewrite_level_refines / rewrite_level_up / walk_levels / edit_root / builder_root / diff_eq / mergeDiff_spec / encode_injective / decode_encode / decode_refuses_trailing",
        axis: Lean,
        anchor: "crates/foundation/prolly/src/chunk.rs Chunker::push, Chunker::finish, chunk; crates/foundation/prolly/src/hash.rs is_boundary; crates/foundation/prolly/src/tree.rs Tree::build, raise, collapse; crates/foundation/prolly/src/build.rs Builder::push, Builder::finish, Builder::feed, Builder::close; crates/foundation/prolly/src/edit.rs Tree::edit, rewrite_level, splice; crates/foundation/prolly/src/diff.rs diff, Walk::open, Walk::take; crates/foundation/prolly/src/node.rs Node::encode, Node::decode, Item::encoded_len, MAX_CHUNK_BYTES",
        model_ref: "ProllyTree (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pull is not wired into p2p yet (#2977); the model is its contract.
        // Each pull is one atomic step and read access never changes, so
        // "caught up" means caught up with the responder as it was at that
        // step, and a read revoked mid-pull is not checked. PulledHolds is
        // about pulls whose walk completes: `ProllyTree.diff_eq` and
        // `ProllyTree.mergeDiff_spec` show a completed walk reports exactly the
        // differing keys, but not that the walk ends or that every node read
        // succeeds. A walk that fails makes no "caught up" claim and changes
        // nothing, a stutter step the model already allows.
        family: "Prolly tree catch-up",
        name: "NoLeak / PulledHolds: a pull never shows a peer a document it may not read, and a pull that ends caught up leaves the puller holding every version the responder could show it at that pull",
        axis: Tla,
        anchor: "crates/foundation/prolly/src/tree.rs Tree::build, Tree::root; crates/foundation/prolly/src/diff.rs diff",
        model_ref: "MC_ProllySync_Green.cfg (GREEN) / MC_ProllySync_{Red_NoFilter,Red_StaleRoot,Red_MissingOnly,Red_KeepClaim}.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
