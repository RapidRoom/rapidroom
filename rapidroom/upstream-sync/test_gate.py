"""Run the real shell gate against local git repositories and a fake gh CLI."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "rapidroom/upstream-sync/gate.sh"
MOCK = r'''#!/usr/bin/env python3
import json, os, sys, subprocess
from pathlib import Path
from urllib.parse import parse_qs, urlparse
c=json.loads(Path(os.environ['MOCK_CONFIG']).read_text())
p=Path(os.environ['RUNNER_TEMP'])
with (p/'calls.jsonl').open('a') as f:f.write(json.dumps(sys.argv[1:])+'\n')
a=sys.argv[1:]
sfile=p/'state.json';s=json.loads(sfile.read_text()) if sfile.exists() else {}
if a[0]=='api':
    path=next(v for v in a[1:] if v.startswith('repos/'))
    if '/git/ref/heads/main' in path:
        s['base_reads']=s.get('base_reads',0)+1;sfile.write_text(json.dumps(s))
        print(c['advance'] if c.get('advance') and s['base_reads']>=2 else c['base'])
    elif '/git/commits/' in path:
        if c.get('parent_unavailable'):sys.exit(1)
        print('e'*40 if c.get('merge_race') else (c['advance'] if c.get('advance') and s.get('base_reads',0)>=2 else c['base']))
    elif '/pulls/' in path:
        query=a[-1]
        if '.labels' in query:print('true' if c.get('human') else 'false')
        elif query=='.merge_commit_sha':print('a'*40)
        else:
            s['head_reads']=s.get('head_reads',0)+1;sfile.write_text(json.dumps(s))
            if c.get('head_change') and s['head_reads']>=c['head_change']:print('f'*40)
            else:print(subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip())
    elif '/jobs?' in path:
        ident=int(path.split('/runs/')[1].split('/')[0]);wf=c['workflows'][ident-1]
        jobs=c['jobs'][wf]
        if c.get('missing_fmt') and wf=='lint.yml':jobs=[j for j in jobs if j['name']!='cargo fmt']
        if c.get('skip_compare') and wf=='regression.yml':jobs[0]['steps'][1]['conclusion']='skipped'
        if c.get('render_change') and wf=='regression.yml':jobs[0]['steps'][1]['conclusion']='failure';jobs[0]['conclusion']='failure'
        print(json.dumps([{'jobs':jobs}]))
    else:
        sha=parse_qs(urlparse(path).query)['head_sha'][0]
        base=c['base'] if sha==c['head'] else c.get('advance',c['base'])
        rows=[]
        for index,wf in enumerate(c['workflows'],1):
            if c.get('reg_missing') and wf=='regression.yml':continue
            conclusion='failure' if wf==c.get('failed') or (c.get('render_change') and wf=='regression.yml') else 'success'
            rows.append({'id':index,'path':'.github/workflows/'+wf,'head_sha':sha,'status':'completed','conclusion':conclusion,'pull_requests':[{'number':7,'head':{'sha':sha},'base':{'sha':'e'*40 if c.get('stale_base') else base}}]})
        print(json.dumps({'workflow_runs':rows}))
elif a[:2]==['issue','list']:print('')
else:print('mock mutation only')
'''


class GateTests(unittest.TestCase):
    def run_case(self, *, engine=True, unknown_filter=False, reference_change=False, **scenario):
        with tempfile.TemporaryDirectory(prefix="upstream-sync-test-") as folder:
            temp = Path(folder)
            repo = temp / "repo"
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=repo, text=True, stderr=subprocess.DEVNULL).strip()
            git("init", "-b", "main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.invalid")
            for wf in ["regression.yml", "mcp-client.yml", "rapidroom-status.yml"]:
                dest = repo / ".github/workflows" / wf
                dest.parent.mkdir(parents=True, exist_ok=True)
                text = (ROOT / ".github/workflows" / wf).read_text()
                if unknown_filter and wf == "regression.yml":
                    text = text.replace("      - 'src-tauri/**'", "      - 'src-tauri/**'\n      - 'another-renderer/**'")
                dest.write_text(text)
            ref = repo / "rapidroom/regression/ci-reference.json"
            ref.parent.mkdir(parents=True)
            ref.write_text('{"reference":"original"}\n')
            git("add", ".")
            git("commit", "-m", "base")
            base = git("rev-parse", "HEAD")
            changed = repo / ("src-tauri/src/fixture.rs" if engine else "README.md")
            changed.parent.mkdir(parents=True, exist_ok=True)
            changed.write_text("test-only candidate marker\n")
            if reference_change:
                ref.write_text('{"reference":"changed"}\n')
            git("add", ".")
            git("commit", "-m", "candidate")
            inside = None
            if scenario.pop("advance_inside_head", False):
                inside = git("rev-parse", "HEAD")
                (repo / "candidate-doc.md").write_text("extra candidate commit\n")
                git("add", ".")
                git("commit", "-m", "extra candidate")
            head = git("rev-parse", "HEAD")
            git("branch", "upstream-sync")
            bare = temp / "origin.git"
            subprocess.run(["git", "init", "--bare", str(bare)], check=True, capture_output=True)
            git("remote", "add", "origin", str(bare))
            git("push", "origin", "upstream-sync")
            advance = None
            if scenario.pop("advance_base", False) or inside:
                git("checkout", "main")
                git("reset", "--hard", inside or base)
                if not inside:
                    (repo / "new-base.md").write_text("base advanced\n")
                    git("add", ".")
                    git("commit", "-m", "new base")
                advance = git("rev-parse", "HEAD")
                git("push", "origin", "main")
                git("checkout", "upstream-sync")
            else:
                git("push", "origin", f"{base}:refs/heads/main")
            jobs = {
                "pr-ci.yml": [{"name": "build (ubuntu-24.04, 03) / build", "conclusion": "success"}],
                "csp.yml": [{"name": "csp", "conclusion": "success"}],
                "lint.yml": [{"name": name, "conclusion": "success"} for name in ["Frontend Lint & Format", "cargo fmt", "cargo clippy", "Headless CLI exit codes"]],
                "frontend-test.yml": [{"name": "Vitest", "conclusion": "success"}],
                "regression.yml": [{"name": "regression", "conclusion": "success", "steps": [{"name": "Determinism (run 1 vs run 2)", "conclusion": "success"}, {"name": "Compare with ci-reference.json", "conclusion": "success"}]}],
                "mcp-client.yml": [{"name": "client", "conclusion": "success"}],
                "rapidroom-status.yml": [{"name": "check", "conclusion": "success"}, {"name": "refresh", "conclusion": "skipped"}],
            }
            config = temp / "config.json"
            config.write_text(json.dumps({"base": base, "head": head, "advance": advance, "workflows": list(jobs), "jobs": jobs, **scenario}))
            bindir = temp / "bin"
            bindir.mkdir()
            gh = bindir / "gh"
            gh.write_text(MOCK)
            gh.chmod(0o755)
            env = {**os.environ, "PATH": str(bindir) + ":" + os.environ["PATH"], "MOCK_CONFIG": str(config), "RUNNER_TEMP": str(temp), "GH_REPO": "test/repo", "GITHUB_RUN_ID": "1", "BRANCH": "upstream-sync"}
            command = 'source "$1"; poll_pause() { DEADLINE=0; }; gate_and_merge 7 "$2" "$3"'
            result = subprocess.run(["bash", "-c", command, "test", str(GATE), base, head], cwd=repo, env=env, capture_output=True, text=True, timeout=20)
            calls = [json.loads(line) for line in (temp / "calls.jsonl").read_text().splitlines()]
            merged = [a for a in calls if a[:2] == ["pr", "merge"]]
            if not reference_change:
                self.assertEqual(ref.read_text(), '{"reference":"original"}\n')
            if not scenario.get('merge_race') and not scenario.get('parent_unavailable'):
                self.assertFalse(any(a[:2] == ["workflow", "run"] for a in calls))
            return result, calls, merged, head

    def test_every_check_passes_and_merge_is_bound(self):
        result, _, merged, head = self.run_case()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(merged[0][-2:], ["--match-head-commit", head])

    def test_missing_regression_fails_closed(self):
        result, _, merged, _ = self.run_case(reg_missing=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(merged)

    def test_proven_docs_filter_skip_is_allowed(self):
        result, _, merged, _ = self.run_case(engine=False, reg_missing=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(merged)

    def test_unknown_filter_cannot_skip(self):
        result, _, merged, _ = self.run_case(engine=False, unknown_filter=True, reg_missing=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(merged)

    def test_build_csp_fmt_and_vitest_failures_block(self):
        for failed in ["pr-ci.yml", "csp.yml", "lint.yml", "frontend-test.yml"]:
            with self.subTest(failed=failed):
                result, _, merged, _ = self.run_case(failed=failed)
                self.assertEqual(result.returncode, 1)
                self.assertFalse(merged)

    def test_missing_fmt_job_cannot_hide_in_successful_workflow(self):
        result, _, merged, _ = self.run_case(missing_fmt=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(merged)

    def test_skipped_reference_comparison_cannot_pass(self):
        result, _, merged, _ = self.run_case(skip_compare=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(merged)

    def test_render_change_notifies_human_without_merge_or_bless(self):
        result, calls, merged, _ = self.run_case(render_change=True)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(merged)
        self.assertTrue(any(a[:2] == ["issue", "create"] and "needs-human" in a and "rendering-change" in a for a in calls))

    def test_reference_change_requires_human(self):
        result, calls, merged, _ = self.run_case(reference_change=True)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(merged)
        self.assertTrue(any(a[:2] == ["issue", "create"] for a in calls))

    def test_stale_base_run_cannot_pass(self):
        result, _, merged, _ = self.run_case(stale_base=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(merged)

    def test_head_changes_during_wait_and_before_merge_block(self):
        for head_change in [1, 2]:
            with self.subTest(head_change=head_change):
                result, _, merged, _ = self.run_case(head_change=head_change)
                self.assertEqual(result.returncode, 1)
                self.assertFalse(merged)

    def test_human_label_blocks_merge(self):
        result, _, merged, _ = self.run_case(human=True)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(merged)

    def test_base_advance_refreshes_and_checks_new_head(self):
        result, calls, merged, head = self.run_case(advance_base=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(merged[0][-1], head)
        queries = [a[1] for a in calls if a[0] == "api" and "head_sha=" in a[1]]
        self.assertEqual(len(queries), 2)
        self.assertNotEqual(queries[0], queries[1])

    def test_base_advance_inside_head_still_triggers_fresh_checks(self):
        result, calls, merged, head = self.run_case(advance_inside_head=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(merged[0][-1], head)
        queries = [a[1] for a in calls if a[0] == "api" and "head_sha=" in a[1]]
        self.assertEqual(len(queries), 2)

    def test_seconds_wide_merge_race_escalates_and_dispatches_main_checks(self):
        result, calls, merged, _ = self.run_case(merge_race=True)
        self.assertEqual(result.returncode, 1)
        self.assertTrue(merged)
        self.assertTrue(any(a[:2] == ["issue", "create"] and "needs-human" in a for a in calls))
        dispatched = [a[2] for a in calls if a[:2] == ["workflow", "run"]]
        self.assertEqual(set(dispatched), {"ci.yml", "csp.yml", "lint.yml", "frontend-test.yml", "regression.yml", "mcp-client.yml", "rapidroom-status.yml"})
        self.assertTrue(all(a[-2:] == ["--ref", "main"] for a in calls if a[:2] == ["workflow", "run"]))

    def test_unverifiable_merge_parent_also_escalates(self):
        result, calls, merged, _ = self.run_case(parent_unavailable=True)
        self.assertEqual(result.returncode, 1)
        self.assertTrue(merged)
        self.assertTrue(any(a[:2] == ["issue", "create"] and "needs-human" in a for a in calls))
        self.assertEqual(sum(a[:2] == ["workflow", "run"] for a in calls), 7)
