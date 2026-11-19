{
  description = "dagenda: roadmaps as dependency graphs, with public milestones and private task detail";

  inputs = {
    command-utils = {
      url = "git+https://tangled.org/expede.wtf/nix-command-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixos-26.05";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    command-utils,
    flake-utils,
    nixpkgs,
    rust-overlay,
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];

        pkgs = import nixpkgs {inherit system overlays;};

        # Single sources of truth, read from the manifests that own them.
        # rust-overlay keys toolchains by full version (`1.91.0`), while
        # `rust-version` may omit the patch (`1.91`)
        workspaceManifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        rustVersion = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
        msrv = pkgs.lib.versions.pad 3 workspaceManifest.workspace.package.rust-version;

        rust-toolchain = pkgs.rust-bin.stable.${rustVersion}.default.override {
          extensions = [
            "cargo"
            "clippy"
            "llvm-tools-preview"
            "rust-src"
            "rust-std"
          ];

          targets = [
            "aarch64-apple-darwin"
            "x86_64-apple-darwin"

            "aarch64-unknown-linux-musl"
            "x86_64-unknown-linux-musl"

            "thumbv6m-none-eabi" # proves dagenda_core is really no_std
            "wasm32-unknown-unknown"
          ];
        };

        msrv-toolchain = pkgs.rust-bin.stable.${msrv}.minimal;

        # Nightly rustfmt for unstable options (imports_granularity, etc.).
        # rustfmt links librustc_driver, so wrap it with the library path.
        nightly-rustfmt-unwrapped = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = ["rustfmt"];
        };

        nightly-rustfmt = pkgs.writeShellScriptBin "rustfmt" ''
          export DYLD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
          export LD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          exec "${nightly-rustfmt-unwrapped}/bin/rustfmt" "$@"
        '';

        format-pkgs = with pkgs; [
          alejandra
          taplo
        ];

        cargo-installs = with pkgs; [
          cargo-deny
          cargo-expand
          cargo-mutants
          cargo-nextest
          cargo-outdated
          cargo-sort
          cargo-udeps
          cargo-watch
          typos
        ];

        # The `dagenda` CLI: `nix run github:inkandswitch/dagenda -- check plan.dagenda`
        dagenda = pkgs.rustPlatform.buildRustPackage {
          pname = "dagenda";
          inherit ((builtins.fromTOML (builtins.readFile ./rust/dagenda_cli/Cargo.toml)).package) version;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [./Cargo.lock ./Cargo.toml ./README.md ./rust];
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["-p" "dagenda_cli"];
          cargoTestFlags = ["-p" "dagenda_cli"];
          meta = {
            description = "Check roadmaps written in the dagenda notation";
            license = with pkgs.lib.licenses; [asl20 mit];
            mainProgram = "dagenda";
          };
        };

        rust = command-utils.rust.${system};

        command_menu = command-utils.commands.${system} [
          (rust.build {cargo = pkgs.cargo;})
          (rust.test {
            cargo = pkgs.cargo;
            cargo-watch = pkgs.cargo-watch;
          })
          (rust.lint {cargo = pkgs.cargo;})
          (rust.fmt {cargo = pkgs.cargo;})
          (rust.doc {cargo = pkgs.cargo;})
          (rust.watch {cargo-watch = pkgs.cargo-watch;})
        ];

        # CI checks: each is a standalone `nix run .#ci-<name>` app so hosted
        # CI and local runs execute byte-identical commands.
        ci-env = [
          nightly-rustfmt
          pkgs.cargo-deny
          pkgs.stdenv.cc # the linker, so checks also pass outside the dev shell
          pkgs.typos
          rust-toolchain
        ];

        mkCheck = name: text: mkCheckWith [] name text;
        mkCheckWith = extra: name: text:
          pkgs.writeShellApplication {
            name = "dagenda-${name}";
            runtimeInputs = ci-env ++ extra;
            text = ''
              export RUSTFMT="${nightly-rustfmt}/bin/rustfmt"
              set -x
              ${text}
            '';
          };

        ci-checks = {
          ci-fmt = mkCheckWith [pkgs.alejandra] "ci-fmt" ''
            cargo fmt --all --check
            alejandra --check flake.nix
          '';

          ci-clippy = mkCheck "ci-clippy" ''
            cargo check --workspace --all-targets
            cargo clippy --workspace --all-targets --all-features -- -D warnings
            # What a plain dependency builds: default features, library only
            cargo clippy -p dagenda_core -- -D warnings
          '';

          ci-test = mkCheck "ci-test" ''
            cargo test --workspace
          '';

          ci-doc = mkCheck "ci-doc" ''
            cargo test --doc --workspace --all-features
            RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
          '';

          # Mutation testing: with a diff, only the lines it touches, failing
          # if any mutant survives (timeouts hung the tests, so they count as
          # caught); without one, everything, or one shard of it
          # (`--shard 0/4`), reporting without failing. Results land in
          # target/mutants/mutants.out
          ci-mutants = mkCheckWith [pkgs.cargo-mutants] "ci-mutants" ''
            out=target/mutants/mutants.out
            rm -rf "$out"
            status=0
            if [ -f "''${1:-}" ]; then
              cargo mutants --in-diff "$1" --no-shuffle -j 2 --output target/mutants || status=$?
            else
              cargo mutants --no-shuffle -j 2 --output target/mutants "$@" || status=$?
            fi
            # 0 all caught, 2 some missed, 3 some timed out (which counts as
            # caught, and hides misses in the exit code); anything else is a
            # usage error or a failing baseline
            case "$status" in 0 | 2 | 3) ;; *) exit "$status" ;; esac
            if [ -s "$out/missed.txt" ]; then
              echo "Survived (each needs a test, or an equivalence note in .cargo/mutants.toml):" >&2
              cat "$out/missed.txt" >&2
              [ -f "''${1:-}" ] && exit 1
            fi
            true
          '';

          # A target without `std` is the only real proof of no_std
          ci-no-std = mkCheck "ci-no-std" ''
            cargo check -p dagenda_core --target thumbv6m-none-eabi
          '';

          ci-bolero = mkCheck "ci-bolero" ''
            export BOLERO_RANDOM_ITERATIONS="''${BOLERO_RANDOM_ITERATIONS:-10000}"
            cargo test --profile bolero -p dagenda_core --tests
          '';

          ci-deny = mkCheck "ci-deny" ''
            cargo deny check
          '';

          ci-typos = mkCheck "ci-typos" ''
            typos
          '';

          # Re-resolve as a library consumer would, preferring MSRV-compatible
          # versions, then restore the committed lockfile.
          ci-msrv = pkgs.writeShellApplication {
            name = "dagenda-ci-msrv";
            runtimeInputs = [msrv-toolchain pkgs.coreutils pkgs.stdenv.cc];
            text = ''
              export CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback
              committed_lock="$(mktemp)"
              cp Cargo.lock "$committed_lock"
              trap 'mv "$committed_lock" Cargo.lock' EXIT
              set -x
              cargo generate-lockfile
              cargo check --workspace --all-targets --all-features
            '';
          };
        };
      in {
        apps =
          builtins.mapAttrs (_: drv: {
            type = "app";
            program = pkgs.lib.getExe drv;
          })
          (ci-checks // {inherit dagenda;});

        packages = {
          inherit dagenda;
          default = dagenda;
        };

        devShells.default = pkgs.mkShell {
          name = "dagenda_shell";

          nativeBuildInputs =
            command_menu
            ++ [
              rust-toolchain
              nightly-rustfmt

              pkgs.rust-analyzer
            ]
            ++ format-pkgs
            ++ cargo-installs;

          shellHook = ''
            unset SOURCE_DATE_EPOCH
            export WORKSPACE_ROOT="$(pwd)"
            export RUSTFMT="${nightly-rustfmt}/bin/rustfmt"
            menu
          '';
        };

        formatter = pkgs.alejandra;
      }
    );
}
