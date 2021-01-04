{ sources, system }:
let
  pkgs = import sources.nixpkgs { inherit system; };
  mozPkgs = import "${sources.nixpkgsMoz}/package-set.nix" { inherit pkgs; };

  rustChannel =
    let
      channel = mozPkgs.rustChannelOf {
        channel = "stable";
        sha256 = "sha256-KCh2UBGtdlBJ/4UOqZlxUtcyefv7MH1neoVNV4z0nWs=";
      };
    in
    channel // {
      rust = channel.rust.override { extensions = [ "rust-src" ]; };
    };
in rec {
  pkgs = import sources.nixpkgs {
    inherit system;
    overlays = [
      (final: prev: {
        rustc = rustChannel.rust;
        inherit (rustChannel);
      })
      (final: prev: {
        naersk = prev.callPackage sources.naersk { };
      })
    ];
  };

  # Libraries needed to run icy_matrix (graphics stuff)
  neededLibs = with pkgs; (with xorg; [ libX11 libXcursor libXrandr libXi ])
    ++ [ vulkan-loader wayland wayland-protocols libxkbcommon ];

  # Deps that certain crates need
  crateDeps =
    with pkgs;
    {
      buildInputs = [ gtk3 atk cairo pango gdk_pixbuf glib openssl expat freetype fontconfig x11 xorg.libxcb ];
      nativeBuildInputs = [ pkg-config cmake python3 ];
    };
}
