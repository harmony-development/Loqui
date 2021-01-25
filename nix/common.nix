{ sources, system }:
let
  pkgz = import sources.nixpkgs { inherit system; overlays = [ sources.rustOverlay.overlay ]; };
  rustChannel = pkgz.rust-bin.stable.latest;

  pkgs = import sources.nixpkgs {
    inherit system;
    overlays = [
      (final: prev: {
        rustc = rustChannel.rust.override {
          extensions = [ "rust-src" ];
        };
      })
      (final: prev: {
        naersk = prev.callPackage sources.naersk { };
      })
    ];
  };
in
with pkgs; {
  inherit pkgs;

  # Libraries needed to run crust (graphics stuff)
  neededLibs = (with xorg; [ libX11 libXcursor libXrandr libXi ])
    ++ [ vulkan-loader wayland wayland-protocols libxkbcommon ];

  # Deps that certain crates need
  crateDeps =
    {
      buildInputs = [ protobuf gtk3 atk cairo pango gdk_pixbuf glib expat freetype fontconfig x11 xorg.libxcb ];
      nativeBuildInputs = [ pkg-config cmake python3 ];
    };

  env = {
    PROTOC = "${protobuf}/bin/protoc";
    PROTOC_INCLUDE = "${protobuf}/include";
  };
}
