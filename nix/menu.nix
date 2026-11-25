# The dev shell's menu. Its commands are also apps (`nix run .#rust:test`),
# which run outside the dev shell, calling each tool by its store path. So
# each tool here brings what it runs by name: cargo the pinned toolchain
# (rustc, clippy, the nightly rustfmt) and a linker; cargo-watch that cargo,
# sh, and the toolchain (its default command looks past cargo); wasm-pack
# that cargo, rustc (it asks for its version), and the pinned wasm-bindgen
{
  command-utils,
  dagenda,
  pkgs,
  system,
  toolchains,
  tools,
}: let
  inherit (pkgs.lib) getExe;

  asModule = command-utils.asModule.${system};
  cmd = command-utils.cmd.${system};
  rust = command-utils.rust.${system};
  wasm = command-utils.wasm.${system};

  cargo = pkgs.writeShellApplication {
    name = "cargo";
    runtimeInputs = [toolchains.rust pkgs.stdenv.cc];
    text = ''
      export RUSTFMT="''${RUSTFMT:-${toolchains.nightly-rustfmt}/bin/rustfmt}"
      exec ${toolchains.rust}/bin/cargo "$@"
    '';
  };

  cargo-watch = pkgs.writeShellApplication {
    name = "cargo-watch";
    runtimeInputs = [cargo pkgs.bash toolchains.rust];
    text = ''exec ${getExe pkgs.cargo-watch} "$@"'';
  };

  wasm-pack = pkgs.writeShellApplication {
    name = "wasm-pack";
    runtimeInputs = [cargo toolchains.rust toolchains.wasm-bindgen-cli pkgs.binaryen];
    text = ''exec ${getExe pkgs.wasm-pack} "$@"'';
  };

  # The Wasm commands build the crate the web pages load
  path = "rust/dagenda_wasm";
in [
  (rust.build {inherit cargo;})
  (rust.test {inherit cargo cargo-watch;})
  (rust.lint {inherit cargo;})
  (rust.fmt {inherit cargo;})
  (rust.doc {inherit cargo;})
  (rust.watch {inherit cargo-watch;})

  (wasm.build {inherit path wasm-pack;})
  (wasm.release {
    inherit path wasm-pack;
    gzip = pkgs.gzip;
  })

  (asModule (builtins.mapAttrs (_: drv: cmd drv.meta.description ''${getExe drv} "$@"'') tools))

  # The CLI from this checkout, built as you go: the `dagenda` app's package
  # would rebuild on entering the shell after every change
  (asModule {
    dagenda = cmd dagenda.meta.description ''
      exec ${getExe cargo} run --quiet --manifest-path "''${WORKSPACE_ROOT:-$PWD}/Cargo.toml" -p dagenda_cli -- "$@"
    '';
  })
]
