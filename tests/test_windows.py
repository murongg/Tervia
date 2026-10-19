"""Exercise Windows packaging with a synthetic binary and mocked installer compiler."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(shutil.which("pwsh"), "Windows packaging requires PowerShell")
class WindowsPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="windows-package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "synthetic project"
        (self.root / "scripts").mkdir(parents=True)
        for name in ("windows.ps1", "windows.iss"):
            if (ROOT / "scripts" / name).exists():
                shutil.copyfile(ROOT / "scripts" / name, self.root / "scripts" / name)
        # Exercise Windows line endings even when the packaging test runs on another OS.
        (self.root / "LICENSE").write_bytes(b"synthetic project license\r\n")
        (self.root / "assets").mkdir()
        for name in ("tervia.ico", "lobe-license.txt", "hermes-license.txt", "material-license.txt"):
            (self.root / "assets" / name).write_text("synthetic asset\n")
        binary = self.root / "target/release/tervia.exe"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"synthetic executable")
        self.env = {**os.environ, "PACKAGE_ROOT": str(self.root)}
        (self.root / "harness.ps1").write_text(r'''
function cargo {
    $global:LASTEXITCODE = 0
    'path+file:///synthetic#tervia@1.2.3-beta.1'
}
function ISCC.exe {
    $args | Set-Content (Join-Path $env:PACKAGE_ROOT 'compiler.txt')
    $global:LASTEXITCODE = 0
    if ($env:PACKAGE_FAIL) { $global:LASTEXITCODE = 1; return }
    Set-Content (Join-Path $env:PACKAGE_ROOT 'build/Tervia-windows-x64-setup.exe') 'synthetic installer'
}
& (Join-Path $env:PACKAGE_ROOT 'scripts/windows.ps1') -NoBuild
''')

    def package(self):
        return subprocess.run(
            ["pwsh", "-NoProfile", "-File", str(self.root / "harness.ps1")],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )

    def test_creates_portable_zip_and_passes_full_and_numeric_versions_to_installer(self):
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "build/Tervia-windows-x64-setup.exe").is_file())
        with zipfile.ZipFile(self.root / "build/Tervia-windows-x64.zip") as archive:
            self.assertEqual(archive.read("Tervia.exe"), b"synthetic executable")
            self.assertEqual(archive.read("LICENSE"), (self.root / "LICENSE").read_bytes())
            self.assertIn("synthetic asset", archive.read("ThirdPartyNotices.txt").decode("utf-8-sig"))
        arguments = (self.root / "compiler.txt").read_text()
        self.assertIn("/DAppVersion=1.2.3-beta.1", arguments)
        self.assertIn("/DAppNumericVersion=1.2.3", arguments)

    def test_installer_compiler_failure_is_a_packaging_failure(self):
        self.env["PACKAGE_FAIL"] = "1"
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Installer build failed", result.stderr)
