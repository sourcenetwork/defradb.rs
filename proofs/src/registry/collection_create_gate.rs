use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "acp, db, query (the collection create gate, #3490)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A create in a collection whose policy gates creates is decided
        // before it writes: on Vera's certified state outside the
        // transaction, or in the transaction under the local engine. Vera
        // decides again at registration; a document it does not hold is
        // hidden from everyone, a refusal is final, and a replica refused as
        // a create is parked after a bounded number of sends.
        family: "Collection create gate",
        name: "INV_NoSquat / INV_NoAnonymous / INV_HiddenUntilRegistered / INV_RefusalFinal / INV_ReplicaBounded: whoever holds a gated document's ID was admitted by the state the node decided on, no anonymous create commits, nobody reads a gated document Vera does not hold, a create refusal is never resent, and a replica's refused registration is sent a bounded number of times",
        axis: Tla,
        anchor: "crates/database/db/src/write/create/gate.rs create_gate; crates/database/db/src/write/prepare_create.rs resolve_create_decision, resolve_dac_policy; crates/database/db/src/write/autocommit/helpers.rs register_created_doc; crates/engine/acp/src/create_decision.rs authorize_create_through; crates/engine/acp/src/pending.rs PendingRegistration::hidden, create_refused, refuse_create; crates/engine/acp/src/external/registration.rs note_failure, parked_create, MAX_REPLICA_CREATE_SENDS; crates/engine/acp/src/external/voided.rs owe_again",
        model_ref: "MC_CreateGate_{Green,Green_Local,Red_NoPreCheck,Red_LocalUngated,Red_AnonymousAdmitted,Red_CreatorReadsOwed,Red_VoidDropsGated,Red_ResendRefused,Red_ReplicaUnbounded}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/vera/create_gate.rs a_gated_collection_admits_creators_its_policy_lets_create; tools/integration-test/tests/testo/acp/dac/create_gate/local_gate.rs a_refused_create_stores_nothing_and_an_admitted_one_takes_its_id, an_anonymous_create_in_a_gated_collection_is_refused; crates/engine/acp/src/external/registration.rs a_create_refusal_is_final_and_hides_the_document_from_its_creator, a_replica_refused_as_a_create_is_parked_after_its_bound; crates/engine/acp/src/external/voided_tests.rs a_voided_registration_with_derived_relationships_stays_hidden_from_its_creator",
    },
    Property {
        family: "Collection create gate",
        name: "decide_is_reference / arrow_needs_its_relation / other_never_admits: deciding `create` by one check per linked object is acp-rs's check_create, an arrow through an absent relationship never admits, and a name that is not an arrow admits nothing",
        axis: Lean,
        anchor: "crates/engine/acp/src/create_decision.rs authorize_create, admits",
        model_ref: "CreateDecision (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/src/create_decision_tests.rs an_arrow_holds_when_the_linked_object_grants_its_name, intersections_and_differences_combine_the_checks, an_arrow_through_a_relation_the_document_lacks_never_holds, a_name_that_is_not_an_arrow_never_admits",
    },
];
