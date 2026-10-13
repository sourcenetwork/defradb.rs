{
  description = "DefraDB.rs development environment";

  # `just setup` is the portable onboarding path: it installs every tool into
  # .tooling/ without sudo or a package manager. It cannot work on NixOS,
  # because the archives it downloads are prebuilt and dynamically linked
  # against an FHS loader that does not exist there. This flake is the NixOS
  # equivalent, not a replacement -- both are kept working.

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      fenix,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fx = fenix.packages.${system};

        # nixpkgs rustc trails the workspace: Cargo.toml sets
        # rust-version = "1.98" and nixos-unstable currently carries 1.95, so
        # the toolchain comes from fenix. `stable` matches rust-toolchain.toml's
        # `channel = "stable"`; flake.lock is what actually pins the version.
        #
        # Components are listed individually rather than taking
        # `stable.toolchain`, which collides with the extra wasm rust-std.
        rust = fx.combine [
          fx.stable.cargo
          fx.stable.rustc
          fx.stable.rust-std
          fx.stable.rust-src
          fx.stable.clippy
          fx.stable.rustfmt
          # ci.yml's wasm-check job and `just lint-wasm` / `just test-wasm`.
          fx.targets.wasm32-unknown-unknown.stable.rust-std
        ];
      in
      {
        # `nix fmt` on this flake. nixfmt takes files, not trees, and `nix fmt`
        # hands its formatter whole paths, so this filters to *.nix rather than
        # feeding it the repo and watching it fail on the first .rs file.
        #
        # There is deliberately no `packages` output:
        # crates/defra-version/build.rs shells out to git, which a sandboxed
        # build cannot do; Cargo.lock carries 52 git dependencies, each needing
        # a hand-maintained cargoLock.outputHashes entry; and [profile.release]
        # is fat LTO at codegen-units = 1, which measured 6-8 minutes for one
        # binary. `just build` and this shell cover the actual workflows.
        formatter = pkgs.writeShellApplication {
          name = "nixfmt-nix-files";
          runtimeInputs = [
            pkgs.nixfmt
            pkgs.findutils
          ];
          text = ''
            [ "$#" -gt 0 ] || set -- .
            find "$@" -type f -name '*.nix' -print0 | xargs -0 -r nixfmt
          '';
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            rust

            pkgs.pkg-config
            pkgs.cmake # aws-lc-sys
            pkgs.perl # openssl-sys, aws-lc-sys
            pkgs.protobuf # crates/orbis/build.rs, via tonic-prost-build
            pkgs.rust-cbindgen # crates/ffi C header
            pkgs.just
            pkgs.git # .cargo/config.toml sets git-fetch-with-cli
            pkgs.jq
          ];

          buildInputs = [
            pkgs.openssl # openssl-sys
            pkgs.dbus # libdbus-sys, via keyring's secret-service backend
            pkgs.zlib # libz-sys
          ];

          # bindgen does not locate libclang on its own here.
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          OPENSSL_NO_VENDOR = "1";

          # librocksdb-sys 0.16 vendors RocksDB 8.10, whose headers use
          # uint64_t without including <cstdint> and relied on libstdc++
          # pulling it in transitively. Current GCC does not, so the bundled
          # build fails with "'uint64_t' has not been declared". RocksDB
          # reaches the graph through the vera.rs dependency, so this is needed
          # for `cargo clippy --all` even though no first-party crate uses it.
          # cc-rs passes CXXFLAGS through to every C++ translation unit.
          CXXFLAGS = "-include cstdint";

          # Test binaries link libdbus-1.so.3 without recording an rpath for
          # it, so the library path is needed at run time and not only at link
          # time. `cargo test -p cli` fails with a loader error without this.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.openssl
            pkgs.dbus
            pkgs.zlib
          ];

          shellHook = ''
            # .cargo/config.toml carries the wasm32 rustflags table, and a
            # RUSTFLAGS environment variable REPLACES that table wholesale
            # rather than adding to it, so an inherited value silently breaks
            # every wasm build. The table's own comment says as much.
            unset RUSTFLAGS

            # Deliberately absent, because each is version-matched elsewhere and
            # a nixpkgs copy would drift: wasm-bindgen-cli (pinned to Cargo.lock
            # by `just setup-cargo-tools`), wasm-pack, Go (justfile pins
            # go_version), the Temurin JDK and TLC jar (checksum-pinned by
            # `just setup-tla`), and elan/Lean. Run the matching `just setup-*`
            # recipe for those; `just doctor` reports what is missing.
            echo "defradb.rs: $(rustc --version)"
          '';
        };
      }
    );
}
