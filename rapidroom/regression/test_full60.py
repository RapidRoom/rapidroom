import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import full60

ROOT = Path(__file__).resolve().parents[2]
SHA = '1' * 40
ENGINE = '2' * 64


class Full60Tests(unittest.TestCase):
    def setUp(self):
        self.ids, self.corpus_sha = full60.corpus_ids(ROOT)
        self.recipe = {'corpus_sha256': self.corpus_sha,
                       'adjustment_sets': {name: {'sha256': hashlib.sha256(name.encode()).hexdigest()} for name in full60.SETS},
                       'lut_sha256': '3' * 64}
        self.engine = {'source_sha': SHA, 'engine_sha256': ENGINE}

    def manifest(self, shard):
        return {**copy.deepcopy(self.recipe), 'engine': {'sha256': ENGINE},
                'renders': [{'set': setting, 'id': image, 'ok': True, 'width': 16, 'height': 12,
                             'pixel_sha256': hashlib.sha256(f'{setting}/{image}'.encode()).hexdigest()}
                            for setting, image in sorted(full60.keys(self.ids[shard::4]))],
                'environment': {'hostname': 'one runner'}, 'created': 'one time'}

    def reports(self):
        return [full60.validate_shard(ROOT, i, self.manifest(i), self.manifest(i), self.engine) for i in range(4)]

    def test_independent_recordings_produce_identical_complete_reference_bytes(self):
        first, second = self.reports(), []
        for i in reversed(range(4)):
            a, b = self.manifest(i), self.manifest(i)
            a.update(created='a later time', environment={'hostname': 'another runner'})
            b.update(created='yet later', environment={'kernel': 'another kernel'})
            second.append(full60.validate_shard(ROOT, i, a, b, self.engine))
        a, b = full60.combine(ROOT, SHA, first), full60.combine(ROOT, SHA, second)
        self.assertEqual(len(a['images']), 60)
        self.assertEqual(json.dumps(a, sort_keys=True), json.dumps(b, sort_keys=True))
        full60.validate_reference(a, self.ids, self.corpus_sha)
        self.assertEqual(full60.compare(a, b), [])

    def test_missing_extra_duplicate_failed_and_unhashed_cases_fail(self):
        for mutation in ('missing', 'duplicate', 'extra', 'failed', 'hash'):
            with self.subTest(mutation=mutation):
                a, b = self.manifest(0), self.manifest(0)
                if mutation == 'missing':
                    a['renders'].pop()
                elif mutation == 'duplicate':
                    a['renders'].append(copy.deepcopy(a['renders'][0]))
                elif mutation == 'extra':
                    a['renders'].append({**a['renders'][0], 'id': 'unknown-camera'})
                elif mutation == 'failed':
                    a['renders'][0]['ok'] = False
                else:
                    del a['renders'][0]['pixel_sha256']
                with self.assertRaises(ValueError):
                    full60.validate_shard(ROOT, 0, a, b, self.engine)

    def test_nondeterminism_wrong_engine_and_recipe_changes_fail(self):
        for mutation in ('pixels', 'dimensions', 'engine', 'recipe'):
            with self.subTest(mutation=mutation):
                a, b = self.manifest(0), self.manifest(0)
                if mutation == 'pixels':
                    b['renders'][0]['pixel_sha256'] = 'f' * 64
                elif mutation == 'dimensions':
                    b['renders'][0]['width'] += 1
                elif mutation == 'engine':
                    b['engine']['sha256'] = 'f' * 64
                else:
                    b['lut_sha256'] = 'f' * 64
                with self.assertRaises(ValueError):
                    full60.validate_shard(ROOT, 0, a, b, self.engine)

    def test_missing_duplicate_and_mixed_shards_fail(self):
        for mutation in ('missing', 'duplicate', 'source', 'engine', 'recipe', 'failed'):
            with self.subTest(mutation=mutation):
                rows = self.reports()
                if mutation == 'missing':
                    rows.pop()
                elif mutation == 'duplicate':
                    rows[-1] = rows[0]
                elif mutation == 'source':
                    rows[0]['source_sha'] = 'f' * 40
                elif mutation == 'engine':
                    rows[0]['engine_sha256'] = 'f' * 64
                elif mutation == 'recipe':
                    rows[0]['recipe']['lut_sha256'] = 'f' * 64
                else:
                    rows[0]['deterministic'] = False
                with self.assertRaises(ValueError):
                    full60.combine(ROOT, SHA, rows)

    def test_exact_reference_difference_and_rejection_of_18_case_reference(self):
        reference = full60.combine(ROOT, SHA, self.reports())
        current = copy.deepcopy(reference)
        current['images'][0]['pixel_sha256'] = 'f' * 64
        self.assertEqual(len(full60.compare(reference, current)), 1)
        old = copy.deepcopy(reference)
        old['images'] = old['images'][:18]
        with self.assertRaises(ValueError):
            full60.validate_reference(old, self.ids, self.corpus_sha)
        with self.assertRaisesRegex(ValueError, 'recipe differs'):
            full60.compare(reference, {**current, 'lut_sha256': 'f' * 64})

    def test_missing_full_reference_fails_check_mode(self):
        with tempfile.TemporaryDirectory() as folder:
            args = SimpleNamespace(source=ROOT, source_sha='', record=False,
                                   reference=Path(folder) / 'missing.json', out=Path(folder) / 'plan.json')
            with patch.object(full60, 'resolve_source', return_value=SHA):
                with self.assertRaisesRegex(ValueError, 'full reference missing'):
                    full60.plan(args)

    def test_record_source_requires_pinned_main_ancestor(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            def git(*args):
                return subprocess.check_output(['git', '-C', folder, *args], text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '-b', 'main')
            git('config', 'user.name', 'test')
            git('config', 'user.email', 'test@example.invalid')
            (root / 'source').write_text('main')
            git('add', '.')
            git('commit', '-m', 'main')
            main = git('rev-parse', 'HEAD')
            git('update-ref', 'refs/remotes/origin/main', main)
            git('checkout', '-b', 'workflow')
            (root / 'source').write_text('workflow changes')
            git('commit', '-am', 'workflow')
            branch = git('rev-parse', 'HEAD')
            for source in ('', branch):
                with self.assertRaises(ValueError):
                    full60.resolve_source(root, source, True)
            self.assertEqual(full60.resolve_source(root, main, True), main)

    def test_cli_artifact_roundtrip_and_difference_exit_status(self):
        import sys
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            reports = root / 'reports'
            script = ROOT / 'rapidroom/regression/full60.py'
            full60.write(root / 'engine.json', self.engine)
            for i in range(4):
                first, second = root / f'first-{i}.json', root / f'second-{i}.json'
                full60.write(first, self.manifest(i))
                full60.write(second, self.manifest(i))
                command = [sys.executable, str(script), 'shard', '--source', str(ROOT),
                           '--source-sha', SHA, '--shard', str(i), '--first', str(first),
                           '--second', str(second), '--engine', str(root / 'engine.json'),
                           '--out', str(reports / f'shard-{i}')]
                run = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertTrue((reports / f'shard-{i}' / 'run1-manifest.json').exists())
            output = root / 'record'
            command = [sys.executable, str(script), 'collect', '--source', str(ROOT),
                       '--source-sha', SHA, '--reports', str(reports), '--reference', str(output / 'ci-reference-full60.json'),
                       '--out', str(output)]
            recorded = subprocess.run(command + ['--record'], capture_output=True, text=True)
            self.assertEqual(recorded.returncode, 0, recorded.stderr)
            checked = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(checked.returncode, 0, checked.stderr)
            self.assertEqual(full60.read(output / 'comparison.json')['n_identical'], 60)
            path = reports / 'shard-0/shard.json'
            data = full60.read(path)
            data['images'][0]['pixel_sha256'] = 'f' * 64
            full60.write(path, data)
            failed = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(failed.returncode, 1)
            self.assertEqual(full60.read(output / 'comparison.json')['n_failed'], 1)
            self.assertIn('pixels/dimensions differ', failed.stderr)

    def test_committed_reference_covers_every_case(self):
        path = ROOT / 'rapidroom/regression/ci-reference-full60.json'
        if not path.exists():
            self.skipTest('bootstrap recording not committed yet')
        full60.validate_reference(full60.read(path), self.ids, self.corpus_sha)


if __name__ == '__main__':
    unittest.main()
