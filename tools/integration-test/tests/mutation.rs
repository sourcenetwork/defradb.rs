#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod mutation {
    mod add {
        mod constraints {
            mod size_constraint;
        }
        mod crdt {
            mod pcounter;
            mod pncounter;
        }
        mod embeddings {
            mod embedding;
        }
        mod field_kinds {
            mod field_kind_blob;
            mod field_kind_bool;
            mod field_kind_date_time;
            mod field_kind_float;
            mod field_kind_int;
            mod field_kind_json;
            mod field_kind_string;
            mod typed_nillable_arrays;
            mod one_to_many {
                mod with_alias;
                mod with_simple;
            }
            mod one_to_one {
                mod with_alias;
                mod with_null_value;
                mod with_simple;
            }
            mod one_to_one_to_one {
                mod with_txn;
            }
        }
        mod simple;
        mod simple_add_many;
        mod txn_discard;
        mod with_default_values;
        mod with_delete;
        mod with_null_input;
        mod with_null_value;
        mod with_variables;
        mod with_version;
    }
    mod delete {
        mod field_kinds {
            mod one_to_many {
                mod with_show_deleted;
            }
            mod one_to_one_to_one {
                mod with_id;
                mod with_txn;
            }
        }
        mod simple;
        mod with_deleted_field;
        mod with_filter;
        mod with_filter_action;
        mod with_id;
        mod with_id_alias;
        mod with_id_txn;
        mod with_ids;
        mod with_ids_alias;
        mod with_ids_filter;
        mod with_ids_txn;
        mod with_ids_update_alias;
        mod with_null_input;
    }
    mod mix {
        mod with_multiple_operations;
        mod with_txn;
    }
    mod purge;
    mod save_many;
    mod special {
        mod invalid_operation;
    }
    mod truncate {
        mod docid;
        mod filter;
        mod simple;
    }
    mod update {
        mod concurrent;
        mod constraints {
            mod size_constraint;
        }
        mod crdt {
            mod pcounter;
            mod pncounter;
        }
        mod embeddings {
            mod embedding;
        }
        mod field_kinds {
            mod array_bool;
            mod array_float;
            mod array_int;
            mod array_nillable_bool;
            mod array_nillable_float;
            mod array_nillable_int;
            mod array_nillable_string;
            mod array_string;
            mod blob;
            mod bool;
            mod date_time;
            mod float;
            mod int;
            mod json;
            mod one_to_many {
                mod simple;
                mod with_alias;
            }
            mod one_to_one {
                mod with_alias;
                mod with_self_ref;
                mod with_simple;
            }
            mod string;
        }
        mod underscored_collection;
        mod with_default_values;
        mod with_delete;
        mod with_filter;
        mod with_filter_action;
        mod with_id;
        mod with_id_and_version;
        mod with_ids;
        mod with_null_input;
    }
    mod upsert {
        mod date_time;
        mod simple;
    }
}
