use std::fs;
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
    // A worktree's `.git` is a file, so resolve the paths git itself uses.
    let git_output = |argument: &str| {
        Command::new("git")
            .args(["rev-parse", argument])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|path| path.trim().to_owned())
            .filter(|path| !path.is_empty())
    };
    let watched = [
        git_output("--git-dir").map(|dir| format!("{dir}/HEAD")),
        git_output("--git-dir").map(|dir| format!("{dir}/index")),
        git_output("--git-common-dir").map(|dir| format!("{dir}/packed-refs")),
    ];
    for path in watched.into_iter().flatten() {
        if fs::metadata(&path).is_ok() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
