{
  # `crate2nix` doesn't support profiles in `Cargo.toml`, so default to release.
  # Otherwise bad performance (non-release is built with opt level 0)
  release ? true
, system ? builtins.currentSystem
, sources ? import ./sources.nix { inherit system; }
, nixpkgs ? sources.nixpkgs
,
}:
let
  common = import ./common.nix { inherit sources system nixpkgs; };
  inherit (common) pkgs;

  # TODO: Need an icon
  desktopFile = pkgs.makeDesktopItem rec {
    name = "icy_matrix";
    exec = name;
    comment = "icy_matrix is a Matrix client written in Rust.";
    desktopName = "Icy Matrix";
    genericName = "Matrix Client";
    categories = "Network;";
  };

  icy_matrix = with pkgs;
    (callPackage ./Cargo.nix {
      defaultCrateOverrides = with common;
        defaultCrateOverrides // {
          icy_matrix = _: {
            inherit (crateDeps.icy_matrix) buildInputs;
            nativeBuildInputs = crateDeps.icy_matrix.nativeBuildInputs
            ++ [ makeWrapper wrapGAppsHook copyDesktopItems ];
            desktopItems = [ desktopFile ];
            postFixup = ''
              wrapProgram $out/bin/icy_matrix\
                --set LD_LIBRARY_PATH ${lib.makeLibraryPath common.neededLibs}\
                --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
            '';
          };
        } // (with crateDeps; {
          rfd = _: rfd;
          x11 = _: x11;
          xcb = _: xcb;
          servo-fontconfig-sys = _: servo-fontconfig-sys;
          servo-freetype-sys = _: servo-freetype-sys;
          expat-sys = _: expat-sys;
          openssl-sys = _: openssl-sys;
        });
      inherit release pkgs;
    }).rootCrate.build;
in
pkgs.symlinkJoin {
  name = "icy_matrix-${icy_matrix.version}";
  inherit (icy_matrix) version;
  paths = [ icy_matrix ];
  meta = with pkgs.lib; {
    description = "icy_matrix is a Matrix client written in Rust.";
    longDescription = ''
      icy_matrix is a Matrix client written in Rust using the iced GUI library. It uses ruma and ruma-client to interact with the Matrix network.

      It aims to be lightweight with a good out-of-the-box experience.
    '';
    upstream = "https://gitlab.com/yusdacra/icy_matrix";
    license = licenses.gpl3;
    maintainers = [ maintainers.yusdacra ];
    # TODO: Make it work on BSD and Mac OS
    platforms = platforms.linux;
  };
}
