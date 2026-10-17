"""Real Debian archive round trip for the flattened-resource regression."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import package_runtime as PACKAGE


class PackageTests(unittest.TestCase):
    def test_finished_deb_restores_links_and_integrity_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'src-tauri/resources/openvino-cpu/runtime'
            source.mkdir(parents=True)
            payload = bytes.fromhex('7f454c4602010100000000000000000003003e00') + b'native-fixture'
            notice = b'license-fixture\n'
            manifest = {'schema': 1, 'platform': 'linux-x86_64', 'binary_fingerprint': 'fixture',
                        'files': [{'file': 'runtime.so', 'bytes': len(payload),
                                   'sha256': hashlib.sha256(payload).hexdigest(), 'rpath': ['$ORIGIN']}],
                        'aliases': {'runtime.so.1': 'runtime.so'},
                        'notices': [{'file': 'license.txt', 'bytes': len(notice),
                                     'sha256': hashlib.sha256(notice).hexdigest()}]}
            committed = root / 'rapidroom/openvino/runtime-manifest.json'
            committed.parent.mkdir(parents=True)
            committed.write_text(json.dumps(manifest))
            (source / 'runtime-manifest.json').write_text(json.dumps(manifest))
            (source / 'runtime.so').write_bytes(payload)
            (source / 'runtime.so.1').symlink_to('runtime.so')
            (source / 'notices').mkdir()
            (source / 'notices/license.txt').write_bytes(notice)
            tree = root / 'package-tree'
            runtime = tree / PACKAGE.RUNTIME
            shutil.copytree(source, runtime, symlinks=False)  # Reproduce Tauri.
            unrelated = tree / 'usr/bin/keep-me'
            unrelated.parent.mkdir(parents=True)
            unrelated.write_bytes(b'unchanged executable\n'); unrelated.chmod(0o755)
            control = tree / 'DEBIAN'
            control.mkdir()
            (control / 'control').write_text('Package: runtime-test\nVersion: 1.0\nArchitecture: amd64\n'
                                           'Maintainer: Test <test@example.invalid>\n'
                                           'Installed-Size: 999999\nDescription: archive regression\n')
            target = root / 'target'
            bundle = target / 'release/bundle/deb'
            bundle.mkdir(parents=True)
            package = bundle / 'runtime-test.deb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(tree), str(package)],
                           stdout=subprocess.DEVNULL, check=True)
            before = root / 'before'; before.mkdir()
            PACKAGE.extract(package, 'deb', before)
            with self.assertRaises(ValueError):
                PACKAGE.verify_tree(root, before)
            PACKAGE.finalize(root, target, ['deb'])
            after = root / 'after'; after.mkdir()
            PACKAGE.extract(package, 'deb', after, control=True)
            self.assertEqual(PACKAGE.verify_tree(root, after)['aliases_verified'], 1)
            self.assertEqual((after / PACKAGE.RUNTIME / 'runtime.so.1').readlink(), Path('runtime.so'))
            self.assertEqual((after / 'usr/bin/keep-me').read_bytes(), unrelated.read_bytes())
            self.assertEqual((after / 'usr/bin/keep-me').stat().st_mode & 0o777, 0o755)
            subprocess.run(['md5sum', '--check', 'DEBIAN/md5sums'], cwd=after,
                           stdout=subprocess.DEVNULL, check=True)
            self.assertNotIn('999999', (after / 'DEBIAN/control').read_text())
            (after / PACKAGE.RUNTIME / 'notices/license.txt').write_bytes(b'broken\n')
            with self.assertRaises(ValueError):
                PACKAGE.verify_tree(root, after)
            # Missing staging must leave the original completed artifact intact.
            original = package.read_bytes()
            (source / 'runtime.so.1').unlink()
            with self.assertRaises(ValueError):
                PACKAGE.finalize(root, target, ['deb'])
            self.assertEqual(package.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
