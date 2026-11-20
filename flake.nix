{
  description = "dagenda: roadmaps as dependency graphs, with public milestones and private task detail";

  inputs = {
    command-utils = {
      url = "git+https://tangled.org/expede.wtf/nix-command-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
    nixos-unstable.url = "nixpkgs/nixos-unstable-small";
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
    nixos-unstable,
    nixpkgs,
    rust-overlay,
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];

        pkgs = import nixpkgs {inherit system overlays;};
        unstable = import nixos-unstable {inherit system overlays;};

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

        # Must match the exact `wasm-bindgen` pin in Cargo.toml
        wasm-bindgen-cli = unstable.wasm-bindgen-cli;

        # Builds the Wasm the web pages load (javascript/dagenda-view/pkg)
        wasm-pkgs = [
          wasm-bindgen-cli
          pkgs.binaryen
          pkgs.nodejs
          pkgs.wasm-pack
        ];

        # The collaborative editor's JS dependencies (javascript/editor)
        js-pkgs = [pkgs.nodejs pkgs.pnpm];

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

        # Hot-reloading preview: rebuilds the Wasm when Rust sources change and
        # reloads the page (see javascript/preview/serve.mjs). Run from the
        # repo root, or anywhere inside `nix develop` (via WORKSPACE_ROOT).
        preview = pkgs.writeShellApplication {
          name = "dagenda-preview";
          runtimeInputs = [rust-toolchain pkgs.stdenv.cc] ++ wasm-pkgs;
          text = ''
            root="''${WORKSPACE_ROOT:-$PWD}"
            if [ ! -f "$root/javascript/preview/serve.mjs" ]; then
              echo "run this from the dagenda repo root" >&2
              exit 1
            fi

            exec node "$root/javascript/preview/serve.mjs"
          '';
        };

        # The collaborative editor's dev server (javascript/editor): installs
        # its dependencies if needed, builds the component's Wasm, then serves
        # it with Vite at http://localhost:5173 (PORT to change). The
        # preview's Collaborate button reaches it through `nix run .#preview`
        # (EDITOR_PORT there). Run from the repo root, or anywhere inside
        # `nix develop`.
        editor = pkgs.writeShellApplication {
          name = "dagenda-editor";
          runtimeInputs = [rust-toolchain pkgs.stdenv.cc] ++ wasm-pkgs ++ js-pkgs;
          text = ''
            root="''${WORKSPACE_ROOT:-$PWD}"
            if [ ! -f "$root/javascript/editor/index.html" ]; then
              echo "run this from the dagenda repo root" >&2
              exit 1
            fi
            cd "$root"
            [ -d javascript/editor/node_modules ] || pnpm install --frozen-lockfile
            wasm-pack build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm
            cd javascript/editor
            # Through node, not the .bin shim: its #!/bin/sh is not everywhere
            exec node node_modules/vite/bin/vite.js --port "''${PORT:-5173}" --strictPort
          '';
        };

        # The preview as a static site (for GitHub Pages): a release Wasm
        # build, the component, and the preview page, with `/` sending you to
        # the preview, plus the collaborative editor at `collab/`. Writes
        # `_site/` (or the directory given).
        site = pkgs.writeShellApplication {
          name = "dagenda-site";
          runtimeInputs = [rust-toolchain pkgs.coreutils pkgs.stdenv.cc] ++ wasm-pkgs ++ js-pkgs;
          text = ''
            root="''${WORKSPACE_ROOT:-$PWD}"
            out="$(realpath -m "''${1:-_site}")"
            case "$out" in
              / | "$root" | "$HOME") echo "refusing to replace $out" >&2; exit 1 ;;
            esac
            set -x
            rm -rf "$out"
            mkdir -p "$out/preview" "$out/dagenda-view"
            wasm-pack build --release --target web --no-pack \
              --out-dir "$out/dagenda-view/pkg" "$root/rust/dagenda_wasm"
            rm -f "$out/dagenda-view/pkg/.gitignore"
            cp "$root/javascript/dagenda-view/dagenda-view.js" "$root/javascript/dagenda-view/editing.js" "$out/dagenda-view/"
            for file in index.html examples.js highlight.js indent.js main.js shell.js style.css; do
              cp "$root/javascript/preview/$file" "$out/preview/"
            done
            # The editor bundles the component, so it takes the same release
            # build. A relative base serves it from any path (/dagenda/collab/)
            rm -rf "$root/javascript/dagenda-view/pkg"
            cp -r "$out/dagenda-view/pkg" "$root/javascript/dagenda-view/"
            (cd "$root" && pnpm install --frozen-lockfile)
            (cd "$root/javascript/editor" && node node_modules/vite/bin/vite.js build --base ./ --outDir "$out/collab" --emptyOutDir)
            cat > "$out/index.html" <<'HTML'
            <!doctype html>
            <meta charset="utf-8">
            <title>dagenda</title>
            <meta http-equiv="refresh" content="0; url=preview/">
            <a href="preview/">Open the dagenda preview</a>
            HTML
            touch "$out/.nojekyll"
          '';
        };

        rust = command-utils.rust.${system};
        wasm = command-utils.wasm.${system};
        cmd = command-utils.cmd.${system};

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

          (wasm.build {wasm-pack = pkgs.wasm-pack;})
          (wasm.release {
            wasm-pack = pkgs.wasm-pack;
            gzip = pkgs.gzip;
          })

          {
            commands.preview = cmd "Hot-reloading preview at http://127.0.0.1:8080 (PORT to change)" ''
              ${pkgs.lib.getExe preview}
            '';
            packages = [];
          }

          {
            commands.editor = cmd "Collaborative editor at http://localhost:5173 (PORT to change)" ''
              ${pkgs.lib.getExe editor}
            '';
            packages = [];
          }
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
            cargo clippy -p dagenda_wasm --target wasm32-unknown-unknown --all-targets -- -D warnings
          '';

          # The Wasm bindings only run in Wasm; ci-js tests them from JavaScript
          ci-test = mkCheck "ci-test" ''
            cargo test --workspace --exclude dagenda_wasm
          '';

          # Syntax-check every script (there's no JS build step), build the
          # Wasm the tests load, then run the JS tests
          ci-js = mkCheckWith wasm-pkgs "ci-js" ''
            for file in javascript/*/*.js javascript/*/*.mjs; do
              node --check "$file"
            done
            wasm-pack build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm
            node --test javascript/*/*.test.mjs
          '';

          # The collaborative editor, built, in headless Chromium. Sync goes
          # through the public Subduction server (DAGENDA_SYNC for another)
          ci-editor = mkCheckWith (wasm-pkgs ++ js-pkgs ++ [pkgs.chromium]) "ci-editor" ''
            wasm-pack build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm
            pnpm install --frozen-lockfile
            # Through node, not the .bin shim: its #!/bin/sh is not everywhere
            (cd javascript/editor && node node_modules/vite/bin/vite.js build)
            CHROMIUM=chromium node --test --test-concurrency=1 javascript/editor/browser-test/*.test.mjs
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
          (ci-checks // {inherit dagenda editor preview site;});

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
            ++ wasm-pkgs
            ++ js-pkgs
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
