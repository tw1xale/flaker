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
          version = "0.2.0";
          src = pkgs.lib.cleanSourceWith {
            src = self;
            filter = path: type:
              let baseName = baseNameOf (toString path); in
              !(pkgs.lib.hasPrefix "." baseName)
              && baseName != "target"
              && baseName != "assets"
              && baseName != "dist"
              && baseName != "result";
          };

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [
            pkgs.installShellFiles
          ];

          postInstall = ''
            installManPage man/flaker.1
            installShellCompletion --cmd flaker \
              --bash completions/flaker.bash \
              --fish completions/flaker.fish \
              --zsh completions/_flaker
          '';

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
          default = {
            type = "app";
            program = "${flakerPkg}/bin/flaker";
            meta = flakerPkg.meta;
          };
          flaker = {
            type = "app";
            program = "${flakerPkg}/bin/flaker";
            meta = flakerPkg.meta;
          };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
            pkg-config
            git
            nix
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };

        formatter = pkgs.nixfmt;
      });
}
