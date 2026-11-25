# The project's own tools, each a menu command and a `nix run` app. Each runs
# from the repo root, or anywhere inside `nix develop` (via WORKSPACE_ROOT);
# the scripts are in ./scripts
{
  js-pkgs,
  pkgs,
  toolchains,
  wasm-pkgs,
}: let
  tool = name: description: runtimeInputs:
    pkgs.writeShellApplication {
      name = "dagenda-${name}";
      inherit runtimeInputs;
      text = builtins.readFile ./scripts/${name}.sh;
      meta = {inherit description;};
    };

  rust = [toolchains.rust pkgs.stdenv.cc];
in {
  collab =
    tool "collab"
    "Collaborative editor at http://localhost:5173 (PORT to change); with pull, push, diff, …: a shared plan as files"
    (rust ++ wasm-pkgs ++ js-pkgs);

  preview =
    tool "preview"
    "Hot-reloading preview at http://127.0.0.1:8080 (PORT to change)"
    (rust ++ wasm-pkgs);

  site =
    tool "site"
    "Build the preview and the editor as a static site in _site/ (or the directory given)"
    (rust ++ [pkgs.coreutils] ++ wasm-pkgs ++ js-pkgs);
}
