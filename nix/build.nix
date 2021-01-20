{ release ? false
, common
,
}:
with common;
let
  # TODO: Need an icon
  desktopFile = pkgs.makeDesktopItem rec {
    name = "rucies";
    exec = name;
    comment = "rucies is a Harmony client written in Rust.";
    desktopName = "Rucies";
    genericName = "Harmony Client";
    categories = "Network;";
  };

  meta = with pkgs.stdenv.lib; {
    description = "rucies is a Harmony client written in Rust.";
    longDescription = ''
      rucies is a Harmony client written in Rust using the iced GUI library.

      It aims to be lightweight with a good out-of-the-box experience.
    '';
    upstream = "https://github.com/harmony-development/rucies";
    license = licenses.gpl3;
    maintainers = [ maintainers.yusdacra ];
  };

  icyHarmony = with pkgs; naersk.buildPackage {
    root = ../.;
    nativeBuildInputs = crateDeps.nativeBuildInputs;
    buildInputs = crateDeps.buildInputs;
    overrideMain = (prev: {
      inherit meta;

      nativeBuildInputs = prev.nativeBuildInputs ++ [ makeWrapper copyDesktopItems wrapGAppsHook ];
      desktopItems = [ desktopFile ];
      fixupPhase = ''
        wrapProgram $out/bin/rucies\
          --set LD_LIBRARY_PATH ${lib.makeLibraryPath neededLibs}\
          --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
      '';
    });
    inherit release;
  };
in
icyHarmony
