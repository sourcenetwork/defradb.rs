//! `defradb start` and `defradb storage stats` refuse a data directory Go
//! DefraDB wrote, through the binary and before the storage engine opens it.

use std::path::Path;
use std::time::Duration;

use nuclei::process::Command;

/// The header Badger v4 writes at the top of a new `MANIFEST`: its magic text
/// and format version 8, big-endian.
const BADGER_V4_HEADER: [u8; 8] = *b"Bdgr\0\0\0\x08";

/// Every process this file starts carries a deadline, so a regression fails
/// instead of hanging the suite.
const DEADLINE: Duration = Duration::from_secs(60);

/// A root directory whose `data` folder holds a Badger datastore, as the
/// rootdir a Go DefraDB node created does.
fn go_rootdir() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("data")).unwrap();
    std::fs::write(root.path().join("data/MANIFEST"), BADGER_V4_HEADER).unwrap();
    root
}

/// The names in the root's data folder, sorted.
fn data_entries(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("data"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Runs `defradb --rootdir <root> <args>` to its exit, within [`DEADLINE`],
/// and returns its status and stderr. A keyring or telemetry setup would write
/// into the root, so both are off: the refusal is what the run must reach.
async fn run(root: &Path, args: &[&str]) -> (std::process::ExitStatus, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_defradb"));
    command
        .arg("--rootdir")
        .arg(root)
        .args(["--no-keyring", "--no-telemetry"])
        .args(args)
        .env("RUST_LOG", "info")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = nuclei::time::timeout(DEADLINE, command.output())
        .await
        .expect("defradb exits before the deadline")
        .expect("run defradb");
    (
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// What the refusal says: the directory, what it holds, and the schema-first
/// move Go's documents take.
fn assert_refused(stderr: &str, data: &Path) {
    assert!(
        stderr.contains(&format!(
            "the data directory '{}' holds a Badger datastore written by Go DefraDB",
            data.display()
        )),
        "the refusal names the directory in:\n{stderr}"
    );
    assert!(
        stderr.contains("add the same schema to it first"),
        "the refusal names the schema step in:\n{stderr}"
    );
    assert!(
        stderr.contains("`defradb client backup export`")
            && stderr.contains("`defradb client backup import`"),
        "the refusal names the backup commands in:\n{stderr}"
    );
}

/// `defradb start --rootdir <go rootdir>` opens `config.data_path()` in this
/// process, so this is the path a Go rootdir takes; `defradb storage stats`
/// opens it read-only. Both refuse it, and neither changes a byte of it.
#[nuclei::test]
async fn the_binary_refuses_a_go_datastore_before_start_and_storage_stats() {
    let root = go_rootdir();
    let data = root.path().join("data");

    let (status, stderr) = run(
        root.path(),
        &[
            "--url",
            "127.0.0.1:0",
            "start",
            "--store",
            "regolith",
            "--no-p2p",
        ],
    )
    .await;
    assert!(
        !status.success(),
        "start accepted a Go datastore:\n{stderr}"
    );
    assert_refused(&stderr, &data);
    assert_eq!(data_entries(root.path()), ["MANIFEST"]);
    assert_eq!(
        std::fs::read(data.join("MANIFEST")).unwrap(),
        BADGER_V4_HEADER
    );

    let (status, stderr) = run(root.path(), &["storage", "stats"]).await;
    assert!(
        !status.success(),
        "storage stats accepted a Go datastore:\n{stderr}"
    );
    assert_refused(&stderr, &data);
    assert_eq!(data_entries(root.path()), ["MANIFEST"]);
    assert_eq!(
        std::fs::read(data.join("MANIFEST")).unwrap(),
        BADGER_V4_HEADER
    );
}
