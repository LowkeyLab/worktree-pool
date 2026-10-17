{
  description = "Worktree pool development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/b185ba69c4b6bb6216efe3606338de06b2a21a93";
  outputs = { nixpkgs, ... }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = with pkgs; [ cargo rustc rustfmt clippy gcc gnumake git python3 file ];
    };
  };
}
