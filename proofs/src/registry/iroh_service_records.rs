use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Iroh service records",
        name: "INV_SeqNeverGoesBack / INV_NoExpiredChosen / LIVE_NewestReached",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/database/p2p/src/iroh/records/refresh.rs Refresh::check; crates/database/p2p/src/iroh/records/task.rs Records::renew, Records::on_input, Records::rotate",
        model_ref: "MC_ServiceRecords_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node publishes a service record only after its embedder sets a
        // ServiceProfile, and only the CLI does. For NodeBuilder and FFI
        // nodes LIVE_NewestReached is vacuous.
        family: "Iroh service records",
        name: "LIVE_NewestReached and LIVE_Found assume the embedder sets a ServiceProfile",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::renew; crates/entry-points/cli/src/commands/start/server_p2p/iroh.rs",
        model_ref: "MC_ServiceRecords_Green.cfg / MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "INV_AskOnlyWhenEmpty / LIVE_Found",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A lookup asks only when the book has no live record for the
        // collection key. Records name holders of a collection, not of a root,
        // so a non-holder's live record suppresses Who.
        family: "Iroh service records",
        name: "LIVE_Found assumes a live record's peer holds the root, not only the collection",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "validate_sound / validate_complete / offer_never_lowers / offer_replaces_expired / offer_keeps_only_valid / live_not_expired / encodeKey_injective / no_secret_not_advertised / distinct_ids_distinct_keys / parse_print / providers_exclude_self / providers_nodup / providers_from_book / providers_within_cap / providers_sorted",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/records/verify.rs verify; crates/database/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/database/p2p/src/iroh/records/key.rs blind, advertised_key, CollectionKey::encode; crates/database/p2p/src/iroh/records/pointer.rs Pointer::print, Pointer::parse; crates/database/p2p/src/iroh/records/task.rs Records::providers, rank",
        model_ref: "ServiceRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
