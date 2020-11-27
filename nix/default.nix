{
/* `crate2nix` doesn't support profiles in `Cargo.toml`, so default to release.
   Otherwise bad performance (non-release is built with opt level 0)
*/
release ? true, system ? builtins.currentSystem
, sources ? import ./sources.nix { inherit system; } }:

let
  common = import ./common.nix { inherit sources system; };
  inherit (common) pkgs;

  icy_matrix = with pkgs;
    (callPackage ./Cargo.nix {
      defaultCrateOverrides = with common;
        defaultCrateOverrides // {
          rfd = _: { buildInputs = crateDeps.rfd; };
          x11 = _: { buildInputs = crateDeps.x11; };
          xcb = _: { buildInputs = crateDeps.xcb; };
          servo-fontconfig-sys = _: {
            buildInputs = crateDeps.servo-fontconfig-sys;
          };
          servo-freetype-sys = _: {
            buildInputs = crateDeps.servo-freetype-sys;
          };
          expat-sys = _: { buildInputs = crateDeps.expat-sys; };
          openssl-sys = _: { buildInputs = crateDeps.openssl-sys; };
          icy_matrix = _: {
            buildInputs = crateDeps.icy_matrix;
            nativeBuildInputs = [ makeWrapper wrapGAppsHook ];
            postInstall = ''
              wrapProgram $out/bin/icy_matrix\
                --set LD_LIBRARY_PATH ${neededLibPaths}\
                --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
            '';
          };
        };
      inherit release pkgs;
    }).rootCrate.build;
in pkgs.symlinkJoin {
  name = "icy_matrix-${icy_matrix.version}";
  version = icy_matrix.version;
  paths = [ icy_matrix ];
  meta = with pkgs; {
    description = "icy_matrix is a Matrix client written in Rust.";
    longDescription = ''
      icy_matrix is a Matrix client written in Rust using the iced GUI library. It uses ruma and ruma-client to interact with the Matrix network.

      It aims to be lightweight with a good out-of-the-box experience and have some amount of customization.
    '';
    upstream = "https://hub.darcs.net/yusdacra/icy_matrix";
    license = lib.licenses.gpl3;
    maintainers = [ lib.maintainers.yusdacra ];
    platforms = lib.platforms.all;
  };
}
