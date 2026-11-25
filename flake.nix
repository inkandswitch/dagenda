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

        toolchains = import ./nix/toolchains.nix {inherit pkgs unstable;};

        # Builds the Wasm the web pages load (javascript/dagenda-view/pkg)
        wasm-pkgs = [
          toolchains.wasm-bindgen-cli
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

        dagenda = import ./nix/dagenda.nix {inherit pkgs;};

        # The project's own tools and CI checks: each a derivation with a
        # `meta.description`, and each both a menu command and an app
        tools =
          import ./nix/ci.nix {inherit js-pkgs pkgs toolchains wasm-pkgs;}
          // import ./nix/tools.nix {inherit js-pkgs pkgs toolchains wasm-pkgs;};

        menu-modules = import ./nix/menu.nix {inherit command-utils dagenda pkgs system toolchains tools;};
      in {
        # Every menu command, and `menu`. The tools (and the packaged CLI)
        # run directly, not through the menu's wrapper, whose banner goes to
        # stdout, where `collab diff` and `dagenda render` write
        apps =
          command-utils.mkApps.${system} menu-modules
          // builtins.mapAttrs (_: drv: {
            type = "app";
            program = pkgs.lib.getExe drv;
            meta = {inherit (drv.meta) description;};
          }) (tools // {inherit dagenda;});

        packages = {
          inherit dagenda;
          default = dagenda;
        };

        devShells.default = pkgs.mkShell {
          name = "dagenda_shell";

          nativeBuildInputs =
            command-utils.commands.${system} menu-modules
            ++ [
              toolchains.rust
              toolchains.nightly-rustfmt

              pkgs.rust-analyzer
            ]
            ++ wasm-pkgs
            ++ js-pkgs
            ++ format-pkgs
            ++ cargo-installs;

          shellHook = ''
            unset SOURCE_DATE_EPOCH
            export WORKSPACE_ROOT="$(pwd)"
            export RUSTFMT="${toolchains.nightly-rustfmt}/bin/rustfmt"
            menu
          '';
        };

        formatter = pkgs.alejandra;
      }
    );
}
