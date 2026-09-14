#!/bin/sh
# Builds the .deb and .rpm for one release binary.
#
#   packaging/build-packages.sh <version> <architecture> <binary> [output-directory]
#
# The architecture is the name the distribution uses, `amd64` or `arm64`, and
# is translated to what each package format calls it (`x86_64` / `aarch64` for
# RPM). The binary has to have been built for that architecture.
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
name="deepseek-balance-monitor"
arch="$deb_arch"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

root="$work/$name-$version"
install -Dm755 "$binary" "$root/usr/bin/dsmon2"
install -Dm644 "$repository/packaging/$name.desktop" \
    "$root/usr/share/applications/$name.desktop"

# The application ships one icon, a Windows .ico holding every size. A desktop
# looks for the sizes it needs as PNGs, so they are cut out of it here.
if command -v magick >/dev/null 2>&1; then
    icon_tool=magick
else
    icon_tool=convert
fi

for size in 256 128 64 48; do
    install -d "$root/usr/share/icons/hicolor/${size}x${size}/apps"
    "$icon_tool" "$repository/assets/app.ico[0]" -resize "${size}x${size}" \
        "$root/usr/share/icons/hicolor/${size}x${size}/apps/$name.png"
done

mkdir -p "$output"

# --- Debian -----------------------------------------------------------------
# The X11, Wayland and Vulkan libraries are loaded at runtime rather than
# linked, so nothing pulls them in on its own. xwayland is a hard requirement:
# the window opens through it on a Wayland session, which is what lets a close
# put the window away and the tray bring it back.
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<EOF
Package: $name
Version: $version
Section: utils
Priority: optional
Architecture: $arch
Depends: libc6 (>= 2.36), libx11-6, libxkbcommon0, libwayland-client0, libvulkan1, xwayland, fonts-noto-cjk
Recommends: mesa-vulkan-drivers
Maintainer: wenyinos <https://github.com/wenyinos>
Homepage: https://github.com/wenyinos/DeepSeekBalanceMonitor
Description: DeepSeek account balance in the tray
 Watches the balance of a DeepSeek account, and of the other providers it
 knows, and shows the reading in the system tray. It starts with the session
 and stays out of the way until asked.
EOF

dpkg-deb --root-owner-group --build "$root" "$output/${name}_${version}_${arch}.deb"

# --- RPM --------------------------------------------------------------------
# The same payload, with the names Fedora and its relatives use.
rpm_top="$work/rpm"
mkdir -p "$rpm_top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# rpmbuild empties its build directory before a build, so the payload travels
# as an archive of its own: the tree that the Debian package was made from,
# packed as it stands.
tar -czf "$rpm_top/SOURCES/payload.tar.gz" -C "$root" usr

cat > "$rpm_top/SPECS/$name.spec" <<EOF
Name:           $name
Version:        $version
Release:        1
BuildArch:      $rpm_arch
Summary:        DeepSeek account balance in the tray
License:        MIT
URL:            https://github.com/wenyinos/DeepSeekBalanceMonitor
Requires:       glibc >= 2.36, libX11, libxkbcommon, libwayland-client, vulkan-loader, xorg-x11-server-Xwayland, google-noto-sans-cjk-fonts
Recommends:     mesa-vulkan-drivers

%description
Watches the balance of a DeepSeek account, and of the other providers it
knows, and shows the reading in the system tray.

%prep

%build

%install
mkdir -p %{buildroot}
tar -xf %{_sourcedir}/payload.tar.gz -C %{buildroot}

%files
/usr/bin/dsmon2
/usr/share/applications/$name.desktop
/usr/share/icons/hicolor/*/apps/$name.png

%changelog
EOF

rpmbuild -bb --define "_topdir $rpm_top" --define "_binary_payload w2.xzdio" \
    "$rpm_top/SPECS/$name.spec"
find "$rpm_top/RPMS" -name '*.rpm' -exec cp {} "$output/" \;

echo "Packages in $output ($architecture):"
ls -1 "$output"
