import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RESOLVER = ROOT / "rapidroom/upstream-sync/resolve-generated.sh"
FILES = {
    "src-tauri/Cargo.lock": "lock",
    "src-tauri/Cargo.toml": "manifest",
    "src/example.txt": "code",
}


class ResolveGeneratedTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="upstream-sync-generated-test-")
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / "repo"
        self.repo.mkdir()
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        for path, marker in FILES.items():
            self.write(path, f"base {marker}\n")
        self.git("add", ".")
        self.git("commit", "-m", "base")
        self.git("checkout", "-b", "upstream")
        for path, marker in FILES.items():
            self.write(path, f"upstream {marker}\n")
        self.git("add", ".")
        self.git("commit", "-m", "upstream")
        self.git("checkout", "main")
        for path, marker in FILES.items():
            self.write(path, f"main {marker}\n")
        self.git("add", ".")
        self.git("commit", "-m", "main")
        self.bin = Path(self.temp.name) / "bin"
        self.bin.mkdir()
        self.calls = Path(self.temp.name) / "calls.txt"

    def git(self, *args, check=True):
        return subprocess.run(
            ["git", *args], cwd=self.repo, check=check, capture_output=True, text=True
        ).stdout.strip()

    def write(self, path, content):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)

    def fake_cargo(self, body):
        target = self.bin / "cargo"
        target.write_text(f"#!/bin/sh\necho 'cargo '\"$*\" >> \"$CALLS\"\n{body}\n")
        target.chmod(0o755)

    def merge(self, mixed=False):
        if not mixed:
            self.write("src/example.txt", "upstream code\n")
            self.write("src-tauri/Cargo.toml", "upstream manifest\n")
            self.git("add", "src/example.txt", "src-tauri/Cargo.toml")
            self.git("commit", "-m", "keep code and manifest out of conflict")
        self.git("merge", "upstream", check=False)

    def resolve(self, *, abort_on_failure=False):
        conflicts = Path(self.temp.name) / "conflicts.txt"
        conflicts.write_text(self.git("diff", "--name-only", "--diff-filter=U") + "\n")
        manifest = self.repo / "src-tauri/Cargo.toml"
        env = {
            **os.environ,
            "PATH": str(self.bin) + ":" + os.environ["PATH"],
            "CALLS": str(self.calls),
            "RUNNER_TEMP": self.temp.name,
            "EXPECTED_MANIFEST": str(manifest),
        }
        command = 'bash "$1" "$2" 1.98 "$3"'
        if abort_on_failure:
            command = f'if {command}; then exit 0; else git merge --abort; exit 1; fi'
        return subprocess.run(
            ["bash", "-c", command, "test", str(RESOLVER), str(conflicts), str(manifest)],
            cwd=self.repo,
            env=env,
            capture_output=True,
            text=True,
            timeout=20,
        )

    def test_cargo_lock_only_conflict_is_resolved_and_committed(self):
        self.merge()
        self.fake_cargo(
            "if [ -n \"${GH_TOKEN:-}\" ]; then echo 'token visible' >&2; exit 90; fi\n"
            "if [ \"$PWD\" != \"$RUNNER_TEMP\" ]; then exit 91; fi\n"
            "if [ \"$*\" != \"+1.98 update -w --manifest-path $EXPECTED_MANIFEST\" ]; then exit 92; fi\n"
            "starting=$(cat \"$(dirname \"$EXPECTED_MANIFEST\")/Cargo.lock\")\n"
            "echo \"starting=$starting\" >> \"$CALLS\"\n"
            "if [ \"$starting\" != 'upstream lock' ]; then exit 93; fi\n"
            "printf 'resolved lock\\n' > \"$(dirname \"$EXPECTED_MANIFEST\")/Cargo.lock\""
        )

        result = self.resolve()

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.git("diff", "--name-only", "--diff-filter=U"), "")
        self.assertEqual(len(self.git("rev-list", "--parents", "-n", "1", "HEAD").split()), 3)
        self.assertEqual((self.repo / "src-tauri/Cargo.lock").read_text(), "resolved lock\n")
        self.assertNotIn("# Conflicts:", self.git("log", "-1", "--format=%B"))
        manifest = self.repo / "src-tauri/Cargo.toml"
        self.assertEqual(
            self.calls.read_text(),
            f"cargo +1.98 update -w --manifest-path {manifest}\nstarting=upstream lock\n",
        )

    def test_mixed_conflict_falls_back_without_modifying_merge(self):
        self.merge(mixed=True)
        before_status = self.git("status", "--porcelain")
        before_head = self.git("rev-parse", "HEAD")

        result = self.resolve()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("limited to a lone src-tauri/Cargo.lock conflict", result.stdout)
        self.assertEqual(self.git("status", "--porcelain"), before_status)
        self.assertEqual(self.git("rev-parse", "HEAD"), before_head)
        self.assertTrue((self.repo / ".git/MERGE_HEAD").exists())
        self.assertIn("src/example.txt", self.git("diff", "--name-only", "--diff-filter=U"))
        self.assertFalse(self.calls.exists())

    def test_cargo_failure_is_aborted_cleanly_by_the_workflow_path(self):
        self.merge()
        before_head = self.git("rev-parse", "HEAD")
        self.fake_cargo(
            "printf 'partial lock\\n' > \"$(dirname \"$EXPECTED_MANIFEST\")/Cargo.lock\"\n"
            "echo 'simulated cargo failure' >&2\n"
            "exit 42"
        )

        result = self.resolve(abort_on_failure=True)

        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.repo / ".git/MERGE_HEAD").exists())
        self.assertEqual(self.git("rev-parse", "HEAD"), before_head)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual((self.repo / "src-tauri/Cargo.lock").read_text(), "main lock\n")
        self.assertIn("simulated cargo failure", result.stderr)


if __name__ == "__main__":
    unittest.main()
