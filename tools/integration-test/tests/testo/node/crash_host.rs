/// Not a scenario: the process a `NewNode { own_process }` Rust node runs in,
/// started by the harness as `integration_test::crash::HOST_TEST`. Run on its
/// own it does nothing.
#[test]
#[ignore = "the harness runs it as a node's own process"]
fn serve() {
    integration_test::crash::host::serve();
}
