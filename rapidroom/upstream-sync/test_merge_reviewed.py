"""Run merge-reviewed.sh's review_covers against real local git histories."""
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent / "merge-reviewed.sh"
STANDING = "Covered by the standing maintainer approval for upstream render changes (2026-10-23)."


class ReviewCoversTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="merge-reviewed-test-")
        self.repo = Path(self.temp.name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.commit("base", {"src/app.ts": "a\n", "rapidroom/regression/ci-reference.json": "{}\n"})
        self.git("checkout", "-b", "sync/upstream-test")
        self.reviewed = self.commit("sync merge", {"src/sync.ts": "upstream\n"})

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, message, files):
        for name, text in files.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.git("add", ".")
        self.git("commit", "--allow-empty", "-m", message)
        return self.git("rev-parse", "HEAD")

    def advance_main(self, files):
        self.git("checkout", "main")
        self.commit("main moves", files)
        self.git("checkout", "sync/upstream-test")

    def covers(self, reviewed=None):
        head, main = self.git("rev-parse", "HEAD"), self.git("rev-parse", "main")
        result = subprocess.run(["bash", "-c", f'source "{SCRIPT}"; review_covers "$@"', "_",
                                 reviewed or self.reviewed, head, main], cwd=self.repo)
        return result.returncode == 0

    def test_review_at_head(self):
        self.assertTrue(self.covers())

    def test_clean_merge_of_main(self):
        self.advance_main({"src/other.ts": "main\n"})
        self.git("merge", "--no-ff", "--no-edit", "main")
        self.assertTrue(self.covers())

    def test_merge_of_main_with_extra_changes(self):
        self.advance_main({"src/other.ts": "main\n"})
        self.git("merge", "--no-ff", "--no-commit", "main")
        (self.repo / "src/sync.ts").write_text("changed during the merge\n")
        self.git("add", ".")
        self.git("commit", "--no-edit")
        self.assertFalse(self.covers())

    def test_change_record_commit(self):
        self.commit("record", {"rapidroom/changes/1-sync.json": "{}\n"})
        self.assertTrue(self.covers())

    def test_reference_under_standing_approval(self):
        self.commit(f"Re-record the reference\n\n{STANDING}",
                    {"rapidroom/regression/ci-reference.json": '{"new": 1}\n', "rapidroom/changes/1-sync.json": "{}\n"})
        self.assertTrue(self.covers())

    def test_reference_without_standing_approval(self):
        self.commit("Re-record the reference", {"rapidroom/regression/ci-reference.json": '{"new": 1}\n'})
        self.assertFalse(self.covers())

    def test_code_commit_after_review(self):
        self.commit("fix", {"src/sync.ts": "fixed\n"})
        self.assertFalse(self.covers())

    def test_code_hidden_in_a_reference_commit(self):
        self.commit(f"Re-record\n\n{STANDING}", {"rapidroom/regression/ci-reference.json": "{}\n{}\n", "src/sync.ts": "x\n"})
        self.assertFalse(self.covers())

    def test_deleted_reference_fails_the_checks(self):
        self.git("rm", "-q", "rapidroom/regression/ci-reference.json")
        self.git("commit", "-m", f"Drop the reference\n\n{STANDING}")
        head, main = self.git("rev-parse", "HEAD"), self.git("rev-parse", "main")
        result = subprocess.run(["bash", "-c", f'source "{SCRIPT}"; RUNNER_TEMP=$(mktemp -d); checks_pass "$@"', "_", head, main],
                                cwd=self.repo, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no ci-reference.json", result.stdout)

    def test_review_not_in_head(self):
        self.git("checkout", "-b", "elsewhere", "main")
        other = self.commit("unrelated", {"src/x.ts": "x\n"})
        self.git("checkout", "sync/upstream-test")
        self.assertFalse(self.covers(other))


if __name__ == "__main__":
    unittest.main()
