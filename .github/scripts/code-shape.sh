#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
library="$root/tools/code-shape"
nightly="$(awk -F '"' '/^channel =/ { print $2; exit }' "$library/rust-toolchain.toml")"
dylint_version="$(awk -F '"' '/^dylint_linting =/ { sub(/^=/, "", $2); print $2; exit }' "$library/Cargo.toml")"
tool_root="${CARGO_INSTALL_ROOT:-$root/.tooling}"
if [[ "$tool_root" != /* ]]; then
    tool_root="$root/$tool_root"
fi
export PATH="$tool_root/bin:$PATH"
cd "$root"
mode="${1:-preview}"

tools_installed() {
    local installed
    [[ -x "$tool_root/bin/cargo-dylint" && -x "$tool_root/bin/dylint-link" ]] || return 1
    installed="$(cargo install --list --root "$tool_root")" || return 1
    grep -Fxq "cargo-dylint v$dylint_version:" <<<"$installed" &&
        grep -Fxq "dylint-link v$dylint_version:" <<<"$installed"
}

if [[ "$mode" == setup ]]; then
    rustup toolchain install "$nightly" --profile minimal \
        --component rustc-dev,llvm-tools-preview,rust-src,rustfmt,clippy \
        --target wasm32-unknown-unknown
    if ! tools_installed; then
        install_target="$(mktemp -d "${TMPDIR:-/tmp}/defradb-dylint-install.XXXXXXXX")"
        trap 'cargo clean --manifest-path "$root/Cargo.toml" --target-dir "$install_target" >&2' EXIT
        cargo install cargo-dylint dylint-link --version "$dylint_version" --locked --force \
            --root "$tool_root" --target-dir "$install_target"
    fi
    exit 0
fi

if [[ -n "${CODE_SHAPE_TARGET_DIR:-}" ]]; then
    mkdir -p "$CODE_SHAPE_TARGET_DIR"
    CARGO_TARGET_DIR="$(cd "$CODE_SHAPE_TARGET_DIR" && pwd)"
else
    base_target="$(cargo metadata --format-version 1 --no-deps --locked | jq -er '.target_directory')"
    CARGO_TARGET_DIR="$base_target/code-shape"
fi
export CARGO_TARGET_DIR
export DYLINT_DRIVER_PATH="${DYLINT_DRIVER_PATH:-$CARGO_TARGET_DIR/drivers}"
mkdir -p "$DYLINT_DRIVER_PATH"
enabled_rules=(
    inline_generic_bounds
    anonymous_callback
    callback_lifetime_name
    manual_error_impl
    untyped_error
    erased_error_signature
    test_result
    crate_result
)
export DYLINT_RUSTFLAGS="-A defra_code_shape -D unfulfilled_lint_expectations"
for rule in "${enabled_rules[@]}"; do
    DYLINT_RUSTFLAGS+=" -D $rule"
done
# Dylint owns the compiler wrapper and tracks changes to its lint library.
unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
export CARGO_INCREMENTAL=0

if ! tools_installed; then
    echo "Run .github/scripts/code-shape.sh setup to install Dylint $dylint_version." >&2
    exit 1
fi

lint() {
    cargo dylint --all --fail-on-no-libraries "$@"
}

check_dependencies() {
    if [[ " ${enabled_rules[*]} " != *" untyped_error "* ]]; then
        return
    fi
    local manifest="$1"
    local violations
    violations="$(cargo metadata --manifest-path "$manifest" --format-version 1 --no-deps --locked |
        jq -r '.packages[] | select(.source == null) | .manifest_path as $manifest |
            .dependencies[] | select(.name == "anyhow" or .name == "eyre" or .name == "color-eyre") |
            "\($manifest): remove direct dependency \(.name)"')"
    if [[ -n "$violations" ]]; then
        echo "$violations" >&2
        return 1
    fi
}

check_ui() (
    cd "$library"
    unset DYLINT_RUSTFLAGS
    cargo +"$nightly" fmt -- --check
    cargo +"$nightly" test --locked
)

case "$mode" in
    preview)
        check_ui
        ;;
    native)
        check_dependencies "$root/Cargo.toml"
        check_ui
        lint --workspace -- --all-targets --locked
        lint --workspace -- --all-targets --locked \
            --features cli/iroh,cli/vera,cli/model-trace,cli/embedded-vera,cli/otel
        lint -p defra-node -- --all-targets --locked --no-default-features --features native
        lint -p defra-node -- --all-targets --locked --no-default-features --features native,p2p
        lint -p ffi -- --all-targets --locked --no-default-features --features native
        lint -p ffi -- --all-targets --locked --no-default-features --features native,iroh
        lint -p ffi -- --all-targets --locked --no-default-features --features native,libp2p
        ;;
    wasm)
        check_dependencies "$root/Cargo.toml"
        lint -p defra-wasm -- --all-targets --locked --target wasm32-unknown-unknown
        lint -p defra-wasm -- --all-targets --locked --target wasm32-unknown-unknown \
            --no-default-features
        # These constants are required to compile relay tests; no service runs during linting.
        DEFRA_E2E_API="${DEFRA_E2E_API:-http://127.0.0.1:9181}" \
        DEFRA_E2E_RELAY="${DEFRA_E2E_RELAY:-http://127.0.0.1:3340}" \
        lint -p defra-wasm -- --all-targets --locked --target wasm32-unknown-unknown \
            --features relay-e2e
        ;;
    tools)
        check_dependencies "$root/Cargo.toml"
        for manifest in \
            tools/code-shape/Cargo.toml \
            tools/vector-bench/Cargo.toml \
            tools/integration-test/test-lenses/set_default/Cargo.toml; do
            check_dependencies "$root/$manifest"
            cargo fmt --manifest-path "$root/$manifest" -- --check
            lint --path "$library" --manifest-path "$root/$manifest" -- --all-targets --locked
        done
        lint --path "$library" \
            --manifest-path "$root/tools/integration-test/test-lenses/set_default/Cargo.toml" \
            -- --all-targets --locked --target wasm32-unknown-unknown
        ;;
    *)
        echo "Usage: $0 [setup|preview|native|wasm|tools]" >&2
        exit 2
        ;;
esac
