#!/usr/bin/env python3
"""Build and pin the public fork used by the MCP reproduction (Linux)."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fork', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--target', required=True, type=Path)
    args = parser.parse_args()
    if sys.version_info < (3, 11):
        parser.error('Use Python 3.11 or later')
    root, out, target = args.fork.resolve(), args.out.resolve(), args.target.resolve()
    expected = json.loads(Path(__file__).with_name('fixtures.json').read_text())['fork_commit']
    if 'samples' not in out.parts:
        parser.error('Keep the new output directory under samples/')

    def verify():
        if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() != expected:
            raise RuntimeError('Use the pinned public fork checkout')
        if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
            raise RuntimeError('Source changed or is dirty')

    verify()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.update(CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(target))

    @contextmanager
    def lock(label):
        print('Waiting for build lock: ' + label, flush=True)
        with Path('/tmp/rapidroom-build.lock').open('a') as handle:
            fcntl.flock(handle, fcntl.LOCK_EX)
            started = time.monotonic()
            try:
                verify()
                if shutil.disk_usage(out).free < 25 * 1024**3:
                    raise RuntimeError('Need at least 25 GiB free before building')
                yield
                verify()
            finally:
                fcntl.flock(handle, fcntl.LOCK_UN)
                print(f'Released {label} after {time.monotonic() - started:.1f}s', flush=True)

    command = ['cargo', 'build', '--release', '--manifest-path', 'src-tauri/Cargo.toml', '--features', 'mcp', '--locked']
    for label, argv in [('frontend', ['npm', 'run', 'build']), ('host', ['npm', 'run', 'build', '--prefix', 'mcp']), ('native', command)]:
        with lock(label):
            with (out / (label + '.log')).open('w') as log:
                subprocess.run(['nice', '-n', '10', *argv], cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, check=True)
            if label == 'native':
                # Cargo's executable name is the package name; the installer renames it.
                shutil.copy2(target / 'release/RapidRAW', out / 'rapidraw-mcp')
                with (out / 'rapidraw-mcp').open('rb') as stream:
                    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
                for name in ['resources', 'lensfun_db']:
                    (out / name).symlink_to(root / 'src-tauri' / name, target_is_directory=True)
                (out / 'build.json').write_text(json.dumps({'source_commit': expected, 'source_clean': True,
                    'binary_sha256': digest, 'cargo_locked': True, 'features': ['mcp'], 'command': command}, indent=2) + '\n')
    print('Pinned public fork build complete', flush=True)


if __name__ == '__main__':
    main()
