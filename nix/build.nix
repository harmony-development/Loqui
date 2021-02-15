{ release ? false
, common
,
}:
with common;
let
  # TODO: Need an icon
  desktopFile = pkgs.makeDesktopItem rec {
    name = "Crust";
    exec = name;
    comment = "Crust is a Harmony client written in Rust.";
    desktopName = "Crust";
    genericName = "Harmony Client";
    categories = "Network;";
  };

  meta = with pkgs.lib; {
    description = "Crust is a Harmony client written in Rust.";
    longDescription = ''
      Crust is a Harmony client written in Rust using the iced GUI library.

      It aims to be lightweight with a good out-of-the-box experience.
    '';
    upstream = "https://github.com/harmony-development/crust";
    license = licenses.gpl3;
    maintainers = [ maintainers.yusdacra ];
  };

  crust = with pkgs; naersk.buildPackage {
    root = ../.;
    nativeBuildInputs = crateDeps.nativeBuildInputs;
    buildInputs = crateDeps.buildInputs;
    overrideMain = (prev: {
      inherit meta;

      nativeBuildInputs = prev.nativeBuildInputs ++ [ makeWrapper copyDesktopItems wrapGAppsHook ];
      desktopItems = [ desktopFile ];
      fixupPhase = ''
        wrapProgram $out/bin/crust\
          --set LD_LIBRARY_PATH ${lib.makeLibraryPath neededLibs}\
          --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
      '';
    });
    inherit release;
  };
in
crust
