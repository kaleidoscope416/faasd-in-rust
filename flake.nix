{
  description = "FaaS Rust Project";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url  = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
      # reference: https://crane.dev/examples/quick-start-workspace.html
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        inherit (pkgs) lib;

        rustToolchainFor = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" ];
        });
        
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchainFor;
        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
          # Add additional build inputs here
          buildInputs = with pkgs; [
            pkg-config
          ];

          nativeBuildInputs = with pkgs; [
            openssl
            protobuf
          ];
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          doCheck = false;
        };

        fileSetForCrate = crate: lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            (craneLib.fileset.commonCargoSources ./app)
            (craneLib.fileset.commonCargoSources ./service)
            (craneLib.fileset.commonCargoSources crate)
          ];
        };

        faas-rs-crate = craneLib.buildPackage ( individualCrateArgs // {
          pname = "faas-rs";
          cargoExtraArgs = "--bin faas-rs";
          src = fileSetForCrate ./app;
        });
      in
      with pkgs;
      {
        checks = { inherit faas-rs-crate; };

        packages.default = faas-rs-crate;

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          inputsFrom = [ faas-rs-crate ];

          packages = [
            pkgs.cargo-hakari
            pkgs.containerd
            pkgs.runc
          ];
        };
      }
    );
}
