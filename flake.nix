{
  description = "FaaS Rust Project";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustChannel = pkgs.rust-bin.nightly.latest;
      in
      with pkgs;
      {
        devShells.default = mkShell {
          buildInputs = [
            openssl
            pkg-config
            protobuf
            containerd
            runc
            (rustChannel.default.override {
              extensions = [ "rust-src" ];
            })
          ];
        };
        defaultPackage = stdenv.mkDerivation {
          name = "faas-rs-${rustChannel._version}";
          src = ./.;  # 当前项目目录作为源码
          buildInputs = [
            openssl
            pkg-config
            protobuf
            rustChannel.rustc    # Rust 编译器
            rustChannel.cargo    # Cargo 工具
          ];
          buildPhase = ''
            cargo build --release
          '';
          installPhase = ''
            mkdir -p $out/bin
            cp target/release/* $out/bin
          '';
        };
      }
    );
}
