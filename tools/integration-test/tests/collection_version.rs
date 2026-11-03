#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod collection_version {
    mod add_one_one_data;
    mod aggregates {
        mod inline_array;
        mod simple;
        mod top_level;
    }
    mod branchable_directive;
    mod client;
    mod client_introspection {
        mod one_many;
        mod simple;
    }
    mod comment;
    mod crdt_type;
    mod describe_string_kind;
    mod duplicate_relation_name;
    mod filter;
    mod get_collection_version;
    mod group;
    mod input_type;
    mod migrations {
        mod add_lens;
        mod query {
            mod simple;
            mod with_collection_branch;
            mod with_doc_id;
            mod with_filter;
            mod with_incremental_migrations;
            mod with_index;
            mod with_inverse;
            mod with_p2p;
            mod with_p2p_collection_branch;
            mod with_restart;
            mod with_set_default;
            mod with_txn;
            mod with_update;
        }
        mod simple;
        mod with_txn;
    }
    mod nil_type;
    mod one_many;
    mod one_one_many_self_ref;
    mod patch_migration_numeric_fidelity;
    mod patch_migration_presence;
    mod similarity;
    mod simple;
    mod type_explicit_fields;
    mod updates {
        mod add {
            mod branchable;
            mod collections;
            mod embeddings;
            mod field {
                mod add;
                mod add_update;
                mod constraint {
                    mod size;
                }
                mod crdt {
                    mod composite;
                    mod invalid;
                    mod lww;
                    mod none;
                    mod object_bool;
                    mod pcounter;
                    mod pncounter;
                    mod string_typ;
                }
                mod kind {
                    mod blob;
                    mod bool;
                    mod bool_array;
                    mod bool_nil_array;
                    mod datetime;
                    mod doc_id;
                    mod float;
                    mod float32;
                    mod float32_array;
                    mod float32_nil_array;
                    mod float_array;
                    mod float_nil_array;
                    mod foreign_object;
                    mod foreign_object_array;
                    mod int;
                    mod int_array;
                    mod int_nil_array;
                    mod invalid;
                    mod json;
                    mod none;
                    mod string;
                    mod string_array;
                    mod string_nil_array;
                }
                mod simple;
                mod with_filter;
                mod with_index_sub;
                mod with_introspection;
                mod with_relation;
            }
            mod simple;
        }
        mod copy {
            mod field {
                mod simple;
                mod with_introspection;
            }
            mod name;
            mod simple;
        }
        mod move_ {
            mod field {
                mod simple;
            }
            mod name;
            mod simple;
        }
        mod remove {
            mod col_source_transform;
            mod collections;
            mod collections_txn;
            mod contraints;
            mod embeddings;
            mod fields {
                mod simple;
            }
            mod name;
            mod policy;
            mod version_id;
            mod view_txn;
        }
        mod replace {
            mod branchable;
            mod col_source_source_id;
            mod col_source_transform;
            mod contraints;
            mod embeddings;
            mod field {
                mod simple;
            }
            mod fields;
            mod id;
            mod indexes;
            mod is_active;
            mod is_active_txn;
            mod materialized;
            mod name;
            mod name_one_many;
            mod policy;
            mod query_source_query;
            mod query_source_transform;
            mod root_id;
            mod simple;
            mod sources;
            mod view_policy;
        }
        mod test {
            mod add_field;
            mod field {
                mod simple;
            }
            mod name;
        }
        mod with_version_branch;
    }
    mod vector_embedding;
    mod with_default_fields;
    mod with_inline_array;
    mod with_update_set_default;
}
