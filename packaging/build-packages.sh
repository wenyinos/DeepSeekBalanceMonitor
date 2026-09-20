#!/bin/sh
# Builds the .deb and .rpm for one release binary, and for the desktop widget
# too when its binary sits beside the application's.
#
#   packaging/build-packages.sh <version> <architecture> <binary> [output-directory]
#
# The architecture is the name the distribution uses, `amd64` or `arm64`, and
# is translated to what each package format calls it (`x86_64` / `aarch64` for
# RPM). The binary has to have been built for that architecture.
#
# The widget is packaged apart from the application on purpose: it is a program
# of its own, it can be installed and upgraded on its own, and its version
# simply follows the tag both are released under. A release build leaves both
# binaries in one directory, which is all this script has to see:
#
#   cargo build --release -p dsmon-ui --bin dsmon2 --bin dsmon2-widget
#   packaging/build-packages.sh 2.1.3 amd64 target/release/dsmon2 dist
#
# Needs `dpkg-deb` for the Debian package, `rpmbuild` for the RPM, and
# ImageMagick to turn the application icon into the PNG sizes a desktop wants;
# Debian's ImageMagick 6 calls the tool `convert`, ImageMagick 7 calls it
# `magick`, and either is used below.
# CI runs this inside a container, where all three are installed.

set -eu

version="${1:?usage: build-packages.sh <version> <architecture> <binary> [output-directory]}"
architecture="${2:?usage: build-packages.sh <version> <architecture> <binary> [output-directory]}"
binary="${3:?usage: build-packages.sh <version> <architecture> <binary> [output-directory]}"
output="${4:-dist}"

case "$architecture" in
    amd64) deb_arch=amd64; rpm_arch=x86_64 ;;
    arm64) deb_arch=arm64; rpm_arch=aarch64 ;;
    *)
        echo "unknown architecture: $architecture (expected amd64 or arm64)" >&2
        exit 2
        ;;
esac

repository="$(cd "$(dirname "$0")/.." && pwd)"
arch="$deb_arch"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$output"

if command -v magick >/dev/null 2>&1; then
    icon_tool=magick
else
    icon_tool=convert
fi

# The application ships one icon, a Windows .ico holding every size. A desktop
# looks for the sizes it needs as PNGs, so they are cut out of it here. Each
# package carries its own copy under its own name, because one path owned by
# two packages is a conflict in both formats.
#
# icons <root> <icon-name>
icons() {
    for size in 256 128 64 48; do
        install -d "$1/usr/share/icons/hicolor/${size}x${size}/apps"
        "$icon_tool" "$repository/assets/app.ico[0]" -resize "${size}x${size}" \
            "$1/usr/share/icons/hicolor/${size}x${size}/apps/$2.png"
    done
}

# The paths one package owns, for the RPM manifest.
package_files() {
    case "$1" in
        deepseek-balance-monitor)
            printf '%s\n' \
                "/usr/bin/dsmon2" \
                "/usr/share/applications/deepseek-balance-monitor.desktop" \
                "/usr/share/icons/hicolor/*/apps/deepseek-balance-monitor.png"
            ;;
        deepseek-balance-monitor-widget)
            printf '%s\n' \
                "/usr/bin/dsmon2-widget" \
                "/usr/share/applications/deepseek-balance-monitor-widget.desktop" \
                "/usr/share/icons/hicolor/*/apps/deepseek-balance-monitor-widget.png"
            ;;
    esac
}

# --- Debian -----------------------------------------------------------------
# The X11, Wayland and Vulkan libraries are loaded at runtime rather than
# linked, so nothing pulls them in on its own. xwayland is a hard requirement:
# the window opens through it on a Wayland session, which is what lets a close
# put the window away and the tray bring it back. The widget opens its window
# the same way, so it asks for the same libraries.
#
# build_deb <name> <root> <depends> <summary> <detail>
build_deb() {
    # A machine usually has one of the two packaging tools, not both. What is
    # missing is said out loud and skipped, so the other format is still made.
    if ! command -v dpkg-deb >/dev/null 2>&1; then
        echo "dpkg-deb is not installed: no .deb for $1" >&2
        return 0
    fi

    mkdir -p "$2/DEBIAN"
    cat > "$2/DEBIAN/control" <<EOF
Package: $1
Version: $version
Section: utils
Priority: optional
Architecture: $arch
Depends: $3
Recommends: mesa-vulkan-drivers
Maintainer: wenyinos <https://github.com/wenyinos>
Homepage: https://github.com/wenyinos/DeepSeekBalanceMonitor
Description: $4
 $5
EOF

    dpkg-deb --root-owner-group --build "$2" "$output/${1}_${version}_${arch}.deb"
}

