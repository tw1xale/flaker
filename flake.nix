{
  description = "Flaker: Interactive TUI manager for NixOS Flakes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      # Overlay for consuming flaker as pkgs.flaker in other flakes
      overlay = final: prev: {
        flaker = self.packages.${final.system}.default;
      };
    in
    {
      overlays.default = overlay;
    } //
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        flakerPkg = pkgs.rustPlatform.buildRustPackage {
          pname = "flaker";
          version = "0.1.0";
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          meta = with pkgs.lib; {
            description = "Interactive TUI manager for NixOS Flakes";
            homepage = "https://github.com/tw1xale/flaker";
            license = licenses.mit;
            mainProgram = "flaker";
            maintainers = [ ];
          };
        };
      in
      {
        packages = {
          default = flakerPkg;
          flaker = flakerPkg;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = flakerPkg;
            name = "flaker";
          };
          flaker = flake-utils.lib.mkApp {
            drv = flakerPkg;
            name = "flaker";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
            pkg-config
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };
      });
}
