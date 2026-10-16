use std::path::PathBuf;

/// Rust worktree prefix
pub const RUST_WORKTREE_PREFIX: &str = "defradb.rs";

/// Go worktree prefix
pub const GO_WORKTREE_PREFIX: &str = "defradb";

/// Environment variable naming the Go DefraDB checkout
pub const GO_REPO_ENV: &str = "DEFRADB_GO_REPO";

/// Report retention count per branch+package
pub const REPORT_RETENTION_COUNT: usize = 10;

/// Get the reports directory
pub fn reports_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".defra-ffi-reports")
        .join("runs")
}

/// Cargo's release output directory for a worktree.
///
/// Honors `CARGO_TARGET_DIR` the same way cargo does (a relative value is
/// resolved against the worktree), so the library is found wherever cargo
/// actually put it.
pub fn release_dir(rust_path: &std::path::Path) -> PathBuf {
    let target = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) if !dir.is_empty() => rust_path.join(dir),
        _ => rust_path.join("target"),
    };
    target.join("release")
}

/// Overrides the per-package Go build cache with one shared directory.
///
/// Sequential runs (CI, full sweeps) don't need the per-package isolation and
/// would otherwise compile the whole Go dependency graph once per package.
pub const GOCACHE_OVERRIDE_ENV: &str = "FFI_TEST_GOCACHE";

/// FFI crate path relative to Rust worktree root
pub const FFI_CRATE_PATH: &str = "crates/ffi";

/// cbindgen config path relative to Rust worktree root
pub const CBINDGEN_CONFIG: &str = "crates/ffi/cbindgen.toml";

/// Header destination relative to Go worktree root
pub const HEADER_DESTINATION: &str = "tests/clients/rustffi/defra.h";

/// FFI library name (without lib prefix or extension)
pub const FFI_LIB_NAME: &str = "ffi";

/// Library destination relative to Go worktree root
pub const LIBRARY_DESTINATION: &str = "tests/clients/rustffi";

/// Expected library name in Go worktree (without lib prefix or extension)
pub const GO_FFI_LIB_NAME: &str = "defra_ffi";
