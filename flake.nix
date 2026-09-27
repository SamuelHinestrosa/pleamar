{
  description = "pleamar: the desktop, written as scenes that never stop moving";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        pleamar = pkgs.callPackage ./nix/package.nix { };
        default = pleamar;
      });

      # `nix run github:k4ditano/pleamar -- --scene bar.plm`
      apps = forAll (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.system}.pleamar}/bin/pleamar";
        };
      });

      # `pkgs.pleamar` in your own configuration.
      overlays.default = final: _prev: {
        pleamar = final.callPackage ./nix/package.nix { };
      };

      # home-manager: `programs.pleamar.enable = true;` installs it and,
      # optionally, starts your shells with the desktop.
      homeManagerModules.default = import ./nix/home-manager.nix self;

      # `nix develop`: what building it from the source needs.
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.pleamar ];
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
          ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
            vulkan-loader
            wayland
            libxkbcommon
          ]);
        };
      });
    };
}
