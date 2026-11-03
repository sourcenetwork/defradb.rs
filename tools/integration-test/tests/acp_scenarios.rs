#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

#[path = "acp_scenarios"]
mod acp {
    mod dac {
        mod add_policy {
            mod basic;
            mod with_empty_args;
            mod with_empty_relations;
            mod with_empty_resource;
            mod with_extra_perms;
            mod with_extra_perms_and_relations;
            mod with_extra_relations;
            mod with_managed_relation;
            mod with_multi_policies;
            mod with_multiple_resources;
            mod with_no_perms;
            mod with_no_resources;
            mod with_perm_expr;
            mod with_perm_invalid_expr;
            mod with_unused_relations;
        }
        mod avg;
        mod branchable {
            mod collection_commits;
            mod collection_mutation;
            mod peer;
        }
        mod commits {
            mod basic;
            mod multi_collection;
            mod version_subselect;
            mod with_cid;
            mod with_cid_shared_field;
            mod with_doc_id;
            mod with_filters;
            mod with_relationship;
        }
        mod count;
        mod delete_with_index;
        mod index {
            mod new;
            mod query;
            mod query_with_relation;
            mod update_by_non_creator;
        }
        mod link_collection {
            mod accept_basic_dri_fmts;
            mod accept_extra_permissions_on_dri;
            mod accept_managed_relation_on_dri;
            mod accept_mixed_resources_on_partial_dri;
            mod accept_multi_dris;
            mod accept_multi_resources_on_dri;
            mod accept_permission_with_omitted_owner_authority;
            mod accept_same_resource_on_diff_collections;
            mod reject_empty_arg_on_collection;
            mod reject_invalid_arg_type_on_collection;
            mod reject_invalid_owner_delete_perm_on_dri;
            mod reject_invalid_owner_read_perm_on_dri;
            mod reject_invalid_owner_update_perm_on_dri;
            mod reject_missing_dri;
            mod reject_missing_id_arg_on_collection;
            mod reject_missing_perms_on_dri;
            mod reject_missing_resource_arg_on_collection;
            mod reject_missing_resource_on_dri;
        }
        mod p2p {
            mod add;
            mod delete;
            mod replicator;
            mod replicator_with_doc_actor_relationship;
            mod subscribe;
            mod subscribe_grant_race;
            mod subscribe_with_doc_actor_relationship;
            mod sync_shared_field;
            mod update;
            mod update_by_non_creator_on_peer;
        }
        mod query {
            mod with_cid_shared_field;
        }
        mod register_and_delete;
        mod register_and_read;
        mod register_and_update;
        mod relation_objects;
        mod relationship {
            mod doc_actor {
                mod add {
                    mod invalid;
                    mod with_delete;
                    mod with_dummy_relation;
                    mod with_manager;
                    mod with_manager_gql;
                    mod with_no_policy_on_collection;
                    mod with_only_delete;
                    mod with_only_update;
                    mod with_only_update_gql;
                    mod with_public_document;
                    mod with_reader;
                    mod with_reader_gql;
                    mod with_target_all_actors;
                    mod with_target_all_actors_gql;
                    mod with_update;
                    mod with_update_gql;
                }
                mod delete {
                    mod invalid;
                    mod with_delete;
                    mod with_dummy_relation;
                    mod with_manager;
                    mod with_no_policy_on_collection;
                    mod with_public_document;
                    mod with_reader;
                    mod with_self;
                    mod with_target_all_actors;
                    mod with_update;
                }
            }
        }
        mod subscription;
        mod truncate;
    }
    mod nac {
        mod add_dac_policy;
        mod add_dac_relationship;
        mod add_document;
        mod add_lens;
        mod add_nac_relationship;
        mod add_p2p_collection;
        mod add_p2p_document;
        mod add_p2p_replicator;
        mod add_view;
        mod connect_p2p_peer;
        mod dac_access_by_empty_user_nac_off;
        mod dac_access_by_node_owner_nac_on;
        mod dac_access_by_owner_nac_off;
        mod dac_access_by_wrong_user_nac_off;
        mod delete_dac_relationship;
        mod delete_document;
        mod delete_encrypted_index;
        mod delete_index;
        mod delete_nac_relationship;
        mod delete_p2p_collection;
        mod delete_p2p_document;
        mod delete_p2p_replicator;
        mod disable_nac;
        mod disconnect_p2p_peer;
        mod get_collection_by_id;
        mod get_collection_by_name;
        mod get_collection_by_version;
        mod get_p2p_active_peers;
        mod list_all_encrypted_index;
        mod list_collection;
        mod list_encrypted_index;
        mod list_index;
        mod list_lens;
        mod list_p2p_collection;
        mod list_p2p_document;
        mod list_p2p_replicator;
        mod nac_restart;
        mod nac_start;
        mod new_encrypted_index;
        mod new_index;
        mod refresh_view;
        mod relation_admin {
            mod add_collection;
            mod add_dac_policy;
            mod add_dac_relationship;
            mod add_document;
            mod add_lens;
            mod add_nac_relationship;
            mod add_p2p_collection;
            mod add_p2p_document;
            mod add_p2p_replicator;
            mod add_view;
            mod connect_p2p_peer;
            mod dac_bypass_with_nac_off;
            mod delete_dac_relationship;
            mod delete_document;
            mod delete_encrypted_index;
            mod delete_index;
            mod delete_nac_relationship;
            mod delete_p2p_collection;
            mod delete_p2p_document;
            mod delete_p2p_replicator;
            mod disable_nac;
            mod get_collection_by_id;
            mod get_collection_by_name;
            mod get_collection_by_version;
            mod get_nac_status;
            mod get_p2p_active_peers;
            mod list_all_encrypted_index;
            mod list_collection;
            mod list_encrypted_index;
            mod list_index;
            mod list_lens;
            mod list_p2p_collection;
            mod list_p2p_document;
            mod list_p2p_replicator;
            mod new_encrypted_index;
            mod new_index;
            mod re_enable_nac;
            mod refresh_view;
            mod set_migration;
            mod sync_p2p_branchable_collection;
            mod sync_p2p_collection_versions;
            mod sync_p2p_documents;
            mod truncate_collection;
            mod update_document;
            mod verify_signature;
        }
        mod set_migration;
        mod sync_p2p_branchable_collection;
        mod sync_p2p_collection_versions;
        mod sync_p2p_documents;
        mod truncate_collection;
        mod update_document;
        mod update_document_with_filter;
        mod verify_signature;
    }
}
