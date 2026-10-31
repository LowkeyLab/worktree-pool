{
  description = "Worktree pool development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/4d438fb7c12dce27206a4a94f78a52c603bc8198";
  outputs = { nixpkgs, ... }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = with pkgs; [ cargo rustc rustfmt clippy gcc gnumake git python3 file ];
    };
  };
}
