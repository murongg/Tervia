"""Exercise release asset collection with synthetic distribution files."""

import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/assets.sh"
ASSETS = (
    "macos-arm64.zip", "macos-arm64.dmg", "linux-x86_64.tar.gz",
    "linux-x86_64.deb", "linux-x86_64.rpm", "windows-x64.zip",
    "windows-x64-setup.exe",
)


class AssetTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="assets-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for asset in ASSETS:
            (self.root / f"Tervia-{asset}").write_bytes(f"synthetic {asset}".encode())

    def prepare(self):
        return subprocess.run(
            ["bash", str(SCRIPT), "v1.2.3-beta.1"], cwd=self.root,
            capture_output=True, text=True,
        )

    def test_versions_every_format_and_checksums_the_exact_uploaded_bytes(self):
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = {f"Tervia-v1.2.3-beta.1-{asset}" for asset in ASSETS}
        sums = {}
        for line in (self.root / "SHA256SUMS").read_text().splitlines():
            digest, name = line.split("  ")
            sums[name] = digest
        self.assertEqual(set(sums), expected)
        for name, digest in sums.items():
            self.assertEqual(hashlib.sha256((self.root / name).read_bytes()).hexdigest(), digest)
        self.assertEqual({path.name for path in self.root.iterdir()}, expected | {"SHA256SUMS"})

    def test_missing_or_duplicate_format_fails_before_renaming_any_assets(self):
        for change in ("missing", "duplicate", "unexpected"):
            with self.subTest(change=change):
                path = self.root / "Tervia-linux-x86_64.deb"
                if change == "missing":
                    path.unlink()

                else:
                    path = self.root / ("Tervia-macos-x86_64.dmg" if change == "duplicate" else "Tervia-extra.zip")
                    path.write_bytes(b"synthetic unexpected package")
                before = {entry.name for entry in self.root.iterdir()}
                result = self.prepare()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual({entry.name for entry in self.root.iterdir()}, before)
                if change == "missing":
                    path.write_bytes(b"synthetic deb")
                else:
                    path.unlink()

    def test_mismatched_architectures_are_rejected_before_renaming(self):
        (self.root / "Tervia-macos-arm64.dmg").rename(self.root / "Tervia-macos-x86_64.dmg")
        before = {entry.name for entry in self.root.iterdir()}
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual({entry.name for entry in self.root.iterdir()}, before)
