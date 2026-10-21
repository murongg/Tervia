#!/usr/bin/env bash
set -euo pipefail
TERVIA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$TERVIA_ROOT"
if [[ "$(uname -s)" != Linux ]]; then
  printf '%s\n' 'Build this package on Linux.' >&2
  exit 1
fi
for tool in dpkg-deb dpkg-shlibdeps rpmbuild; do
  command -v "$tool" >/dev/null || { printf 'Missing %s; install dpkg-dev and rpm.\n' "$tool" >&2; exit 1; }
done
if [[ "${1:-}" != --no-build ]]; then cargo build --release --features native; fi
TERVIA_ARCH="$(uname -m)"
case "$TERVIA_ARCH" in
  x86_64) TERVIA_DEB_ARCH=amd64 ;;
  aarch64) TERVIA_DEB_ARCH=arm64 ;;
  *) printf 'Unsupported Linux architecture: %s\n' "$TERVIA_ARCH" >&2; exit 1 ;;
esac
TERVIA_PACKAGE_ID="$(cargo pkgid --locked --offline)"
TERVIA_VERSION="${TERVIA_PACKAGE_ID##*@}"
TERVIA_VERSION="${TERVIA_VERSION%%+*}"
TERVIA_CORE="${TERVIA_VERSION%%-*}"
TERVIA_DEB_VERSION=$TERVIA_CORE
TERVIA_RPM_RELEASE=1
if [[ "$TERVIA_VERSION" == *-* ]]; then
  TERVIA_PRERELEASE="${TERVIA_VERSION#*-}"
  # Both package managers must order prereleases before the final numeric version.
  TERVIA_DEB_VERSION="$TERVIA_CORE~$TERVIA_PRERELEASE"
  TERVIA_RPM_RELEASE="0.${TERVIA_PRERELEASE//-/.}"
fi
TERVIA_PACKAGE="$TERVIA_ROOT/build/linux/Tervia"
rm -rf -- "$TERVIA_PACKAGE" "$TERVIA_ROOT/build/linux/deb"
mkdir -p "$TERVIA_PACKAGE"
install -m 755 target/release/tervia "$TERVIA_PACKAGE/tervia"
cp assets/tervia.svg "$TERVIA_PACKAGE/tervia.svg"
cp LICENSE "$TERVIA_PACKAGE/LICENSE"
cat assets/lobe-license.txt assets/hermes-license.txt assets/material-license.txt > "$TERVIA_PACKAGE/ThirdPartyNotices.txt"
cat > "$TERVIA_PACKAGE/tervia.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Tervia
Exec=tervia
Icon=tervia
Terminal=false
Categories=Development;TerminalEmulator;
DESKTOP
tar -C build/linux -czf "build/Tervia-linux-$TERVIA_ARCH.tar.gz" Tervia

TERVIA_DEB="$TERVIA_ROOT/build/linux/deb"
mkdir -p "$TERVIA_DEB/usr/bin" "$TERVIA_DEB/usr/share/applications" \
  "$TERVIA_DEB/usr/share/icons/hicolor/scalable/apps" "$TERVIA_DEB/usr/share/doc/tervia"
install -m755 "$TERVIA_PACKAGE/tervia" "$TERVIA_DEB/usr/bin/tervia"
install -m644 "$TERVIA_PACKAGE/tervia.desktop" "$TERVIA_DEB/usr/share/applications/tervia.desktop"
install -m644 "$TERVIA_PACKAGE/tervia.svg" "$TERVIA_DEB/usr/share/icons/hicolor/scalable/apps/tervia.svg"
install -m644 "$TERVIA_PACKAGE/LICENSE" "$TERVIA_DEB/usr/share/doc/tervia/LICENSE"
install -m644 "$TERVIA_PACKAGE/ThirdPartyNotices.txt" "$TERVIA_DEB/usr/share/doc/tervia/ThirdPartyNotices.txt"

TERVIA_SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/tervia-linux.XXXXXX")"
trap 'rm -rf -- "$TERVIA_SCRATCH"' EXIT
mkdir -p "$TERVIA_SCRATCH/debian" "$TERVIA_DEB/DEBIAN"
cat > "$TERVIA_SCRATCH/debian/control" <<CONTROL
Source: tervia
Section: devel
Priority: optional
Maintainer: Tervia contributors

Package: tervia
Architecture: $TERVIA_DEB_ARCH
Description: A native terminal workspace with Git and lightweight editing
CONTROL
TERVIA_DEPENDENCIES="$(cd "$TERVIA_SCRATCH" && dpkg-shlibdeps -O -e"$TERVIA_DEB/usr/bin/tervia")"
TERVIA_DEPENDENCIES="${TERVIA_DEPENDENCIES#shlibs:Depends=}"
[[ -n "$TERVIA_DEPENDENCIES" ]] || { printf '%s\n' 'No shared-library dependencies were detected.' >&2; exit 1; }
cat > "$TERVIA_DEB/DEBIAN/control" <<CONTROL
Package: tervia
Version: $TERVIA_DEB_VERSION
Section: devel
Priority: optional
Architecture: $TERVIA_DEB_ARCH
Maintainer: Tervia contributors
Homepage: https://github.com/murongg/Tervia
Depends: $TERVIA_DEPENDENCIES, git, libvulkan1, fonts-dejavu-core
Description: A native terminal workspace with Git and lightweight editing
CONTROL
dpkg-deb --root-owner-group --build "$TERVIA_DEB" "$TERVIA_ROOT/build/Tervia-linux-$TERVIA_ARCH.deb"

mkdir -p "$TERVIA_SCRATCH/rpm/"{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
tar -C "$TERVIA_DEB" -czf "$TERVIA_SCRATCH/rpm/SOURCES/tervia.tar.gz" usr
cat > "$TERVIA_SCRATCH/rpm/SPECS/tervia.spec" <<SPEC
Name: tervia
Version: $TERVIA_CORE
Release: $TERVIA_RPM_RELEASE
Summary: A native terminal workspace with Git and lightweight editing
License: MIT
URL: https://github.com/murongg/Tervia
Source0: tervia.tar.gz
BuildArch: $TERVIA_ARCH
Requires: git, vulkan-loader, dejavu-sans-mono-fonts

%description
A native terminal workspace with Git and lightweight editing.

%prep
%build
%install
mkdir -p "%{buildroot}"
tar -xzf "%{SOURCE0}" -C "%{buildroot}"

%files
%defattr(-,root,root,-)
%attr(0755,root,root) /usr/bin/tervia
/usr/share/applications/tervia.desktop
/usr/share/icons/hicolor/scalable/apps/tervia.svg
%license /usr/share/doc/tervia/LICENSE
%doc /usr/share/doc/tervia/ThirdPartyNotices.txt
SPEC
# Keep RPM's ELF dependency detection enabled; Ubuntu package names cannot describe RPM runtime requirements.
rpmbuild --define "_topdir $TERVIA_SCRATCH/rpm" --define '_build_id_links none' \
  --define 'debug_package %{nil}' -bb "$TERVIA_SCRATCH/rpm/SPECS/tervia.spec"
cp "$TERVIA_SCRATCH/rpm/RPMS/$TERVIA_ARCH/"*.rpm "$TERVIA_ROOT/build/Tervia-linux-$TERVIA_ARCH.rpm"
printf '%s\n' "$TERVIA_ROOT/build/Tervia-linux-$TERVIA_ARCH."{tar.gz,deb,rpm}
