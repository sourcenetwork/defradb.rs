use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DiskANN shared quantizer",
        name: "INV_EncodesWithItsCodebooks — a write sharing the node's quantizer encodes only with its own snapshot's codebooks, across rebuilds and retried trainings",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/mod.rs DiskAnn::quantizer; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer; crates/database/db/src/index/vector/engine/diskann/build.rs train_and_build",
        model_ref: "MC_SharedQuantizer_Green.cfg (GREEN) / MC_SharedQuantizer_Red_NoTagCheck.cfg / MC_SharedQuantizer_Red_TagPerRequest.cfg / MC_SharedQuantizer_Red_OneSlot.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_shared_quantizer.rs a_write_shares_the_quantizer_under_its_training, a_quantizer_under_another_training_is_not_used, writers_straddling_a_rebuild_keep_both_quantizers",
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "decode_encode — the trained record reads back as written in both encodings, and the shared quantizer answers a tag only with that tag's codebooks",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs encode_trained, decode_trained; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer",
        model_ref: "SharedQuantizer.Tagged.decode_encode / lookup_is_books / share_keeps_consistent / reused_tag_serves_other_books / share_keeps_previous / one_slot_evicts",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_shared_quantizer.rs trained_record_reads_back_with_and_without_its_training, a_quantizer_under_another_training_is_not_used",
    },
];
