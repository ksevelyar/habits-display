{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.11";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachSystem ["x86_64-linux"] (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
        };
      in {
        devShell = pkgs.mkShell {
          SSID = "ssid";
          PASS = "pass";
          UTC_OFFSET = "180";
          JWT_TOKEN = "very secret";
          NOTIFICATIONS_HOST = "192.168.1.13:3003";

          buildInputs = with pkgs; [
            (rust-bin.nightly.latest.default.override {
              extensions = ["rust-src"];
              targets = ["riscv32imc-unknown-none-elf"];
            })

            (writeShellScriptBin "ci" ''
              set -euo pipefail
              cargo build --release
              cargo fmt --all -- --check --color always
              cargo clippy --all-features --workspace -- -D warnings
            '')

            websocat
            probe-rs-tools
            esp-generate
            rust-analyzer
          ];
        };
      }
    );
}
