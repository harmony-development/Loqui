{ crate2nix, nixpkgs, system }:
rec {
  pkgs = import nixpkgs {
    inherit system;
    overlays = [
      (final: prev: {
        crate2nix = prev.callPackage crate2nix { pkgs = prev; };
      })
    ];
  };

  # Libraries needed to run icy_matrix (graphics stuff)
  neededLibs = with pkgs; (with xorg; [ libX11 libXcursor libXrandr libXi ])
    ++ [ vulkan-loader wayland wayland-protocols libxkbcommon ];

  # Deps that certain crates need
  crateDeps =
    let
      mkDeps = b: n: {
        buildInputs = b;
        nativeBuildInputs = n;
      };
    in
    with pkgs;
    {
      rfd = mkDeps [ gtk3 ] [ pkg-config ];
      atk-sys = mkDeps [ atk ] [ pkg-config ];
      cairo-sys-rs = mkDeps [ cairo ] [ pkg-config ];
      pango-sys = mkDeps [ pango ] [ pkg-config ];
      gdk-pixbuf-sys = mkDeps [ gdk_pixbuf ] [ pkg-config ];
      gdk-sys = mkDeps [ gtk3 ] [ pkg-config ];
      gtk-sys = mkDeps [ gtk3 ] [ pkg-config ];
      glib-sys = mkDeps [ glib ] [ pkg-config ];
      gio-sys = mkDeps [ glib ] [ pkg-config ];
      gobject-sys = mkDeps [ glib ] [ pkg-config ];
      openssl-sys = mkDeps [ openssl ] [ cmake pkg-config ];
      expat-sys = mkDeps [ expat ] [ cmake pkg-config ];
      servo-freetype-sys = mkDeps [ freetype ] [ pkg-config cmake ];
      servo-fontconfig-sys = mkDeps [ freetype expat fontconfig ] [ pkg-config ];
      x11 = mkDeps [ x11 ] [ pkg-config ];
      xcb = mkDeps [ ] [ python3 ];
      icy_matrix = mkDeps [ xorg.libxcb ] [ pkg-config ];
    };
}
