#![recursion_limit = "4096"]

mod chunk {
    mod access_local;
    mod backup;
    mod concurrency;
    mod declare_collection;
    mod declare_definition;
    mod declare_string_only;
    mod lifecycle;
    mod locality;
    mod protect_api_writes;
    mod protect_list_input;
    mod protect_no_mutations;
    mod protection;
    mod replication;
    mod revert;
    mod support;
}
