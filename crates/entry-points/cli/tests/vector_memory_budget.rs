//! `datastore.vector_memory_budget` and `--vector-memory-budget`: the bytes a
//! node's vector indexes may keep in memory between searches.

use clap::Parser;
use cli::cli::{Cli, Command};
use cli::config::Config;

#[test]
#[mantra_macros::req_test("defra.node.vector_memory_budget")]
fn an_unset_budget_is_the_default() {
    let mut value = serde_json::to_value(Config::default()).unwrap();
    value["datastore"]
        .as_object_mut()
        .unwrap()
        .remove("vector_memory_budget");
    let config: Config = serde_json::from_value(value).unwrap();
    assert_eq!(config.datastore.vector_memory_budget, 256 << 20);
    assert_eq!(
        config.datastore.vector_memory_budget,
        db::index::vector::budget::DEFAULT_VECTOR_MEMORY_BUDGET
    );
}

#[test]
#[mantra_macros::req_test("defra.node.vector_memory_budget")]
fn the_flag_overrides_the_config_key() {
    let mut config: Config =
        serde_yaml::from_str("datastore:\n  vector_memory_budget: 1048576\n").unwrap();
    assert_eq!(config.datastore.vector_memory_budget, 1 << 20);

    let cli = Cli::try_parse_from(["defra", "start", "--vector-memory-budget=0"]).unwrap();
    let Command::Start(args) = cli.command else {
        panic!("expected start command")
    };
    args.apply_to_config(&mut config).unwrap();
    assert_eq!(config.datastore.vector_memory_budget, 0);
}
