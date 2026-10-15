{
  description = "Worktree pool development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/7ae733852e8551bcf17cf1ad760ffe18df7a3be0";
  outputs = { nixpkgs, ... }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = with pkgs; [ cargo rustc rustfmt clippy gcc gnumake git python3 file ];
    };
  };
}
