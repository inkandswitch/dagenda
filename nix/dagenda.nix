# The `dagenda` CLI: `nix run github:inkandswitch/dagenda -- check plan.dagenda`
{pkgs}:
pkgs.rustPlatform.buildRustPackage {
  pname = "dagenda";
  inherit ((builtins.fromTOML (builtins.readFile ../rust/dagenda_cli/Cargo.toml)).package) version;
  src = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [../Cargo.lock ../Cargo.toml ../README.md ../rust];
  };
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = ["-p" "dagenda_cli"];
  cargoTestFlags = ["-p" "dagenda_cli"];
  meta = {
    description = "Check roadmaps written in the dagenda notation";
    license = with pkgs.lib.licenses; [asl20 mit];
    mainProgram = "dagenda";
  };
}
