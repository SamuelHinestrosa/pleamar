# pleamar, built from this source. `nix build`, `nix run`, or through the
# overlay (`pkgs.pleamar`).
{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  wayland,
  libxkbcommon,
  vulkan-loader,
  fontconfig,
  pam,
}:
rustPlatform.buildRustPackage {
  pname = "pleamar";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSourceWith {
    src = ../.;
    # Not the README's pictures: a new screenshot is not a new pleamar.
    filter = path: _type: !(lib.hasInfix "/assets" path);
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  buildInputs = [
    wayland
    libxkbcommon
    # The lock screen checks the password with PAM.
    pam
  ];

  # The language tests need a screen for some of their examples; the build
  # itself checks every scene it carries (`include_str!`).
  doCheck = false;

  # The card is reached through Vulkan, loaded at run time (wgpu), so its
  # loader has to be where it looks. The system fonts' aliases come from
  # fontconfig's files.
  postFixup = ''
    wrapProgram $out/bin/pleamar \
      --prefix LD_LIBRARY_PATH : ${
        lib.makeLibraryPath [
          vulkan-loader
          wayland
          libxkbcommon
        ]
      } \
      --set-default FONTCONFIG_FILE ${fontconfig.out}/etc/fonts/fonts.conf
  '';

  meta = {
    description = "A language and a Rust runtime for the desktop shell, where animation never waits for logic";
    homepage = "https://github.com/k4ditano/pleamar";
    license = lib.licenses.bsd3;
    mainProgram = "pleamar";
    platforms = lib.platforms.linux;
  };
}
