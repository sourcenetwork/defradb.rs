use std::path::PathBuf;

use integration_test::{build_cli_variant, workspace_root};
use regolith::sync::OnceCell;

static VERA_IROH_BINARY: OnceCell<PathBuf> = OnceCell::new();

#[ctor::ctor]
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

pub fn vera_binary_available() -> bool {
    std::env::var_os("VERA_BINARY").is_some()
        || std::env::var_os("VERA_WORKSPACE").is_some()
        || path_contains_binary("verad")
}

pub fn vera_iroh_binary() -> PathBuf {
    VERA_IROH_BINARY
        .get_or_init_racy(|| {
            build_cli_variant(&workspace_root(), &["iroh", "vera"], "defra-iroh-vera")
        })
        .clone()
}

fn path_contains_binary(name: &str) -> bool {
    let binary = format!("{}{}", name, std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(&binary).is_file()))
        .unwrap_or(false)
}
