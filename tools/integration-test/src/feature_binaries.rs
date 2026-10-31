use regolith::sync::OnceCell;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static SNAPSHOT_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

/// Resolve a CLI snapshot with the Vera providers enabled.
pub fn vera_cli_binary() -> PathBuf {
    static BINARY: OnceCell<PathBuf> = OnceCell::new();
    // Racing first callers serialize on the build lock in `build_cli_variant`, and a repeat
    // build is a cargo no-op, so the racy init costs a second snapshot copy at worst.
    BINARY
        .get_or_init_racy(|| {
            std::env::var_os("DEFRA_RUST_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    build_cli_variant(&crate::workspace_root(), &["vera"], "defra-vera")
                })
        })
        .clone()
}

/// Where the running test binary was built: its target directory, the
/// profile, and that profile's output directory. Cargo puts a test binary in
/// `<target>/<profile dir>/deps/`, and the `dev` profile's directory is `debug`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestBuild {
    pub target_dir: PathBuf,
    pub profile: String,
    pub out_dir: PathBuf,
}

impl TestBuild {
    /// The build of the running test, or `dev` under `<workspace>/target`
    /// when the executable is not in a cargo `deps` directory.
    pub fn current(workspace: &Path) -> Self {
        std::env::current_exe()
            .ok()
            .and_then(|exe| Self::of(&exe))
            .unwrap_or_else(|| Self {
                target_dir: workspace.join("target"),
                profile: "dev".into(),
                out_dir: workspace.join("target/debug"),
            })
    }

    fn of(exe: &Path) -> Option<Self> {
        let deps = exe.parent().filter(|dir| dir.ends_with("deps"))?;
        let out_dir = deps.parent()?;
        let profile = match out_dir.file_name()?.to_str()? {
            "debug" => "dev",
            name => name,
        };
        Some(Self {
            target_dir: out_dir.parent()?.to_path_buf(),
            profile: profile.into(),
            out_dir: out_dir.to_path_buf(),
        })
    }
}

/// Whether the harness was started only to list its tests (`nextest list`,
/// `--list`), which must not build anything.
pub fn listing_tests() -> bool {
    std::env::args().any(|arg| arg == "--list")
}

/// Build and snapshot a CLI feature variant without racing other test
/// processes, on the profile and into the target directory the tests were
/// built with.
pub fn build_cli_variant(workspace: &Path, features: &[&str], output_name: &str) -> PathBuf {
    let build = TestBuild::current(workspace);
    let target_dir = build.out_dir.clone();
    fs::create_dir_all(&target_dir).expect("failed to create target directory");

    let build_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(target_dir.join(".defra-build.lock"))
        .expect("failed to open defra build lock");
    build_lock
        .lock()
        .expect("failed to acquire defra build lock");

    let mut command = Command::new("cargo");
    command.args(["build", "-p", "cli", "--profile", &build.profile]);
    command.arg("--target-dir").arg(&build.target_dir);
    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }

    let status = command
        .current_dir(workspace)
        .status()
        .expect("failed to build defra test binary");
    assert!(status.success(), "cargo build -p cli failed");

    let source = target_dir.join(format!("defra{}", std::env::consts::EXE_SUFFIX));
    let destination = target_dir.join(format!("{output_name}{}", std::env::consts::EXE_SUFFIX));
    let temporary = temporary_snapshot_path(&target_dir, output_name);

    fs::copy(&source, &temporary).expect("failed to snapshot defra test binary");
    publish_snapshot(temporary, destination)
}

fn temporary_snapshot_path(target_dir: &Path, output_name: &str) -> PathBuf {
    let sequence = SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    target_dir.join(format!(
        "{output_name}-{}-{sequence}{}",
        std::process::id(),
        std::env::consts::EXE_SUFFIX
    ))
}

fn publish_snapshot(temporary: PathBuf, destination: PathBuf) -> PathBuf {
    match fs::rename(&temporary, &destination) {
        Ok(()) => destination,
        Err(error) => {
            assert!(
                temporary.is_file(),
                "failed to publish defra test binary and temporary snapshot disappeared: {error}"
            );
            eprintln!(
                "failed to publish defra test binary to {}; using {}: {error}",
                destination.display(),
                temporary.display()
            );
            temporary
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_binary_names_its_profile_and_target_dir() {
        let custom = TestBuild::of(Path::new("/w/target/super-dev/deps/p2p-1a2b")).unwrap();
        assert_eq!(
            custom,
            TestBuild {
                target_dir: "/w/target".into(),
                profile: "super-dev".into(),
                out_dir: "/w/target/super-dev".into(),
            }
        );
        let dev = TestBuild::of(Path::new("/shared/debug/deps/p2p-1a2b")).unwrap();
        assert_eq!(dev.profile, "dev");
        assert_eq!(dev.target_dir, PathBuf::from("/shared"));
        assert_eq!(TestBuild::of(Path::new("/usr/bin/defra")), None);
    }

    #[test]
    fn snapshot_paths_are_unique_within_a_process() {
        let directory = tempfile::tempdir().expect("temporary directory");

        let first = temporary_snapshot_path(directory.path(), "defra-iroh");
        let second = temporary_snapshot_path(directory.path(), "defra-iroh");

        assert_ne!(first, second);
    }

    #[test]
    fn publishes_snapshot_to_stable_destination() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let temporary = directory.path().join("defra-iroh-123");
        let destination = directory.path().join("defra-iroh");
        fs::write(&temporary, b"snapshot").expect("write snapshot");

        let published = publish_snapshot(temporary.clone(), destination.clone());

        assert_eq!(published, destination);
        assert_eq!(
            fs::read(published).expect("read published binary"),
            b"snapshot"
        );
        assert!(!temporary.exists());
    }

    #[test]
    fn retains_temporary_snapshot_when_destination_cannot_be_replaced() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let temporary = directory.path().join("defra-iroh-123");
        let destination = directory.path().join("defra-iroh");
        fs::write(&temporary, b"snapshot").expect("write snapshot");
        fs::create_dir(&destination).expect("create non-replaceable destination");

        let published = publish_snapshot(temporary.clone(), destination.clone());

        assert_eq!(published, temporary);
        assert_eq!(
            fs::read(published).expect("read retained binary"),
            b"snapshot"
        );
        assert!(destination.is_dir());
    }
}
