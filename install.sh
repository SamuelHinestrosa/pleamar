#!/bin/sh
# pleamar: install it, keep it up to date, take it away. One script for all:
#
#   curl -fsSL https://raw.githubusercontent.com/k4ditano/pleamar/main/install.sh | sh
#   ./install.sh                  install (or, if it is there, update)
#   pleamar-update                update: new changes, built and put in place
#   pleamar-update --session      also puts pleamar-wm in the login screen (asks for sudo)
#   pleamar-update --uninstall    takes the programs away; your ~/.config/pleamar stays
#
# What it installs, all in your home and nothing else:
#   ~/.local/share/pleamar/src/   pleamar, pleamar-wm and Marea's source (git)
#   ~/.local/bin/                 pleamar, pleamar-wm, pleamar-session, marea, pleamar-update
#   ~/.config/pleamar/            your configuration, made once and never written over
#   the AI agents you have        pleamar's skill (Claude Code, Codex, OpenCode)
#
# From a copy of the source you already have (a developer's), it builds that
# one instead of cloning: PLEAMAR_SRC=/folder/with/the/three ./install.sh
set -eu

repo_base="${PLEAMAR_REPOS:-https://github.com/k4ditano}"
repos="pleamar pleamar-wm marea-plm"
bin="${PLEAMAR_BIN:-$HOME/.local/bin}"
say() { printf '\033[1;36mpleamar ·\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31mpleamar ·\033[0m %s\n' "$*" >&2; exit 1; }

session=false
action=install
for a in "$@"; do
    case "$a" in
        --session) session=true ;;
        --uninstall) action=uninstall ;;
        --help|-h) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) fail "I don't know «$a» (--session, --uninstall, --help)" ;;
    esac
done

# Where the source is: PLEAMAR_SRC; or, run from inside a copy of pleamar that
# has the other two beside it, that folder; or its own place in your home.
here=$(dirname "$(readlink -f "$0" 2> /dev/null || echo "$0")")
if [ -n "${PLEAMAR_SRC:-}" ]; then
    src="$PLEAMAR_SRC"
elif [ -f "$here/Cargo.toml" ] && [ -d "$here/../pleamar-wm" ] && [ -d "$here/../marea-plm" ]; then
    src=$(cd "$here/.." && pwd)
elif [ -f "$here/.pleamar-src" ]; then
    src=$(cat "$here/.pleamar-src")
else
    src="$HOME/.local/share/pleamar/src"
fi
# Only the source it keeps itself is brought up to date with git; a copy of
# yours (a developer's) is built as it is: its git is yours.
managed=false
[ "$src" = "$HOME/.local/share/pleamar/src" ] && managed=true
[ "${PLEAMAR_PULL:-}" = 1 ] && managed=true

if [ "$action" = uninstall ]; then
    for f in pleamar pleamar-wm pleamar-session marea pleamar-update .pleamar-src; do
        rm -f "$bin/$f"
    done
    say "the programs are gone from $bin"
    say "left as they were: your ~/.config/pleamar, and the source in $src"
    if [ -f /usr/share/wayland-sessions/pleamar-wm.desktop ]; then
        say "the login screen still offers pleamar-wm: sudo rm /usr/share/wayland-sessions/pleamar-wm.desktop /usr/local/bin/pleamar-wm-session"
    fi
    exit 0
fi

# ── what it needs to be built ──────────────────────────────────────
missing=""
command -v git > /dev/null || missing="$missing git"
command -v cargo > /dev/null || missing="$missing cargo"
command -v cc > /dev/null || command -v gcc > /dev/null || missing="$missing a-C-compiler"
command -v c++ > /dev/null || command -v g++ > /dev/null || command -v clang++ > /dev/null || missing="$missing a-C++-compiler"
# PipeWire's bindings (pleamar-wm shares the screen) are made with libclang.
command -v clang > /dev/null || missing="$missing clang"
if command -v pkg-config > /dev/null; then
    for lib in libinput libseat libudev gbm xkbcommon libpipewire-0.3; do
        pkg-config --exists "$lib" || missing="$missing $lib"
    done
else
    missing="$missing pkg-config"
fi
if [ -n "$missing" ]; then
    say "to build it, this is missing:$missing"
    if command -v pacman > /dev/null; then
        hint="sudo pacman -S --needed git rust base-devel pkgconf clang libinput seatd libxkbcommon mesa vulkan-icd-loader xorg-xwayland pipewire"
    elif command -v apt > /dev/null; then
        hint="sudo apt install git cargo build-essential pkg-config clang libclang-dev libinput-dev libseat-dev libudev-dev libgbm-dev libxkbcommon-dev libvulkan1 xwayland libpipewire-0.3-dev"
    elif command -v dnf > /dev/null; then
        hint="sudo dnf install git cargo gcc-c++ pkgconf clang-devel libinput-devel libseat-devel systemd-devel mesa-libgbm-devel libxkbcommon-devel vulkan-loader xorg-x11-server-Xwayland pipewire-devel"
    elif command -v zypper > /dev/null; then
        hint="sudo zypper install git cargo gcc-c++ pkgconf libinput-devel libseat-devel systemd-devel libgbm-devel libxkbcommon-devel libvulkan1 xwayland"
    else
        hint="your distribution's packages for:$missing"
    fi
    say "install it with:"
    echo "    $hint"
    if [ -t 0 ] && [ "${hint#sudo }" != "$hint" ]; then
        printf 'pleamar · run it now? [y/N] '
        read -r yes
        case "$yes" in y|Y|s|S) sh -c "$hint" ;; *) exit 1 ;; esac
    else
        exit 1
    fi
