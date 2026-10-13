{
  description = "Worktree pool development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/db81a1d5ba9fc2b2ac5305557a7342a594e709e4";
  outputs = { nixpkgs, ... }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = with pkgs; [ cargo rustc rustfmt clippy gcc gnumake git python3 file ];
    };
  };
}
