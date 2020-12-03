{ system, sources, nixpkgs }:
let
  mozPkgs = import "${sources.nixpkgsMoz}/package-set.nix" {
    pkgs = import nixpkgs { inherit system; };
  };
  rustChannel = mozPkgs.latest.rustChannels.stable;
  pkgs = import nixpkgs {
    inherit system;
    overlays = [
      (final: prev: {
        rustc = rustChannel.rust;
        inherit (rustChannel)
          ;

        crate2nix = prev.callPackage sources.crate2nix { pkgs = prev; };
      })
    ];
  };
in
with pkgs;
let
  # Libraries needed to run icy_matrix (graphics stuff)
  neededLibs = (with xorg; [ libX11 libXcursor libXrandr libXi ])
    ++ [ vulkan-loader wayland wayland-protocols ];

  # Deps that certain crates need
  crateDeps =
    let
      mkAttr = bi: nbi: {
        buildInputs = bi;
        nativeBuildInputs = nbi;
      };
    in
    {
      rfd = mkAttr [ gtk3 ] [ pkg-config ];
      openssl-sys = mkAttr [ cmake openssl ] [ pkg-config ];
      expat-sys = mkAttr [ expat ] [ cmake pkg-config ];
      servo-freetype-sys = mkAttr [ freetype ] [ pkg-config cmake ];
      servo-fontconfig-sys = mkAttr [ freetype expat fontconfig ] [ pkg-config ];
      x11 = mkAttr [ x11 ] [ pkg-config ];
      xcb = mkAttr [ ] [ python3 ];
      icy_matrix = mkAttr [ gtk3 glib atk cairo pango gdk_pixbuf ] [ pkg-config ];
    };

  getCrateInputs = with lib;
    name:
    concatLists (map (attr: attr."${name}") (attrValues crateDeps));
  crateBuildInputs = getCrateInputs "buildInputs";
  crateNativeBuildInputs = getCrateInputs "nativeBuildInputs";

in
{
  inherit pkgs neededLibs crateDeps crateBuildInputs crateNativeBuildInputs;
}