fi

# ── the source: fetched, or brought up to date ────────────────────
mkdir -p "$src"
for r in $repos; do
    if [ -d "$src/$r/.git" ] && ! $managed; then
        say "$r: your copy in $src/$r, built as it is"
    elif [ -d "$src/$r/.git" ]; then
        before=$(git -C "$src/$r" rev-parse HEAD)
        if git -C "$src/$r" diff --quiet && git -C "$src/$r" diff --cached --quiet; then
            git -C "$src/$r" pull --ff-only --quiet || say "$r: could not bring it up to date (left as it was)"
        else
            say "$r: has changes of yours; not updated (built as it is)"
        fi
        after=$(git -C "$src/$r" rev-parse HEAD)
        if [ "$before" != "$after" ]; then
            say "$r: new since last time:"
            git -C "$src/$r" log --oneline --no-decorate "$before..$after" | sed 's/^/    /' | head -20
        fi
    else
        say "fetching $r"
        git clone --quiet "$repo_base/$r.git" "$src/$r" || fail "could not fetch $repo_base/$r.git"
    fi
done

# ── built ──────────────────────────────────────────────────────────
say "building pleamar and pleamar-wm (the first time takes a few minutes)"
(cd "$src/pleamar" && cargo build --release --quiet) || fail "pleamar did not build"
(cd "$src/pleamar-wm" && cargo build --release --quiet) || fail "pleamar-wm did not build"

# ── put in place ───────────────────────────────────────────────────
# Copied, not linked: a build half-way through never leaves a broken program.
mkdir -p "$bin"
for p in pleamar/target/release/pleamar pleamar-wm/target/release/pleamar-wm; do
    install -m755 "$src/$p" "$bin/$(basename "$p").new"
    mv -f "$bin/$(basename "$p").new" "$bin/$(basename "$p")"
done
install -m755 "$src/pleamar-wm/session.sh" "$bin/pleamar-session"
# Marea runs from her folder (her scene, her images): a link to her launcher.
ln -sf "$src/marea-plm/marea" "$bin/marea"
install -m755 "$src/pleamar/install.sh" "$bin/pleamar-update"
echo "$src" > "$bin/.pleamar-src"

# Your folder, the first time; never over what is there.
"$bin/pleamar-wm" init > /dev/null
# The AI agents you have learn pleamar (and it refreshes itself on updates).
"$bin/pleamar" --install-skill | sed 's/^/    /'

# ── the login screen, if asked ─────────────────────────────────────
if $session; then
    say "pleamar-wm in the login screen (asks for your password)"
    sudo sh -c "
        printf '#!/bin/sh\n# pleamar-wm as the login screen starts it (pleamar-update --session).\nexport XDG_CURRENT_DESKTOP=pleamar XDG_SESSION_DESKTOP=pleamar XDG_SESSION_TYPE=wayland PLEAMAR_WM_EXPORT=1\nexec \"\$HOME/.local/bin/pleamar-session\" \"\$@\"\n' > /usr/local/bin/pleamar-wm-session
        chmod 755 /usr/local/bin/pleamar-wm-session
        install -Dm644 '$src/pleamar-wm/pleamar-wm.desktop' /usr/share/wayland-sessions/pleamar-wm.desktop
        install -Dm644 '$src/pleamar-wm/pleamar-portals.conf' /usr/share/xdg-desktop-portal/pleamar-portals.conf
        install -Dm644 '$src/pleamar-wm/pleamar.portal' /usr/share/xdg-desktop-portal/portals/pleamar.portal
    "
fi

version=$("$bin/pleamar" --version 2> /dev/null || echo "")
say "ready: $version"
case ":$PATH:" in
    *":$bin:"*) ;;
    *) say "add $bin to your PATH (in ~/.profile: export PATH=\"$bin:\$PATH\")" ;;
esac
cat << EOF

  Your things:      ~/.config/pleamar   (session.conf, keys.conf, autostart, shells/, wm/)
  On Hyprland:      exec-once = pleamar --autostart      (your shells, Marea)
  Its own session:  pleamar-session from a TTY, or pleamar-update --session for the login screen
  Up to date:       pleamar-update
  Docs:             pleamar --docs      Ask your AI agent: it knows pleamar now.

EOF
