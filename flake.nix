{
  description = "A configurable fetch tool — centered ASCII art with powerline panels";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "atlasfetch";
          version = "2.0.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          postInstall = ''
            cp -r logos $out/bin/logos
          '';

          meta = with pkgs.lib; {
            description = "A configurable fetch tool with centered ASCII art and powerline panels";
            longDescription = ''
              atlasfetch is a spiritual sibling of atlasWM — a Wayland compositor built
              around an infinite canvas. It displays system information with a centered
              ASCII logo and powerline panels, supporting 25 color presets and 18 distro
              logos. Compiled as a single Rust binary with zero runtime dependencies.
            '';
            homepage = "https://github.com/mafuzyk/atlasfetch";
            license = licenses.gpl3Plus;
            platforms = platforms.linux;
            maintainers = [ ];
            mainProgram = "atlasfetch";
          };
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/atlasfetch";
        };
      });
}