# --- RPM --------------------------------------------------------------------
# The same payload, with the names Fedora and its relatives use.
#
# build_rpm <name> <root> <requires> <summary> <detail>
build_rpm() {
    if ! command -v rpmbuild >/dev/null 2>&1; then
        echo "rpmbuild is not installed: no .rpm for $1" >&2
        return 0
    fi

    rpm_top="$work/rpm-$1"
    files="$(package_files "$1")"

    # One at a time: the shell this runs under is `sh`, which has no brace
    # expansion — `mkdir -p dir/{a,b}` would make a directory with that name.
    for directory in BUILD RPMS SOURCES SPECS SRPMS; do
        mkdir -p "$rpm_top/$directory"
    done

    # rpmbuild empties its build directory before a build, so the payload
    # travels as an archive of its own: the tree that the Debian package was
    # made from, packed as it stands.
    tar -czf "$rpm_top/SOURCES/payload.tar.gz" -C "$2" usr

    cat > "$rpm_top/SPECS/$1.spec" <<EOF
Name:           $1
Version:        $version
Release:        1
BuildArch:      $rpm_arch
Summary:        $4
License:        MIT
URL:            https://github.com/wenyinos/DeepSeekBalanceMonitor
Requires:       $3
Recommends:     mesa-vulkan-drivers

%description
$5

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{_sourcedir}/payload.tar.gz -C %{buildroot}

%files
$files

%changelog
EOF

    rpmbuild -bb --define "_topdir $rpm_top" --define "_binary_payload w2.xzdio" \
        "$rpm_top/SPECS/$1.spec"
    find "$rpm_top/RPMS" -name '*.rpm' -exec cp {} "$output/" \;
}

only_glibc="libc6 (>= 2.36), libx11-6, libxkbcommon0, libwayland-client0, libvulkan1, xwayland, fonts-noto-cjk"
only_glibc_rpm="glibc >= 2.36, libX11, libxkbcommon, libwayland-client, vulkan-loader, xorg-x11-server-Xwayland, google-noto-sans-cjk-fonts"

# --- The application --------------------------------------------------------
app="deepseek-balance-monitor"
app_summary="DeepSeek account balance in the tray"
app_detail="Watches the balance of a DeepSeek account, and of the other providers it knows, and shows the reading in the system tray. It starts with the session and stays out of the way until asked."

app_root="$work/$app-$version"
install -Dm755 "$binary" "$app_root/usr/bin/dsmon2"
install -Dm644 "$repository/packaging/$app.desktop" \
    "$app_root/usr/share/applications/$app.desktop"
icons "$app_root" "$app"

build_deb "$app" "$app_root" "$only_glibc" "$app_summary" "$app_detail"
build_rpm "$app" "$app_root" "$only_glibc_rpm" "$app_summary" "$app_detail"

# --- The desktop widget -----------------------------------------------------
# A package of its own, and nothing in it but the widget: the application it
# reads from is what the description points at, since that is the one thing the
# widget cannot do without.
widget="deepseek-balance-monitor-widget"
widget_summary="Desktop widget for DeepSeek Balance Monitor"
widget_detail="Draws the balances and the subscription quotas on the desktop, reading them from the running DeepSeek Balance Monitor. It holds no keys and keeps no database of its own, and says so plainly while the application is not running."
widget_binary="${binary%/*}/dsmon2-widget"

if [ -x "$widget_binary" ]; then
    widget_root="$work/$widget-$version"
    install -Dm755 "$widget_binary" "$widget_root/usr/bin/dsmon2-widget"
    install -Dm644 "$repository/packaging/$widget.desktop" \
        "$widget_root/usr/share/applications/$widget.desktop"
    icons "$widget_root" "$widget"

    build_deb "$widget" "$widget_root" "$only_glibc" "$widget_summary" "$widget_detail"
    build_rpm "$widget" "$widget_root" "$only_glibc_rpm" "$widget_summary" "$widget_detail"
else
    echo "no dsmon2-widget beside $binary: building the application's packages only" >&2
fi

echo "Packages in $output ($architecture):"
ls -1 "$output"
