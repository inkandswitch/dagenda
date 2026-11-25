# The toolchains, pinned by the manifests that own them. rust-overlay keys
# toolchains by full version (`1.91.0`), while `rust-version` may omit the
# patch (`1.91`)
{
  pkgs,
  unstable,
}: let
  workspace = builtins.fromTOML (builtins.readFile ../Cargo.toml);
  channel = (builtins.fromTOML (builtins.readFile ../rust-toolchain.toml)).toolchain.channel;
  msrv = pkgs.lib.versions.pad 3 workspace.workspace.package.rust-version;

  # Nightly rustfmt for unstable options (imports_granularity, etc.)
  nightly-rustfmt-unwrapped = pkgs.rust-bin.nightly.latest.minimal.override {
    extensions = ["rustfmt"];
  };
in {
  rust = pkgs.rust-bin.stable.${channel}.default.override {
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

  msrv = pkgs.rust-bin.stable.${msrv}.minimal;

  # rustfmt links librustc_driver, so wrap it with the library path
  nightly-rustfmt = pkgs.writeShellScriptBin "rustfmt" ''
    export DYLD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    export LD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    exec "${nightly-rustfmt-unwrapped}/bin/rustfmt" "$@"
  '';

  # Must match the exact `wasm-bindgen` pin in Cargo.toml
  wasm-bindgen-cli = unstable.wasm-bindgen-cli;
}
