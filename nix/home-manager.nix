# home-manager: pleamar installed, and your shells started with the desktop.
#
#   programs.pleamar = {
#     enable = true;
#     autostart = true;   # runs ~/.config/pleamar/autostart with the graphical session
#   };
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.pleamar;
in
{
  options.programs.pleamar = {
    enable = lib.mkEnableOption "pleamar, the desktop shell language";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.pleamar;
      description = "The pleamar package.";
    };
    autostart = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Run ~/.config/pleamar/autostart (your shells) when the graphical session starts. On Hyprland, `exec-once = pleamar --autostart` does the same.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
    systemd.user.services.pleamar-autostart = lib.mkIf cfg.autostart {
      Unit = {
        Description = "Your pleamar shells";
        PartOf = [ "graphical-session.target" ];
        After = [ "graphical-session.target" ];
      };
      Service = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${cfg.package}/bin/pleamar --autostart";
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
