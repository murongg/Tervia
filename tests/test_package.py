"""Exercise macOS packaging with synthetic binaries and mocked build tools."""

import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "package.sh"
TOOL = r'''
import json
import os
from pathlib import Path
import sys

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["PACKAGE_CALLS"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")
if tool == "cargo":
    if args[0] == "pkgid":
        print("path+file:///synthetic#tervia@1.2.3-beta.1")
    elif args[0] == "build":
        profile = "release" if "--release" in args else "debug"
        binary = Path("target") / profile / "tervia"
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_bytes(f"synthetic {profile} executable".encode())
elif tool == "swift":
    Path(args[-1]).write_bytes(b"synthetic icon")
elif tool in ("sips", "iconutil"):
    option = "--out" if tool == "sips" else "-o"
    Path(args[args.index(option) + 1]).write_bytes(b"synthetic icon")
'''


@unittest.skipUnless(shutil.which("zsh"), "macOS packaging requires zsh")
class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "synthetic project"
        (self.root / "scripts").mkdir(parents=True)
        shutil.copyfile(SCRIPT, self.root / "scripts/package.sh")
        (self.root / "LICENSE").write_text("synthetic project license\n")
        (self.root / "assets").mkdir()
        for name in ("lobe-license.txt", "hermes-license.txt"):
            (self.root / "assets" / name).write_text("synthetic license\n")
        (self.root / "assets/material-license.txt").write_text("synthetic material license\n")
        self.tools = self.root / "tools"
        self.tools.mkdir()
        for name in ("cargo", "swift", "sips", "iconutil", "codesign"):
            path = self.tools / name
            path.write_text(f"#!{sys.executable}\n" + TOOL)
            path.chmod(0o755)
        self.calls = self.root / "calls.jsonl"
        self.env = {
            **os.environ,
            "PATH": f"{self.tools}{os.pathsep}{os.environ['PATH']}",
            "PACKAGE_CALLS": str(self.calls),
        }

    def package(self, *arguments):
        return subprocess.run(
            ["zsh", str(self.root / "scripts/package.sh"), *arguments],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )

    def test_default_build_packages_a_release_executable_that_supports_updates(self):
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stderr)
        binary = self.root / "build/Tervia.app/Contents/MacOS/Tervia"
        self.assertEqual(binary.read_bytes(), b"synthetic release executable")
        notices = self.root / "build/Tervia.app/Contents/Resources/ThirdPartyNotices.txt"
        self.assertIn("synthetic material license", notices.read_text())
        license_file = self.root / "build/Tervia.app/Contents/Resources/LICENSE"
        self.assertEqual(license_file.read_text(), "synthetic project license\n")
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertIn(["cargo", "build", "--release", "--features", "native"], calls)
        with (self.root / "build/Tervia.app/Contents/Info.plist").open("rb") as file:
            self.assertEqual(plistlib.load(file)["CFBundleShortVersionString"], "1.2.3")

    def test_no_build_uses_the_release_binary_even_when_a_debug_binary_exists(self):
        for profile in ("debug", "release"):
            binary = self.root / "target" / profile / "tervia"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(f"synthetic {profile} executable".encode())
        result = self.package("--no-build")
        self.assertEqual(result.returncode, 0, result.stderr)
        binary = self.root / "build/Tervia.app/Contents/MacOS/Tervia"
        self.assertEqual(binary.read_bytes(), b"synthetic release executable")
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertFalse(any(call[:2] == ["cargo", "build"] for call in calls))

    def test_no_build_without_a_release_binary_fails_instead_of_packaging_debug(self):
        binary = self.root / "target/debug/tervia"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"synthetic debug executable")
        result = self.package("--no-build")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "build/Tervia.app/Contents/MacOS/Tervia").exists())
