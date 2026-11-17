use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DEFRA_NODE_GIT_HASH_OVERRIDE");
    println!("cargo:rerun-if-env-changed=DEFRA_NODE_GIT_DIRTY_OVERRIDE");
    println!("cargo:rerun-if-env-changed=DEFRA_NODE_RELEASE_TAG_OVERRIDE");
    println!("cargo:rerun-if-env-changed=DEFRA_NODE_BUILD_TIME_OVERRIDE");

    // Git commit hash
    let git_hash = std::env::var("DEFRA_NODE_GIT_HASH_OVERRIDE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short=12", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DEFRA_NODE_GIT_HASH={}", git_hash.trim());

    // Git dirty flag
    let dirty = std::env::var("DEFRA_NODE_GIT_DIRTY_OVERRIDE")
        .ok()
        .unwrap_or_else(|| {
            let dirty = Command::new("git")
                .args(["status", "--porcelain", "--untracked-files=no"])
                .output()
                .ok()
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            if dirty {
                "-dirty".into()
            } else {
                String::new()
            }
        });
    println!("cargo:rustc-env=DEFRA_NODE_GIT_DIRTY={}", dirty.trim());

    let release_tag = std::env::var("DEFRA_NODE_RELEASE_TAG_OVERRIDE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["describe", "--tags", "--exact-match", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .unwrap_or_default();
    println!(
        "cargo:rustc-env=DEFRA_NODE_RELEASE_TAG={}",
        release_tag.trim()
    );

    // Build timestamp (UTC). CI pins it: a per-second value changes this
    // crate's compile inputs on every build, so nothing above it hits sccache.
    let now = std::env::var("DEFRA_NODE_BUILD_TIME_OVERRIDE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            Command::new("date")
                .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DEFRA_NODE_BUILD_TIME={}", now.trim());

    // Target triple
    println!(
        "cargo:rustc-env=DEFRA_NODE_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );

    // Rustc version
    let rustc = Command::new("rustc")
        .args(["--version"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DEFRA_NODE_RUSTC={}", rustc.trim());

    // Rebuild on git branch / index changes so version metadata tracks commits.
    // Paths come from git, since in a worktree `.git` is a file and the
    // branch refs live in the common dir. A watched path that does not exist
    // counts as changed on every build, so only existing paths are emitted.
    let git_dir = git_path("--git-dir");
    let common_dir = git_path("--git-common-dir");
    let mut watched = Vec::new();
    if let Some(dir) = &git_dir {
        watched.push(dir.join("HEAD"));
        watched.push(dir.join("index"));
        if let Ok(head) = fs::read_to_string(dir.join("HEAD")) {
            if let (Some(reference), Some(common)) = (head.strip_prefix("ref: "), &common_dir) {
                watched.push(common.join(reference.trim()));
            }
        }
    }
    if let Some(common) = &common_dir {
        watched.push(common.join("packed-refs"));
    }
    for path in watched.iter().filter(|path| path.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn git_path(flag: &str) -> Option<PathBuf> {
    Command::new("git")
        .args(["rev-parse", "--path-format=absolute", flag])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|path| PathBuf::from(path.trim()))
}
