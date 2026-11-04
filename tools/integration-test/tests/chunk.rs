#![recursion_limit = "4096"]

mod chunk {
    mod declare_collection;
    mod declare_definition;
    mod declare_string_only;
    mod protect_api_writes;
    mod protect_list_input;
    mod protect_no_mutations;
    mod support;
}
