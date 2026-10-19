"""Exercise distribution staging with synthetic executables and mocked OS tools."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
TOOL = r'''
import json
import os
from pathlib import Path
import shutil
import sys

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["PACKAGE_CALLS"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")
if tool == os.environ.get("PACKAGE_FAIL"):
    sys.exit(1)
if tool == "uname":
    print(os.environ["PACKAGE_OS"] if args == ["-s"] else os.environ.get("PACKAGE_ARCH", "x86_64"))
elif tool == "cargo":
    if args[0] == "pkgid":
        print("path+file:///synthetic#tervia@" + os.environ.get("PACKAGE_VERSION", "1.2.3-beta.1"))
elif tool == "dpkg-shlibdeps":
    print("shlibs:Depends=libc6 (>= 2.38), libfontconfig1 (>= 2.13)")
elif tool == "dpkg-deb":
    Path(args[-1]).write_bytes(b"synthetic deb")
elif tool == "du":
    root = Path(args[-1])
    size = sum(p.stat().st_size for p in root.rglob("*") if p.is_file() and not p.is_symlink())
    print(f"{(size + 1023) // 1024}\t{root}")
elif tool == "rpmbuild":
    definitions = [args[i + 1] for i, arg in enumerate(args) if arg == "--define"]
    top = Path(next(value.split(" ", 1)[1] for value in definitions if value.startswith("_topdir ")))
    spec = Path(args[-1]).read_text()
    Path(os.environ["PACKAGE_SPEC"]).write_text(spec)
    artifact = top / "RPMS" / os.environ.get("PACKAGE_ARCH", "x86_64") / "synthetic.rpm"
    artifact.parent.mkdir(parents=True, exist_ok=True)
    artifact.write_bytes(b"synthetic rpm")
elif tool in ("ditto", "hdiutil"):
    if tool == "ditto" and args[0] != "-c":
        shutil.copytree(args[0], args[1])
        sys.exit(0)
    if tool == "hdiutil":
        stage = Path(args[args.index("-srcfolder") + 1])
        assert (stage / "Applications").is_symlink()
        assert (stage / "Tervia.app/Contents/MacOS/Tervia").read_bytes() == b"synthetic signed executable"
    Path(args[-1]).write_bytes(b"synthetic distribution")
'''


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="distribution-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "synthetic project"
        (self.root / "scripts").mkdir(parents=True)
        for name in ("linux.sh", "macos.sh"):
            if (ROOT / "scripts" / name).exists():
                shutil.copyfile(ROOT / "scripts" / name, self.root / "scripts" / name)
        (self.root / "LICENSE").write_text("synthetic project license\n")
        (self.root / "assets").mkdir()
        for name in ("tervia.svg", "lobe-license.txt", "hermes-license.txt", "material-license.txt"):
            (self.root / "assets" / name).write_text("synthetic asset\n")
        binary = self.root / "target/release/tervia"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"synthetic executable")
        binary.chmod(0o755)
        self.tools = self.root / "tools"
        self.tools.mkdir()
        for name in ("uname", "cargo", "dpkg-shlibdeps", "dpkg-deb", "rpmbuild", "ditto", "hdiutil", "du"):
            path = self.tools / name
            path.write_text(f"#!{sys.executable}\n" + TOOL)
            path.chmod(0o755)
        self.calls = self.root / "calls.jsonl"
        self.spec = self.root / "recorded.spec"
        self.env = {
            **os.environ,
            "PATH": f"{self.tools}{os.pathsep}{os.environ['PATH']}",
            "PACKAGE_CALLS": str(self.calls), "PACKAGE_SPEC": str(self.spec),
            "PACKAGE_OS": "Linux",
        }

    def package(self, script, *args):
        return subprocess.run(
            ["bash", str(self.root / "scripts" / script), *args],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )

    def recorded(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def test_linux_builds_archive_deb_and_rpm_with_desktop_integration_and_dependencies(self):
        result = self.package("linux.sh", "--no-build")
        self.assertEqual(result.returncode, 0, result.stderr)
        for suffix in ("tar.gz", "deb", "rpm"):
            self.assertTrue((self.root / f"build/Tervia-linux-x86_64.{suffix}").is_file())
        with tarfile.open(self.root / "build/Tervia-linux-x86_64.tar.gz") as archive:
            self.assertEqual(archive.extractfile("Tervia/tervia").read(), b"synthetic executable")
            self.assertEqual(archive.extractfile("Tervia/LICENSE").read(), b"synthetic project license\n")
        stage = self.root / "build/linux/deb"
        self.assertTrue((stage / "usr/bin/tervia").stat().st_mode & 0o111)
        self.assertTrue((stage / "usr/share/icons/hicolor/scalable/apps/tervia.svg").is_file())
        desktop = (stage / "usr/share/applications/tervia.desktop").read_text()
        self.assertIn("Exec=tervia", desktop)
        self.assertIn("Icon=tervia", desktop)
        self.assertIn("synthetic asset", (stage / "usr/share/doc/tervia/ThirdPartyNotices.txt").read_text())
        self.assertEqual((stage / "usr/share/doc/tervia/LICENSE").read_text(), "synthetic project license\n")
        control = (stage / "DEBIAN/control").read_text()
        self.assertIn("Version: 1.2.3~beta.1", control)
        self.assertIn("Architecture: amd64", control)
        self.assertIn("libc6 (>= 2.38)", control)
        self.assertIn("git", control)
        self.assertIn("Release: 0.beta.1", self.spec.read_text())
        self.assertIn("License: AGPL-3.0-only\n", self.spec.read_text())
        self.assertIn("%license /usr/share/doc/tervia/LICENSE\n", self.spec.read_text())
        self.assertIn(["dpkg-deb", "--root-owner-group", "--build", str(stage), str(self.root / "build/Tervia-linux-x86_64.deb")], self.recorded())
        self.assertFalse(any(call[:2] == ["cargo", "build"] for call in self.recorded()))

    def test_linux_arm64_packages_use_native_architecture_and_stable_versions(self):
        self.env.update(PACKAGE_ARCH="aarch64", PACKAGE_VERSION="1.2.3+synthetic.7")
        result = self.package("linux.sh", "--no-build")
        self.assertEqual(result.returncode, 0, result.stderr)
        control = (self.root / "build/linux/deb/DEBIAN/control").read_text()
        self.assertIn("Architecture: arm64", control)
        self.assertIn("Version: 1.2.3\n", control)
        self.assertIn("Release: 1\n", self.spec.read_text())
        self.assertTrue((self.root / "build/Tervia-linux-aarch64.rpm").is_file())

    def test_linux_dependency_scan_failure_stops_before_deb_is_created(self):
        self.env["PACKAGE_FAIL"] = "dpkg-shlibdeps"
        result = self.package("linux.sh", "--no-build")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "build/Tervia-linux-x86_64.deb").exists())

    def test_macos_archives_existing_signed_app_and_dmg_contains_applications_link(self):
        self.env.update(PACKAGE_OS="Darwin", PACKAGE_ARCH="arm64")
        binary = self.root / "build/Tervia.app/Contents/MacOS/Tervia"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"synthetic signed executable")
        result = self.package("macos.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        for suffix in ("zip", "dmg"):
            self.assertTrue((self.root / f"build/Tervia-macos-arm64.{suffix}").is_file())
        self.assertEqual(binary.read_bytes(), b"synthetic signed executable")
        self.assertFalse(any(call[0] == "cargo" for call in self.recorded()))

    @unittest.skipUnless(sys.platform == "darwin", "DMG integration requires macOS")
    def test_real_dmg_fits_a_large_sparse_app_and_preserves_its_contents(self):
        self.env.update(PACKAGE_OS="Darwin", PACKAGE_ARCH="arm64")
        for name in ("ditto", "hdiutil", "du"):
            (self.tools / name).unlink()
        binary = self.root / "build/Tervia.app/Contents/MacOS/Tervia"
        binary.parent.mkdir(parents=True)
        with binary.open("wb") as output:
            output.write(b"synthetic signed executable")
            output.truncate(128 * 1024 * 1024)
        result = self.package("macos.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        mount = Path(self.temporary.name) / "mounted"
        subprocess.run(
            ["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount),
             str(self.root / "build/Tervia-macos-arm64.dmg")],
            check=True, capture_output=True,
        )
        try:
            mounted_binary = mount / "Tervia.app/Contents/MacOS/Tervia"
            self.assertEqual(mounted_binary.stat().st_size, binary.stat().st_size)
            with binary.open("rb") as original, mounted_binary.open("rb") as mounted:
                self.assertEqual(hashlib.file_digest(original, "sha256").digest(), hashlib.file_digest(mounted, "sha256").digest())
            self.assertEqual(os.readlink(mount / "Applications"), "/Applications")
        finally:
            subprocess.run(["hdiutil", "detach", str(mount)], check=True, capture_output=True)
