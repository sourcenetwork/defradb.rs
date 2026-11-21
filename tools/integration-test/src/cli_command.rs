//! Spawning the defra CLI with its own root directory.
//!
//! The CLI reads `config.yaml` from its root directory, which defaults to
//! `$DEFRA_ROOTDIR` and then `~/.defradb`. A test that spawns it without
//! `--rootdir` would load the developer's own config, so every spawn goes
//! through [`cli_command`]. Go's `defradb` ignores `DEFRA_ROOTDIR`, so the
//! flag is the one form both binaries honour.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use regolith::sync::Lazy;

/// An empty root directory at one fixed path, shared by every test process.
/// A per-process temp dir would leak one directory per process: statics are
/// never dropped, so its cleanup would never run.
static ROOTDIR: Lazy<PathBuf> = Lazy::new(|| {
    let path = std::env::temp_dir().join("defra-cli-rootdir");
    std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
    path
});

/// A `Command` for `binary` (Rust `defradb` or Go `defradb`) with
/// `--rootdir` set to an empty shared directory, so no config from the
/// user's home leaks in. Calls that act on a node's own files pass that
/// node's root directory instead.
pub fn cli_command(binary: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(binary);
    command.arg("--rootdir").arg(&*ROOTDIR);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rootdir_arg(command: &Command) -> std::path::PathBuf {
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[0], "--rootdir");
        args[1].into()
    }

    /// The root directory is one fixed path shared by every test process,
    /// so runs do not leave a new directory behind in the temp dir each time.
    #[test]
    fn rootdir_is_a_fixed_existing_path() {
        let path = rootdir_arg(&cli_command("defra"));
        assert_eq!(path, std::env::temp_dir().join("defra-cli-rootdir"));
        assert!(path.is_dir(), "{} should exist", path.display());
    }
}
