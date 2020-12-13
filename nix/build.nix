{
  # `crate2nix` doesn't support profiles in `Cargo.toml`, so default to release.
  # Otherwise bad performance (non-release is built with opt level 0)
  release ? true
, common
,
}:
with common;
let
  # TODO: Need an icon
  desktopFile = pkgs.makeDesktopItem rec {
    name = "icy_matrix";
    exec = name;
    comment = "icy_matrix is a Matrix client written in Rust.";
    desktopName = "Icy Matrix";
    genericName = "Matrix Client";
    categories = "Network;";
  };

  meta = with pkgs.stdenv.lib; {
    description = "icy_matrix is a Matrix client written in Rust.";
    longDescription = ''
      icy_matrix is a Matrix client written in Rust using the iced GUI library. It uses ruma and ruma-client to interact with the Matrix network.

      It aims to be lightweight with a good out-of-the-box experience.
    '';
    upstream = "https://gitlab.com/yusdacra/icy_matrix";
    license = licenses.gpl3;
    maintainers = [ maintainers.yusdacra ];
  };
in
with pkgs;
(callPackage ./Cargo.nix {
  defaultCrateOverrides =
    defaultCrateOverrides // {
      icy_matrix = _: {
        name = "icy_matrix";
        inherit meta;
        inherit (crateDeps.icy_matrix) buildInputs;
        nativeBuildInputs = crateDeps.icy_matrix.nativeBuildInputs
        ++ [ makeWrapper wrapGAppsHook copyDesktopItems ];
        desktopItems = [ desktopFile ];
        postFixup = ''
          wrapProgram $out/bin/icy_matrix\
            --set LD_LIBRARY_PATH ${lib.makeLibraryPath neededLibs}\
            --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
        '';
      };
    } // (with crateDeps; {
      rfd = _: rfd;
      x11 = _: x11;
      xcb = _: xcb;
      atk-sys = _: atk-sys;
      gdk-pixbuf-sys = _: gdk-pixbuf-sys;
      gdk-sys = _: gdk-sys;
      gtk-sys = _: gtk-sys;
      gobject-sys = _: gobject-sys;
      glib-sys = _: glib-sys;
      gio-sys = _: gio-sys;
      cairo-sys-rs = _: cairo-sys-rs;
      pango-sys = _: pango-sys;
      servo-fontconfig-sys = _: servo-fontconfig-sys;
      servo-freetype-sys = _: servo-freetype-sys;
      expat-sys = _: expat-sys;
      openssl-sys = _: openssl-sys;
    });
  inherit release pkgs;
}).rootCrate.build
