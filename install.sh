#!/bin/sh
# Installs the latest release of SQL Pro on macOS or Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/necessarylion/sql-pro/main/install.sh | sh
#
# macOS (Apple silicon and Intel): puts "SQL Pro.app" in the Applications folder.
#   The app is not signed with an Apple Developer ID. A browser marks every download as
#   "from the internet", and macOS then blocks such apps. A download with curl does not get
#   that mark, so the app opens.
# Linux (x64 and arm64): installs the .deb or the .rpm with the package manager of the system.
#   A system with no such package manager gets the AppImage in ~/.local/bin.
# Windows: use install.ps1. See the README.
#
# Set SQL_PRO_INSTALL_DIR to install the app (macOS) or the AppImage (Linux) into another folder.
# Set SQL_PRO_VERSION to a tag, like v1.2.0, to install that release and not the latest one.
set -eu

REPO="necessarylion/sql-pro"
PAGE="https://github.com/$REPO/releases"

fail() {
    echo "$1" >&2
    exit 1
}

# Runs a command as root. Asks for the password with sudo when the user is not root.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    else
        echo "The package manager needs admin rights. sudo may ask for your password."
        sudo "$@"
    fi
}

# Prints the SHA-256 of the file $1. Prints nothing when the system has no tool for it.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# Downloads the file of the release whose name ends with $1 (a pattern for grep) to $2.
# GitHub puts a dot where the name has a space, so the files start with "SQL.Pro".
download() {
    url="$(grep -o "\"browser_download_url\": *\"[^\"]*$1\"" "$TMP/release.json" | head -1 | cut -d'"' -f4)"
    [ -n "$url" ] || fail "This release of SQL Pro has no file for your system. See $PAGE"
    echo "Downloading $url"
    curl -fL --progress-bar "$url" -o "$2" || fail "The download failed. Nothing was installed."

    # GitHub lists the SHA-256 of each file of a release. An old release may not have it.
    want="$(awk -F'"' -v url="$url" '$2 == "digest" { sum = $4 } $2 == "browser_download_url" && $4 == url { print sum; exit }' "$TMP/release.json")"
    got="$(sha256_of "$2")"
    if [ -n "$want" ] && [ -n "$got" ] && [ "sha256:$got" != "$want" ]; then
        fail "The downloaded file is broken (wrong checksum). Nothing was installed."
    fi
}

install_mac() {
    dest="${SQL_PRO_INSTALL_DIR:-/Applications}"
    # The release has one disk image for Apple silicon and one for Intel.
    case "$(uname -m)" in
        arm64) download '_aarch64\.dmg' "$TMP/app.dmg" ;;
        x86_64) download '_x64\.dmg' "$TMP/app.dmg" ;;
        *) fail "SQL Pro has no build for this chip ($(uname -m))." ;;
    esac
    mkdir "$MOUNT"
    hdiutil attach "$TMP/app.dmg" -nobrowse -quiet -mountpoint "$MOUNT"

    # Check that the download is the app we expect before it replaces an older copy.
    [ -x "$MOUNT/SQL Pro.app/Contents/MacOS/sql-pro" ] || fail "The downloaded app is broken. Nothing was installed."

    rm -rf "$dest/SQL Pro.app"
    cp -R "$MOUNT/SQL Pro.app" "$dest/SQL Pro.app"
    echo "SQL Pro is installed in $dest."
    echo "The app is not signed by Apple. It still opens, because a download with curl"
    echo "does not get the \"from the internet\" mark that makes macOS block such apps."
    echo "To open the application run: open \"$dest/SQL Pro.app\""
}

install_linux() {
    # The same chip has another name in each kind of file.
    case "$(uname -m)" in
        x86_64 | amd64) deb="amd64" rpm="x86_64" image="amd64" ;;
        aarch64 | arm64) deb="arm64" rpm="aarch64" image="aarch64" ;;
        *) fail "SQL Pro has no build for this chip ($(uname -m)). It needs 64-bit Intel, AMD or ARM." ;;
    esac

    if command -v apt-get >/dev/null 2>&1; then
        download "_$deb\\.deb" "$TMP/sql-pro.deb"
        # Everyone must be able to read the file, because apt reads it as another user.
        chmod 755 "$TMP" && chmod 644 "$TMP/sql-pro.deb"
        as_root apt-get install -y "$TMP/sql-pro.deb"
    elif command -v dnf >/dev/null 2>&1; then
        download "\\.$rpm\\.rpm" "$TMP/sql-pro.rpm"
        as_root dnf install -y "$TMP/sql-pro.rpm"
    elif command -v yum >/dev/null 2>&1; then
        download "\\.$rpm\\.rpm" "$TMP/sql-pro.rpm"
        as_root yum install -y "$TMP/sql-pro.rpm"
    elif command -v zypper >/dev/null 2>&1; then
        download "\\.$rpm\\.rpm" "$TMP/sql-pro.rpm"
        # The package is not signed, so zypper must be told that this is fine.
        as_root zypper --non-interactive install --allow-unsigned-rpm "$TMP/sql-pro.rpm"
    else
        dest="${SQL_PRO_INSTALL_DIR:-$HOME/.local/bin}"
        download "_$image\\.AppImage" "$TMP/sql-pro.AppImage"
        mkdir -p "$dest"
        mv "$TMP/sql-pro.AppImage" "$dest/sql-pro.AppImage"
        chmod +x "$dest/sql-pro.AppImage"
        echo "SQL Pro is installed. To open the application run \"$dest/sql-pro.AppImage\""
        return
    fi
    echo "SQL Pro is installed. Open it from the list of your applications."
}

main() {
    TMP="$(mktemp -d)"
    MOUNT="$TMP/mount"
    trap 'hdiutil detach "$MOUNT" -quiet 2>/dev/null || true; rm -rf "$TMP"' EXIT

    if [ -n "${SQL_PRO_VERSION:-}" ]; then
        api="https://api.github.com/repos/$REPO/releases/tags/$SQL_PRO_VERSION"
    else
        api="https://api.github.com/repos/$REPO/releases/latest"
    fi
    curl -fsSL "$api" -o "$TMP/release.json" 2>/dev/null ||
        fail "Could not find a release of SQL Pro. Maybe none is published yet, or GitHub cannot be reached. See $PAGE"

    case "$(uname -s)" in
        Darwin) install_mac ;;
        Linux) install_linux ;;
        *) fail "This script is for macOS and Linux. On Windows, use install.ps1. See $PAGE" ;;
    esac
}

# The whole script is read before anything runs. A download that stops halfway does nothing.
main
