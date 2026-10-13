"""Release integration tests using synthetic repositories and local bare remotes."""

import os
from pathlib import Path
import shutil
import shlex
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "release.sh"


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "project"
        self.root.mkdir()
        self.env = {
            **os.environ,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_AUTHOR_NAME": "Release Test",
            "GIT_AUTHOR_EMAIL": "release@example.test",
            "GIT_COMMITTER_NAME": "Release Test",
            "GIT_COMMITTER_EMAIL": "release@example.test",
        }
        self.git("init", "--initial-branch=main")
        (self.root / "scripts").mkdir()
        if SCRIPT.exists():
            shutil.copyfile(SCRIPT, self.root / "scripts/release.sh")
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text("pub fn synthetic() {}\n", encoding="utf-8")
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "synthetic"\nversion = "0.1.0"\n', encoding="utf-8"
        )
        (self.root / "Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "synthetic"\nversion = "0.1.0"\n',
            encoding="utf-8",
        )
        self.commit("chore: initialize synthetic project")

    def git(self, *arguments, cwd=None):
        return subprocess.run(
            ["git", *arguments], cwd=cwd or self.root, env=self.env,
            capture_output=True, text=True, encoding="utf-8", check=True,
        ).stdout.strip()

    def commit(self, message):
        self.git("add", "--all")
        self.git("commit", "--allow-empty", "-m", message)

    def release(self, *arguments):
        return subprocess.run(
            ["sh", str(self.root / "scripts/release.sh"), *arguments],
            cwd=self.root, env=self.env, capture_output=True, text=True, encoding="utf-8",
        )

    def remote(self):
        remote = Path(self.temporary.name) / "remote.git"
        self.git("init", "--bare", str(remote))
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "origin", "main")
        return remote

    def assert_rejected_without_changes(self, phrase, *arguments):
        before = (self.git("rev-parse", "HEAD"), self.git("status", "--porcelain"), self.git("tag"))
        result = self.release(*arguments)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(phrase, result.stderr)
        self.assertEqual(before, (self.git("rev-parse", "HEAD"), self.git("status", "--porcelain"), self.git("tag")))

    def test_first_release_commits_changelog_and_annotated_tag(self):
        self.commit("feat(editor): add synthetic search")
        self.commit("fix: handle synthetic empty input")
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        changelog = (self.root / "CHANGELOG.md").read_text(encoding="utf-8")
        self.assertIn("## v0.1.0", changelog)
        self.assertIn("### Features", changelog)
        self.assertIn("### Fixes", changelog)
        self.assertIn("add synthetic search", changelog)
        self.assertEqual(self.git("cat-file", "-t", "v0.1.0"), "tag")
        self.assertEqual(self.git("show", "v0.1.0:CHANGELOG.md"), changelog.strip())
        annotation = self.git("for-each-ref", "--format=%(contents)", "refs/tags/v0.1.0")
        self.assertEqual(annotation, changelog.removeprefix("# Changelog\n\n").strip())
        self.assertEqual(self.git("log", "-1", "--format=%s"), "chore(release): v0.1.0")
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_incremental_release_preserves_history_and_marks_breaking_changes(self):
        old = "## v0.0.1 (2025-01-01)\n\n- synthetic earlier change\n"
        (self.root / "CHANGELOG.md").write_text("# Changelog\n\n" + old, encoding="utf-8")
        self.commit("chore(release): v0.0.1")
        self.git("tag", "-a", "v0.0.1", "-m", "Synthetic earlier release")
        self.commit("feat(api)!: replace synthetic format")
        self.commit("fix: reject synthetic input\n\nBREAKING CHANGE: synthetic migration required")
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        changelog = (self.root / "CHANGELOG.md").read_text(encoding="utf-8")
        current = changelog.split(old)[0]
        self.assertIn("### Breaking changes", current)
        self.assertIn("synthetic migration required", current)
        self.assertNotIn("initialize synthetic project", current)
        self.assertTrue(changelog.endswith(old))

    def test_dry_run_changes_no_files_commits_or_tags(self):
        before = self.git("rev-parse", "HEAD")
        result = self.release("--dry-run")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("## v0.1.0", result.stdout)
        self.assertFalse((self.root / "CHANGELOG.md").exists())
        self.assertEqual(self.git("rev-parse", "HEAD"), before)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.git("tag"), "")

    def test_untracked_changes_are_rejected(self):
        (self.root / "draft.txt").write_text("synthetic draft", encoding="utf-8")
        self.assert_rejected_without_changes("clean working tree")

    def test_staged_changes_are_rejected(self):
        (self.root / "draft.txt").write_text("synthetic draft", encoding="utf-8")
        self.git("add", "draft.txt")
        self.assert_rejected_without_changes("clean working tree")

    def test_duplicate_tag_is_rejected(self):
        self.git("tag", "v0.1.0")
        self.assert_rejected_without_changes("already exists")

    def test_prerelease_uses_exact_cargo_version(self):
        for name in ("Cargo.toml", "Cargo.lock"):
            path = self.root / name
            path.write_text(path.read_text(encoding="utf-8").replace("0.1.0", "0.2.0-beta.1"), encoding="utf-8")
        self.commit("build: prepare synthetic prerelease")
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git("tag"), "v0.2.0-beta.1")

    def test_remote_duplicate_tag_is_detected_before_release_commit(self):
        remote = self.remote()
        self.git("tag", "v0.1.0", "main", cwd=remote)
        before = self.git("rev-parse", "HEAD")
        result = self.release("--push")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already exists", result.stderr)
        self.assertEqual(self.git("rev-parse", "HEAD"), before)
        self.assertFalse((self.root / "CHANGELOG.md").exists())

    def test_shallow_history_is_rejected(self):
        clone = Path(self.temporary.name) / "shallow"
        self.git("clone", "--depth=1", self.root.as_uri(), str(clone))
        self.root = clone
        self.assert_rejected_without_changes("full history")

    def test_unchanged_release_is_rejected(self):
        self.git("tag", "v0.0.1")
        self.assert_rejected_without_changes("No commits")

    def test_stale_lockfile_is_rejected(self):
        lock = self.root / "Cargo.lock"
        lock.write_text(lock.read_text(encoding="utf-8").replace("0.1.0", "0.0.1"), encoding="utf-8")
        self.commit("build: change synthetic lockfile")
        self.assert_rejected_without_changes("Cargo.lock")

    def test_detached_head_is_rejected(self):
        self.git("switch", "--detach")
        self.assert_rejected_without_changes("branch")

    def test_commit_hook_failure_preserves_changelog_without_creating_tag(self):
        hook = self.root / ".git/hooks/pre-commit"
        hook.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
        hook.chmod(0o755)
        before = self.git("rev-parse", "HEAD")
        result = self.release()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.git("tag"), "")
        self.assertEqual(self.git("rev-parse", "HEAD"), before)
        self.assertTrue((self.root / "CHANGELOG.md").exists())

    def test_push_publishes_branch_and_tag_to_local_remote(self):
        remote = self.remote()
        result = self.release("--push")
        self.assertEqual(result.returncode, 0, result.stderr)
        tip = self.git("rev-parse", "HEAD")
        self.assertEqual(self.git("rev-parse", "main", cwd=remote), tip)
        self.assertEqual(self.git("rev-parse", "v0.1.0^{commit}", cwd=remote), tip)

    def test_default_does_not_push(self):
        remote = self.remote()
        before = self.git("rev-parse", "main", cwd=remote)
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git("rev-parse", "main", cwd=remote), before)
        self.assertEqual(self.git("tag", cwd=remote), "")

    def test_shell_metacharacters_in_commit_messages_are_literal(self):
        subject = "keep $(touch injected) and `touch injected` literal"
        self.commit("fix: " + subject)
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(subject, (self.root / "CHANGELOG.md").read_text(encoding="utf-8"))
        self.assertFalse((self.root / "injected").exists())

    def test_printed_push_command_quotes_branch_name(self):
        branch = "feature/synthetic'quote"
        self.git("switch", "-c", branch)
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        command = next(line for line in result.stdout.splitlines() if line.startswith("git push --atomic"))
        self.assertEqual(shlex.split(command), [
            "git", "push", "--atomic", "origin", f"HEAD:refs/heads/{branch}", "refs/tags/v0.1.0",
        ])

    def test_atomic_push_failure_leaves_remote_unchanged(self):
        remote = self.remote()
        before = self.git("rev-parse", "main", cwd=remote)
        hook = remote / "hooks/update"
        hook.write_text('#!/bin/sh\ncase "$1" in refs/tags/*) exit 1;; esac\n', encoding="utf-8")
        hook.chmod(0o755)
        result = self.release("--push")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.git("rev-parse", "main", cwd=remote), before)
        self.assertEqual(self.git("tag", cwd=remote), "")
        self.assertEqual(self.git("tag"), "v0.1.0")
        self.assertIn("git push --atomic", result.stderr)


if __name__ == "__main__":
    unittest.main()
