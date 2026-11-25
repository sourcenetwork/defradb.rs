use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "kms, db, db-merge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The unauthorized-denied leg is now observable: a serve-side
        // `tracing::warn!("DEK release DENIED")` at the KMS policy check makes the
        // previously-silent denial loggable, and the behavioral test reads node0's
        // log to confirm node0 denies the DEK to an unauthorized node1's fetch.
        // ECIES secrecy of a released envelope remains a crypto Boundary (proven
        // by crates/database/kms ecies_envelope unit tests).
        family: "KMS key distribution",
        name: "INV_AuthorizedEventuallyGets / no-unauthorized-usable",
        axis: Tla,
        anchor: "crates/database/kms/src/defra_kms.rs serve_request; crates/database/kms/src/nac_dac_policy.rs",
        model_ref: "MC_Kms_Green.cfg",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/kms.rs kms_unauthorized_dek_release_denied",
    },
    Property {
        family: "KMS key distribution",
        name: "chunks_flatten / chunk_size_le / answers_in_order / answers_length: a key batch split for a transport asks for every key once, no chunk over the per-request limit, and answers every key in its place",
        axis: Lean,
        anchor: "crates/database/kms/src/defra_kms.rs DefraKms::with_inputs, CHUNKS_IN_FLIGHT; crates/building-blocks/inputs/src/service.rs Inner::inline, Inner::answers",
        model_ref: "KmsChunks (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/kms/src/remote_tests.rs a_batch_is_split_to_the_keys_a_transport_carries, a_split_batch_keeps_few_chunks_on_the_wire_at_once, a_retryable_send_failure_of_one_chunk_is_retried",
    },
    Property {
        family: "KMS key distribution",
        name: "WindowBound / PermitBound / RetriedBeforeError / OwnOutcome / AllAnswered: an inline call's chunks run at most flights_per_group at once and max_in_flight on the wire, a chunk errs only after its retries, each chunk's keys get that chunk's own outcome, and every chunk is answered",
        axis: Tla,
        anchor: "crates/building-blocks/inputs/src/service.rs Inner::inline, Inner::round_trip, Inner::answers; crates/database/kms/src/defra_kms.rs DefraKms::with_inputs, CHUNKS_IN_FLIGHT",
        model_ref: "MC_InlineBatchFlights_Green.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/kms/src/remote_tests.rs a_split_batch_keeps_few_chunks_on_the_wire_at_once, a_retryable_send_failure_of_one_chunk_is_retried",
    },
    Property {
        // A merge that needs a DEK fetches it inline on the serial merge
        // executor, blocking unrelated merges for up to RESPONSE_TIMEOUT (5 s,
        // plus up to SUBSCRIBER_WAIT_TIMEOUT, 10 s, when no subscriber is
        // known). KeyUnavailable is retryable, so an unauthorized replica pays
        // this on every re-drive.
        family: "KMS key distribution",
        name: "LIVE_UnrelatedMergeProgress is not claimed: a merge awaiting a DEK holds the merge executor",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/dispatch.rs merge_with_retries; crates/database/p2p/src/kms/pubsub_transport.rs RESPONSE_TIMEOUT, SUBSCRIBER_WAIT_TIMEOUT",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A write asks the KMS once per key scope: every field under document
        // encryption shares the document key, and a field key is never it.
        // Exercised by the testo scenarios
        // encryption::field_commit::*_should_use_dedicated_enc_key_for_individual_fields.
        family: "KMS key distribution",
        name: "DocKeyScopes.same_scope_same_key / different_scope_different_key",
        axis: Lean,
        anchor: "crates/database/db/src/write/prepare.rs KMS key preparation (`generated` memo)",
        model_ref: "DocKeyScopes (lake build)",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/encryption/field_commit.rs doc_encryption_field_with_doc_and_field_encryption_should_use_dedicated_enc_key_for_individual_fields",
    },
];
