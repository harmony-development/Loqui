{ release ? false
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

  icyMatrix = with pkgs; naersk.buildPackage {
    root = ../.;
    nativeBuildInputs = crateDeps.nativeBuildInputs;
    buildInputs = crateDeps.buildInputs;
    overrideMain = (prev: {
      inherit meta;

      nativeBuildInputs = prev.nativeBuildInputs ++ [ makeWrapper copyDesktopItems wrapGAppsHook ];
      desktopItems = [ desktopFile ];
      fixupPhase = ''
        wrapProgram $out/bin/icy_matrix\
          --set LD_LIBRARY_PATH ${lib.makeLibraryPath neededLibs}\
          --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
      '';
    });
    inherit release;
  };
in
icyMatrix
