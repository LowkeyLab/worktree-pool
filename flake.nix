{
  description = "Worktree pool development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/c861d1f7d8450fa2eb449330cbdd5d7897ec595f";
  outputs = { nixpkgs, ... }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = with pkgs; [ cargo rustc rustfmt clippy gcc gnumake git python3 file ];
    };
  };
}
