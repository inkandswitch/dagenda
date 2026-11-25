# CI checks: each is a standalone `nix run .#ci-<name>` app (and a menu
# command), so hosted CI and local runs execute byte-identical commands
{
  js-pkgs,
  pkgs,
  toolchains,
  wasm-pkgs,
}: let
  env = [
    toolchains.nightly-rustfmt
    toolchains.rust
    pkgs.cargo-deny
    pkgs.stdenv.cc # the linker, so checks also pass outside the dev shell
    pkgs.typos
  ];

  mkCheck = name: {
    description,
    extra ? [],
    text,
  }:
    pkgs.writeShellApplication {
      name = "dagenda-${name}";
      runtimeInputs = env ++ extra;
      text = ''
        export RUSTFMT="${toolchains.nightly-rustfmt}/bin/rustfmt"
        set -x
        ${text}
      '';
      meta = {inherit description;};
    };
in
  builtins.mapAttrs mkCheck {
    ci-bolero = {
      description = "CI: property tests (BOLERO_RANDOM_ITERATIONS, default 10000)";
      text = ''
        export BOLERO_RANDOM_ITERATIONS="''${BOLERO_RANDOM_ITERATIONS:-10000}"
        cargo test --profile bolero -p dagenda_core --tests
      '';
    };

    ci-clippy = {
      description = "CI: cargo check and Clippy, native and Wasm";
      text = ''
        cargo check --workspace --all-targets
        cargo clippy --workspace --all-targets --all-features -- -D warnings
        # What a plain dependency builds: default features, library only
        cargo clippy -p dagenda_core -- -D warnings
        cargo clippy -p dagenda_wasm --target wasm32-unknown-unknown --all-targets -- -D warnings
      '';
    };

    ci-deny = {
      description = "CI: cargo deny (licences, advisories, sources)";
      text = "cargo deny check";
    };

    ci-doc = {
      description = "CI: doc tests, and docs without warnings";
      text = ''
        cargo test --doc --workspace --all-features
        RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
      '';
    };

    # Sync goes through the public Subduction server (DAGENDA_SYNC for another)
    ci-editor = {
      description = "CI: the preview and the collaborative editor, built, in headless Chromium";
      extra = wasm-pkgs ++ js-pkgs ++ [pkgs.chromium];
      text = ''
        wasm-pack build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm
        pnpm install --frozen-lockfile
        # Through node, not the .bin shim: its #!/bin/sh is not everywhere
        (cd javascript/editor && node node_modules/vite/bin/vite.js build)
        CHROMIUM=chromium node --test --test-concurrency=1 javascript/editor/browser-test/*.test.mjs
        node --test javascript/editor/cli/*.test.mjs
      '';
    };

    ci-fmt = {
      description = "CI: rustfmt and alejandra, checking only";
      extra = [pkgs.alejandra];
      text = ''
        cargo fmt --all --check
        alejandra --check flake.nix nix
      '';
    };

    # Every script is syntax-checked, built or not
    ci-js = {
      description = "CI: syntax-check the scripts, build the Wasm, run the JS tests";
      extra = wasm-pkgs;
      text = ''
        for file in javascript/*/*.js javascript/*/*.mjs javascript/editor/src/*.js; do
          node --check "$file"
        done
        wasm-pack build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm
        node --test javascript/*/*.test.mjs javascript/editor/src/*.test.mjs
      '';
    };

    ci-mutants = {
      description = "CI: mutation testing (a diff file for just its lines, or --shard n/m)";
      extra = [pkgs.cargo-mutants];
      text = builtins.readFile ./scripts/ci-mutants.sh;
    };

    # A target without `std` is the only real proof of no_std
    ci-no-std = {
      description = "CI: build dagenda_core for a target without std";
      text = "cargo check -p dagenda_core --target thumbv6m-none-eabi";
    };

    # The Wasm bindings only run in Wasm; ci-js tests them from JavaScript
    ci-test = {
      description = "CI: the Rust tests";
      text = "cargo test --workspace --exclude dagenda_wasm";
    };

    ci-typos = {
      description = "CI: spell-check with typos";
      text = "typos";
    };
  }
  // {
    # Re-resolve as a library consumer would, preferring MSRV-compatible
    # versions, then restore the committed lockfile. Its own toolchain, so
    # not a mkCheck
    ci-msrv = pkgs.writeShellApplication {
      name = "dagenda-ci-msrv";
      runtimeInputs = [toolchains.msrv pkgs.coreutils pkgs.stdenv.cc];
      text = ''
        export CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback
        committed_lock="$(mktemp)"
        cp Cargo.lock "$committed_lock"
        trap 'mv "$committed_lock" Cargo.lock' EXIT
        set -x
        cargo generate-lockfile
        cargo check --workspace --all-targets --all-features
      '';
      meta.description = "CI: check the workspace with the MSRV toolchain";
    };
  }
