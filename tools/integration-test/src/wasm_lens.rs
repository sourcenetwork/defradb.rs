//! The `set_default` WASM lens module the legacy schema migration tests load.

use std::path::PathBuf;
use std::process::Command;

use regolith::sync::OnceCell;

use crate::workspace_root;

/// The outcome of the one build per process.
static BUILD: OnceCell<Result<(), String>> = OnceCell::new();

/// A WASM lens crate built for `wasm32-unknown-unknown` into its own
/// `target` directory.
pub struct WasmLens {
    source_dir: PathBuf,
}

impl WasmLens {
    pub fn new(source_dir: PathBuf) -> Self {
        Self { source_dir }
    }

    /// Build the module, once per process.
    ///
    /// The output is pinned with `--target-dir <source>/target`, which beats an
    /// inherited `CARGO_TARGET_DIR`, so the module lands where
    /// [`Self::module_path`] reads it.
    pub fn build(&self) -> Result<(), String> {
        // Racing first callers serialize on cargo's target-dir lock, and a
        // repeat build is a cargo no-op.
        BUILD.get_or_init_racy(|| self.do_build()).clone()
    }

    /// The `cargo build` that puts the module at [`Self::wasm_file_path`].
    fn build_command(&self) -> Command {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(self.source_dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(self.source_dir.join("target"));
        cmd
    }

    fn do_build(&self) -> Result<(), String> {
        let status = self
            .build_command()
            .status()
            .map_err(|e| format!("failed to run cargo build for WASM lens: {e}"))?;
        if !status.success() {
            return Err(format!(
                "cargo build failed for WASM lens at {} ({status}); is the \
                 wasm32-unknown-unknown target installed? \
                 Run: rustup target add wasm32-unknown-unknown",
                self.source_dir.display()
            ));
        }
        let wasm = self.wasm_file_path();
        if !wasm.exists() {
            return Err(format!(
                "WASM file not found after build at {}",
                wasm.display()
            ));
        }
        Ok(())
    }

    /// The `file://` URL of the built module.
    pub fn module_path(&self) -> String {
        format!("file://{}", self.wasm_file_path().display())
    }

    /// The built module's bytes.
    pub fn module_bytes(&self) -> Vec<u8> {
        std::fs::read(self.wasm_file_path()).expect("read WASM file")
    }

    /// `<source>/target/wasm32-unknown-unknown/debug/<crate>_lens.wasm`, the
    /// crate name taken from the source directory.
    fn wasm_file_path(&self) -> PathBuf {
        let crate_name = self
            .source_dir
            .file_name()
            .expect("source_dir has no file name")
            .to_string_lossy()
            .replace('-', "_");
        self.source_dir
            .join("target/wasm32-unknown-unknown/debug")
            .join(format!("{crate_name}_lens.wasm"))
    }
}

/// The `set_default` lens in `tools/integration-test/test-lenses`.
pub fn wasm_lens_defra() -> WasmLens {
    WasmLens::new(workspace_root().join("tools/integration-test/test-lenses/set_default"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2350: an inherited `CARGO_TARGET_DIR` (VMs, agent worktrees) must not
    /// move the module away from where the tests read it.
    #[test]
    fn build_ignores_an_inherited_cargo_target_dir() {
        let lens = wasm_lens_defra();
        let stray = lens.source_dir.join("target/stray-cargo-target-dir");
        let _ = std::fs::remove_dir_all(&stray);
        let status = lens
            .build_command()
            .env("CARGO_TARGET_DIR", &stray)
            .status()
            .expect("run cargo build for the WASM lens");
        let strayed = stray
            .join("wasm32-unknown-unknown/debug/set_default_lens.wasm")
            .exists();
        let _ = std::fs::remove_dir_all(&stray);
        assert!(status.success(), "cargo build failed: {status}");
        assert!(!strayed, "the module was built into CARGO_TARGET_DIR");
        assert!(
            lens.wasm_file_path().exists(),
            "no module at {}",
            lens.wasm_file_path().display()
        );
    }
}
