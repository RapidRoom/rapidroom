import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('stage_openvino', Path(__file__).parents[1] / 'stage-openvino.py')
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)


class StageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'; self.source.mkdir()
        self.payload = bytes.fromhex('7f454c4602010100000000000000000003003e00') + b'fixture'
        (self.source / 'runtime.so').write_bytes(self.payload)
        self.manifest = {'schema': 1, 'platform': 'linux-x86_64', 'notices': [],
                         'files': [{'file': 'runtime.so', 'bytes': len(self.payload),
                                    'sha256': hashlib.sha256(self.payload).hexdigest(), 'rpath': ['$ORIGIN']}],
                         'aliases': {'runtime.so.1': 'runtime.so'}}

    def tearDown(self):
        self.temporary.cleanup()

    def test_staging_keeps_relative_alias_and_refuses_existing_directory(self):
        out = self.root / 'out'
        self.assertEqual(STAGE.stage(self.source, out, self.manifest), len(self.payload))
        self.assertEqual((out / 'runtime.so.1').readlink(), Path('runtime.so'))
        self.assertEqual((out / 'runtime.so').read_bytes(), self.payload)
        with self.assertRaises(FileExistsError):
            STAGE.stage(self.source, out, self.manifest)
        self.assertEqual((out / 'runtime.so').read_bytes(), self.payload)

    def test_tampering_and_wrong_architecture_fail_before_publication(self):
        for payload in [self.payload[:-1] + b'x', b'MZ' + self.payload[2:]]:
            (self.source / 'runtime.so').write_bytes(payload)
            with self.assertRaises(ValueError):
                STAGE.stage(self.source, self.root / 'out', self.manifest)
            self.assertFalse((self.root / 'out').exists())

    def test_path_and_alias_escapes_are_rejected(self):
        for name in ['../runtime.so', '/runtime.so', '.', '..']:
            invalid = copy.deepcopy(self.manifest)
            invalid['files'][0]['file'] = name
            with self.assertRaises(ValueError):
                STAGE.verify(self.source, invalid)
        invalid = copy.deepcopy(self.manifest); invalid['aliases']['../outside'] = 'runtime.so'
        with self.assertRaises(ValueError):
            STAGE.verify(self.source, invalid)

    def test_missing_file_symlink_and_search_path_are_rejected(self):
        invalid = copy.deepcopy(self.manifest); invalid['files'][0]['rpath'] = ['/tmp/other']
        with self.assertRaises(ValueError):
            STAGE.verify(self.source, invalid)
        path = self.source / 'runtime.so'; path.unlink(); path.symlink_to(self.root / 'missing')
        with self.assertRaises(ValueError):
            STAGE.verify(self.source, self.manifest)

    def test_copy_failure_cleans_only_new_output(self):
        invalid = copy.deepcopy(self.manifest)
        invalid['notices'] = [{'file': 'missing', 'bytes': 1, 'sha256': '0'*64}]
        with self.assertRaises(FileNotFoundError):
            STAGE.stage(self.source, self.root / 'out', invalid)
        self.assertFalse((self.root / 'out').exists())
        self.assertEqual((self.source / 'runtime.so').read_bytes(), self.payload)


if __name__ == '__main__':
    unittest.main()
