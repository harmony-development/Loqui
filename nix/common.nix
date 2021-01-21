{ sources, system }:
let
  pkgz = import sources.nixpkgs { inherit system; };
  mozPkgs = import "${sources.nixpkgsMoz}/package-set.nix" { pkgs = pkgz; };

  rustChannel =
    let
      channel = mozPkgs.rustChannelOf {
        date = "2020-12-31";
        channel = "stable";
        sha256 = "sha256-KCh2UBGtdlBJ/4UOqZlxUtcyefv7MH1neoVNV4z0nWs=";
      };
    in
    channel // {
      rust = channel.rust.override { extensions = [ "rust-src" "clippy-preview" "rustfmt-preview" ]; };
    };

  pkgs = import sources.nixpkgs {
    inherit system;
    overlays = [
      (final: prev: {
        rustc = rustChannel.rust;
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
