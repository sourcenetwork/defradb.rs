use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2742: an explicit config always encrypts its field, each field
        // without one follows its own heads, a headless field takes the
        // document key only on a wholly encrypted document, and no composite
        // links encryption. The premise is not monotone (remote plaintext
        // merges); a decision serializes before a later concurrent merge. A
        // later write of a request counts the request's earlier writes: as
        // prepared (written_fields) to mint its key ahead, and as built when
        // the build decides. A memo attempt decides on its own heads, and a
        // document key the stale preparation did not mint ends the attempt
        // until it is prepared, which happens only after a merge.
        family: "Encryption policy",
        name: "INV_FollowsOwnHeads / INV_NoDowngradeOfEncryptedField / INV_HeadlessAllEncrypted / INV_StaleDecisionSerializable / INV_SeesRequestWrites / INV_PreparedKeyCovers / INV_ReprepareOnlyWhenStale / INV_CompositeUnlinked / INV_ExplicitConfigEncrypts",
        axis: Tla,
        anchor: "crates/database/db/src/block/builder/write.rs field_encryption_source, DocScope::preparing, every_field_encrypted, doc_scope_fields, build_document_blocks, encrypt_with_new_key, write_delete_block_with_inputs; crates/database/db/src/block/builder/compute.rs compute_document_blocks; crates/database/db/src/write/prepare.rs prepare_keys_with_stores",
        model_ref: "MC_EncryptionPolicyInheritance_{Green,Green_Local,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/write/stale_doc_scope.rs a_stale_headless_decision_is_prepared_again",
    },
    Property {
        // #2742: a field with heads gets Go's answer; a headless field gets the
        // document key iff the document is non-empty and wholly encrypted;
        // plaintext heads are never encrypted without config; the composite is
        // never linked; a request's earlier pending writes count as heads.
        family: "Encryption policy",
        name: "EncryptionPolicy.with_heads_matches_go / headless_doc_scope_iff / unlisted_plaintext_heads_stay_plaintext / composite_unlinked / pending_field_follows_its_write / pending_plaintext_blocks_doc_scope / create_then_update_takes_doc_key",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/write.rs field_encryption_source, every_field_encrypted, DocScope::preparing, doc_scope_fields",
        model_ref: "EncryptionPolicy (lake build)",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/encryption/new_field.rs doc_encryption_field_level_update_keeps_unlisted_field_plaintext; tools/integration-test/tests/testo/encryption/doc_id.rs doc_encryption_doc_id_matches_go",
    },
];
