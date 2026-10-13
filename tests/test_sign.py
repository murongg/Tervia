"""Exercise signing orchestration with synthetic credentials and mocked Apple tools."""

import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/sign.sh"
TOOL = r'''
import json
import os
from pathlib import Path
import sys

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["SIGN_CALLS"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")
failure = os.environ.get("SIGN_FAIL", "")
if " ".join([tool, *args]).startswith(failure) and failure:
    sys.exit(1)
if tool == "uname":
    print("Darwin")
elif tool == "security":
    if args[0] == "create-keychain":
        Path(args[-1]).write_text("synthetic keychain")
    elif args[0] == "delete-keychain":
        Path(args[-1]).unlink(missing_ok=True)
    elif args[0] == "find-identity":
        print(os.environ.get("SIGN_IDENTITIES", '1) ' + 'A' * 40 + ' "Developer ID Application: Synthetic (TESTTEAM01)"'))
elif tool == "ditto":
    Path(args[-1]).write_bytes(b"synthetic archive")
elif tool == "xcrun" and args[:2] == ["notarytool", "submit"]:
    print(json.dumps({"id": "synthetic-submission", "status": os.environ.get("SIGN_STATUS", "Accepted")}))
elif tool == "xcrun" and args[:2] == ["notarytool", "log"]:
    print(json.dumps({"issues": [{"message": "synthetic notarization rejection"}]}))
elif tool == "plutil":
    try:
        print(json.loads(Path(args[-1]).read_text())[args[1]])
    except (ValueError, KeyError):
        sys.exit(1)
'''


class SigningTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sign-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.tools = self.root / "bin"
        self.tools.mkdir()
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.app = self.root / "Synthetic App.app"
        (self.app / "Contents/MacOS").mkdir(parents=True)
        (self.app / "Contents/MacOS/Tervia").write_bytes(b"synthetic executable")
        (self.app / "Contents/Info.plist").write_text("synthetic plist")
        self.calls = self.root / "calls.jsonl"
        for name in ("uname", "security", "codesign", "ditto", "xcrun", "plutil", "spctl"):
            path = self.tools / name
            path.write_text(f"#!{sys.executable}\n" + TOOL)
            path.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{self.tools}{os.pathsep}{os.environ['PATH']}",
            "RUNNER_TEMP": str(self.scratch),
            "SIGN_CALLS": str(self.calls),
            "MACOS_CERTIFICATE": base64.b64encode(b"synthetic certificate").decode(),
            "MACOS_CERTIFICATE_PASSWORD": "synthetic-p12-password",
            "APPLE_ID": "release@example.test",
            "APPLE_TEAM_ID": "TESTTEAM01",
            "APPLE_APP_SPECIFIC_PASSWORD": "synthetic-notary-password",
        }

    def run_sign(self, *arguments):
        return subprocess.run(
            ["sh", str(SCRIPT), *(arguments or [str(self.app)])], cwd=self.root,
            env=self.env, capture_output=True, text=True,
        )

    def recorded(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []

    def assert_cleaned(self):
        self.assertEqual(list(self.scratch.iterdir()), [])
        self.assertTrue(any(call[:2] == ["security", "delete-keychain"] for call in self.recorded()))

    def test_preflight_requires_all_secrets_without_using_apple_tools(self):
        for name in ("MACOS_CERTIFICATE", "MACOS_CERTIFICATE_PASSWORD", "APPLE_ID", "APPLE_TEAM_ID", "APPLE_APP_SPECIFIC_PASSWORD"):
            with self.subTest(name=name):
                value = self.env.pop(name)
                result = self.run_sign("--check")
                self.env[name] = value
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(name, result.stderr)
        self.assertEqual(self.recorded(), [])

    def test_preflight_with_secrets_does_not_import_or_sign(self):
        result = self.run_sign("--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.recorded(), [])

    def test_shell_tracing_does_not_print_credentials(self):
        result = subprocess.run(
            ["sh", "-x", str(SCRIPT), str(self.app)], cwd=self.root,
            env=self.env, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in ("MACOS_CERTIFICATE", "MACOS_CERTIFICATE_PASSWORD", "APPLE_APP_SPECIFIC_PASSWORD"):
            self.assertNotIn(self.env[name], result.stdout + result.stderr)
        self.assert_cleaned()

    def test_invalid_base64_is_rejected_before_import(self):
        self.env["MACOS_CERTIFICATE"] = "invalid!base64"
        result = self.run_sign()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == "security" for call in self.recorded()))
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_cleanup_failure_is_not_reported_as_a_successful_release(self):
        self.env["SIGN_FAIL"] = "security delete-keychain"
        result = self.run_sign()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("temporary signing keychain", result.stderr)
        self.assert_cleaned()

    def test_accepted_submission_signs_staples_verifies_and_cleans_up(self):
        result = self.run_sign()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.recorded()
        sign = next(call for call in calls if call[:2] == ["codesign", "--force"])
        self.assertIn("--timestamp", sign)
        self.assertEqual(sign[sign.index("--options") + 1], "runtime")
        self.assertEqual(sign[sign.index("--sign") + 1], "A" * 40)
        self.assertNotIn("--deep", sign)
        submit = next(call for call in calls if call[:3] == ["xcrun", "notarytool", "submit"])
        staple = ["xcrun", "stapler", "staple", str(self.app)]
        self.assertLess(calls.index(sign), calls.index(submit))
        self.assertLess(calls.index(submit), calls.index(staple))
        self.assertIn(["xcrun", "stapler", "validate", str(self.app)], calls)
        self.assertTrue(any(call[0] == "spctl" for call in calls))
        self.assert_cleaned()
        for name in ("MACOS_CERTIFICATE", "MACOS_CERTIFICATE_PASSWORD", "APPLE_APP_SPECIFIC_PASSWORD"):
            self.assertNotIn(self.env[name], result.stdout + result.stderr)

    def test_wrong_team_certificate_cannot_sign(self):
        self.env["SIGN_IDENTITIES"] = '1) ' + 'A' * 40 + ' "Developer ID Application: Synthetic (OTHERTEAM1)"'
        result = self.run_sign()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == "codesign" for call in self.recorded()))
        self.assert_cleaned()

    def test_development_certificate_cannot_sign(self):
        self.env["SIGN_IDENTITIES"] = '1) ' + 'A' * 40 + ' "Apple Development: Synthetic (TESTTEAM01)"'
        result = self.run_sign()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == "codesign" for call in self.recorded()))
        self.assert_cleaned()

    def test_ambiguous_certificates_are_rejected(self):
        self.env["SIGN_IDENTITIES"] = '\n'.join(
            f'{index}) {digit * 40} "Developer ID Application: Synthetic (TESTTEAM01)"'
            for index, digit in enumerate(("A", "B"), 1)
        )
        result = self.run_sign()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == "codesign" for call in self.recorded()))
        self.assert_cleaned()

    def test_notary_rejection_or_timeout_never_staples(self):
        for status in ("Invalid", "In Progress", ""):
            with self.subTest(status=status):
                self.env["SIGN_STATUS"] = status
                self.calls.unlink(missing_ok=True)
                result = self.run_sign()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(call[:2] == ["xcrun", "stapler"] for call in self.recorded()))
                self.assertTrue(any(call[:3] == ["xcrun", "notarytool", "log"] for call in self.recorded()))
                self.assert_cleaned()

    def test_tool_failures_abort_and_remove_credentials(self):
        for command in (
            "security import", "codesign --force", "codesign --verify", "ditto",
            "xcrun notarytool store-credentials", "xcrun notarytool submit",
            "xcrun stapler staple", "xcrun stapler validate", "spctl",
        ):
            with self.subTest(command=command):
                self.env["SIGN_FAIL"] = command
                self.calls.unlink(missing_ok=True)
                result = self.run_sign()
                self.assertNotEqual(result.returncode, 0)
                self.assert_cleaned()


if __name__ == "__main__":
    unittest.main()
