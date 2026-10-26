{
  description = "Axum";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };
  outputs =
    { self, nixpkgs, ... }@inputs:
    let
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        config = {
          allowUnfree = true;
        };
      };
    in
    {
      devShell.x86_64-linux = pkgs.mkShell {
        buildInputs = [
          pkgs.cargo
          pkgs.rustc
          pkgs.rust-analyzer
          pkgs.rustfmt
          pkgs.openssl
          pkgs.pkg-config
          pkgs.sqlx-cli
        ];
      };

      packages.x86_64-linux.default = pkgs.rustPlatform.buildRustPackage {
        pname = "yt-download-api";
        version = "0.1.0";

        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        nativeBuildInputs = [
          pkgs.makeWrapper
        ];

        buildInputs = [
          pkgs.openssl
        ];

        postInstall = ''
          wrapProgram $out/bin/yt-download-api \
            --prefix PATH : ${
              pkgs.lib.makeBinPath [
                pkgs.yt-dlp
                pkgs.ffmpeg
              ]
            }
        '';
      };
    };
}
