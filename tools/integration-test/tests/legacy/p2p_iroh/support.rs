use std::path::PathBuf;

use integration_test::{build_cli_variant, workspace_root};

#[ctor::ctor(unsafe)]
fn prepare_iroh_binary() {
    // Listing tests runs this constructor too, before nextest's setup script
    // has exported the binaries, so a list must not build one.
    if integration_test::listing_tests() {
        return;
    }
    if std::env::var_os("DEFRA_IROH_BINARY")
        .zip(std::env::var_os("DEFRA_RUST_BINARY"))
        .is_some_and(|(iroh, rust)| PathBuf::from(iroh).is_file() && PathBuf::from(rust).is_file())
    {
        return;
    }

    let binary = build_cli_variant(&workspace_root(), &["iroh"], "defra-iroh");

    std::env::set_var("DEFRA_IROH_BINARY", &binary);
    std::env::set_var("DEFRA_RUST_BINARY", &binary);
}
