use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Every signing key lives in one process-wide store, one slot per
        // DID. Holders (FFI identity handles, nodes) take and give back
        // holds; the hold count, the pin and the key change together in one
        // swap, a slot never leaves the map, and a clear only unpins and
        // drops keys nothing holds. While a DID has a key its signer is
        // fixed: a hold with another signer, or a pin over a held DID, is
        // refused, so no holder is re-routed to a host context it did not
        // bring.
        family: "Signing identity store",
        name: "INV_CountExact / INV_KeyWhileHeld / INV_KeyMatchesHolders / INV_OwnSigner / INV_AcceptSame: a DID's slot counts its holds exactly, a held DID always answers its key, no key outlives its holders unless pinned, no holder signs through a signer it did not bring, and a hold with the held host context is never refused",
        axis: Tla,
        anchor: "crates/foundation/defra-core/src/signing/identity_store.rs hold_identity, retain_identity, release_identity, store_identity, clear_identity_store, same_signer, IdentityHold; crates/foundation/defra-core/src/signing.rs RemoteSigner::host_context; crates/entry-points/ffi/src/remote_signer/mod.rs FfiRemoteSigner::host_context; crates/entry-points/ffi/src/remote_signer/register.rs store",
        model_ref: "MC_IdentityStore_{Green,Red_SplitSwap,Red_DropCount,Red_ClearRemoves,Red_ClearDropsHeld,Red_RetainAbsent,Red_ReplaceOnHold,Red_PinReplacesHeld,Red_RefuseSameContext}.cfg",
        tiers: &[Contract],
        bound_by: "crates/foundation/defra-core/src/signing/identity_store_tests.rs the_last_hold_on_an_identity_removes_its_key, a_retained_identity_outlives_the_hold_it_was_taken_from, a_pinned_identity_survives_its_holds, a_clear_keeps_held_keys_and_their_count, a_held_did_refuses_another_signer, a_pin_replaces_only_an_unheld_signer, a_held_did_takes_a_signer_with_the_same_host_context; crates/entry-points/ffi/src/remote_signer/registration_tests.rs a_second_registration_of_a_held_did_is_refused_until_the_first_is_freed, a_second_registration_with_the_same_context_shares_the_signer",
    },
    Property {
        family: "Signing identity store",
        name: "empty_keeps / hold_keeps / hold_keeps_signer / hold_accepts_same / hold_refuses_other / retain_keeps / retain_answers_key / release_keeps / pin_keeps / pin_keeps_held_signer / clear_keeps / held_has_key / free_has_no_key: every operation keeps the count exact and the key present exactly while pinned or held, a held DID's signer never changes, and a hold with its own signer is taken",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/signing/identity_store.rs Holders, same_signer, hold_identity, retain_identity, release_identity, store_identity, clear_identity_store",
        model_ref: "IdentityStore (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
