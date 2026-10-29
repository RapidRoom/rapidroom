import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RESOLVER = ROOT / "rapidroom/upstream-sync/resolve-generated.sh"
GENERATED = {
    "src-tauri/Cargo.lock": "lock",
    "rapidroom/adjustment-schema.json": "schema",
    "rapidroom/plugin/skills/rapidroom/references/adjustments.md": "reference",
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
        for path, marker in {**GENERATED, "src-tauri/Cargo.toml": "manifest", "src/example.txt": "code"}.items():
            self.write(path, f"base {marker}\n")
        self.git("add", ".")
        self.git("commit", "-m", "base")
        self.git("checkout", "-b", "upstream")
        for path, marker in {**GENERATED, "src/example.txt": "code"}.items():
            self.write(path, f"upstream {marker}\n")
        self.git("add", ".")
        self.git("commit", "-m", "upstream")
        self.git("checkout", "main")
        for path, marker in {**GENERATED, "src/example.txt": "code"}.items():
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

    def fake_command(self, name, body):
        target = self.bin / name
        target.write_text(f"#!/bin/sh\necho '{name} '\"$*\" >> \"$CALLS\"\n{body}\n")
        target.chmod(0o755)

    def merge(self, mixed=False):
        if not mixed:
            self.write("src/example.txt", "upstream code\n")
            self.git("add", "src/example.txt")
            self.git("commit", "-m", "keep code out of conflict")
        self.git("merge", "upstream", check=False)

    def resolve(self):
        conflicts = Path(self.temp.name) / "conflicts.txt"
        conflicts.write_text(self.git("diff", "--name-only", "--diff-filter=U") + "\n")
        env = {
            **os.environ,
            "PATH": str(self.bin) + ":" + os.environ["PATH"],
            "CALLS": str(self.calls),
            "FAKE_CARGO_OFFLINE_FAIL": "1",
        }
        return subprocess.run(
            ["bash", str(RESOLVER), str(conflicts)],
            cwd=self.repo,
            env=env,
            capture_output=True,
            text=True,
            timeout=20,
        )

    def test_generated_only_conflict_is_resolved_and_committed(self):
        self.merge()
        self.fake_command(
            "cargo",
            "if [ \"$FAKE_CARGO_OFFLINE_FAIL\" = 1 ] && [ \"$1\" = update ] && [ \"$5\" = --offline ]; then exit 1; fi\nprintf 'resolved lock\\n' > src-tauri/Cargo.lock",
        )
        self.fake_command("npm", "exit 0")
        self.fake_command("node", "printf 'generated schema\\n' > rapidroom/adjustment-schema.json")
        self.fake_command(
            "npx",
            "printf 'generated reference\\n' > rapidroom/plugin/skills/rapidroom/references/adjustments.md",
        )

        result = self.resolve()

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.git("diff", "--name-only", "--diff-filter=U"), "")
        self.assertEqual(len(self.git("rev-list", "--parents", "-n", "1", "HEAD").split()), 3)
        self.assertEqual((self.repo / "src-tauri/Cargo.lock").read_text(), "resolved lock\n")
        self.assertEqual((self.repo / "rapidroom/adjustment-schema.json").read_text(), "generated schema\n")
        self.assertEqual(
            (self.repo / "rapidroom/plugin/skills/rapidroom/references/adjustments.md").read_text(),
            "generated reference\n",
        )
        calls = self.calls.read_text()
        self.assertIn("cargo update -w --manifest-path src-tauri/Cargo.toml --offline", calls)
        self.assertIn("cargo update -w --manifest-path src-tauri/Cargo.toml\n", calls)
        self.assertIn("npm ci", calls)
        self.assertIn("node rapidroom/generate-adjustment-schema.mjs", calls)
        self.assertIn("npx vitest run src/utils/adjustmentReference.test.ts -u", calls)

    def test_mixed_conflict_falls_back_without_modifying_merge(self):
        self.merge(mixed=True)
        before_status = self.git("status", "--porcelain")
        before_head = self.git("rev-parse", "HEAD")

        result = self.resolve()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Non-generated conflict remains: src/example.txt", result.stdout)
        self.assertEqual(self.git("status", "--porcelain"), before_status)
        self.assertEqual(self.git("rev-parse", "HEAD"), before_head)
        self.assertTrue((self.repo / ".git/MERGE_HEAD").exists())
        self.assertIn("src/example.txt", self.git("diff", "--name-only", "--diff-filter=U"))
        self.assertFalse(self.calls.exists())


if __name__ == "__main__":
    unittest.main()
