#!/usr/bin/env bash
# Asserts the flake devShell carries what building this workspace needs. Run
# inside the shell: `nix develop --command bash .github/scripts/assert-nix-shell.sh`.
#
# Each assertion stands for a failure that was hit for real while writing the
# flake, so a regression here is one someone would otherwise discover halfway
# through a build.
set -euo pipefail

fail() {
  echo "::error::$1"
  exit 1
}

for tool in rustc cargo clippy-driver rustfmt protoc cbindgen just jq cmake pkg-config perl git; do
  command -v "$tool" >/dev/null || fail "$tool is missing from the devShell"
done

# The reason the toolchain comes from fenix rather than nixpkgs: nixpkgs trails
# the workspace floor. If this ever passes with the nixpkgs rustc, the comment
# in flake.nix is wrong and the fenix input is dead weight.
want="$(awk -F'"' '/^rust-version = /{print $2; exit}' Cargo.toml)"
have="$(rustc --version | cut -d' ' -f2)"
[ -n "$want" ] || fail "could not read rust-version from Cargo.toml"
printf '%s\n%s\n' "$want" "$have" | sort -V -C ||
  fail "devShell rustc $have is older than the workspace rust-version $want"
echo "rustc $have satisfies rust-version $want"

sysroot="$(rustc --print sysroot)"

# fenix's `stable.toolchain` collides with the extra wasm rust-std, which is
# why flake.nix lists components individually. This is what that buys.
[ -d "$sysroot/lib/rustlib/wasm32-unknown-unknown" ] ||
  fail "wasm32-unknown-unknown rust-std is missing; ci.yml's wasm-check job needs it"

[ -d "$sysroot/lib/rustlib/src/rust" ] || fail "rust-src is missing from the devShell"

# .cargo/config.toml holds the wasm32 rustflags table, and a RUSTFLAGS
# environment variable replaces that table wholesale rather than adding to it.
[ -z "${RUSTFLAGS:-}" ] ||
  fail "RUSTFLAGS is set ($RUSTFLAGS); it silently voids the wasm32 rustflags table"

# Test binaries link libdbus-1.so.3 without recording an rpath for it.
case "${LD_LIBRARY_PATH:-}" in
  *dbus*) ;;
  *) fail "LD_LIBRARY_PATH does not carry dbus; 'cargo test -p cli' dies in the loader" ;;
esac

pkg-config --exists dbus-1 || fail "pkg-config cannot find dbus-1"
pkg-config --exists openssl || fail "pkg-config cannot find openssl"

echo "devShell assertions passed"
