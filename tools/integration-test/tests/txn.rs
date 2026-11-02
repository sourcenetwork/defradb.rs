#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod txn {
    mod add_collection;
    mod add_doc;
    mod add_lens;
    mod add_view;
    mod delete_doc;
    mod delete_encrypted_index;
    mod delete_index;
    mod get_collections;
    mod list_encrypted_indexes;
    mod list_indexes;
    mod list_lenses;
    mod neighbour;
    mod new_encrypted_index;
    mod new_index;
    mod patch_collection;
    mod refresh_view;
    mod request;
    mod set_active_collection_version;
    mod set_migration;
    mod truncate;
    mod ttl;
    mod update_doc;
    mod verify_signature;
}
