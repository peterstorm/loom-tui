{
  description = "loom-tui: Rust TUI dashboard for Claude Code multi-agent orchestration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs @ { self, nixpkgs, flake-parts, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];

      perSystem = { config, self', inputs', pkgs, system, ... }:
        let
          craneLib = crane.mkLib pkgs;

          src = craneLib.cleanCargoSource ./.;

          commonArgs = {
            inherit src;
            strictDeps = true;
            buildInputs = [ ];
            nativeBuildInputs = [ ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          loom-tui = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
        in
        {
          packages = {
            default = loom-tui;
            inherit loom-tui;
          };

          devShells.default = craneLib.devShell {
            checks = self'.checks;
            packages = with pkgs; [
              rustc
              cargo
              rust-analyzer
              clippy
              rustfmt
            ];
          };

          checks = {
            inherit loom-tui;

            loom-tui-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            loom-tui-fmt = craneLib.cargoFmt {
              inherit src;
            };
          };
        };
    };
}
