{ system, sources ? import ./sources.nix { inherit system; }
, nixpkgs ? sources.nixpkgs }:
let pkgs = import nixpkgs { inherit system; };
in with pkgs;
let
  xorgLibraries = with xorg; [ libX11 libXcursor libXrandr libXi ];
  otherLibraries = [ amdvlk vulkan-loader wayland ];
  neededLibPaths = lib.concatStringsSep ":"
    (map (p: "${p}/lib") (xorgLibraries ++ otherLibraries));

  crateDeps = {
    rfd = [ pkg-config gtk3 ];
    openssl-sys = [ pkg-config cmake openssl ];
    expat-sys = [ pkg-config cmake expat ];
    servo-freetype-sys = [ pkg-config cmake freetype ];
    servo-fontconfig-sys = [ pkg-config freetype expat fontconfig ];
    x11 = [ pkg-config x11 ];
    xcb = [ python3 ];
    icy_matrix = [ pkg-config gtk3 glib atk cairo pango gdk_pixbuf ];
  };
in { inherit pkgs neededLibPaths crateDeps; }
