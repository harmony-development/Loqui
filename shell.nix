{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  name = "icy_matrix";
  nativeBuildInputs = with pkgs; [ pkg-config ];
  buildInputes = with pkgs; [ x11 alsaLib ];
}
