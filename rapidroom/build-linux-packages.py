#!/usr/bin/env python3
"""Build Linux packages with the existing MCP feature and stdio adapter."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def source(root):
    return {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff', 'HEAD'], cwd=root)).hexdigest(),
            'status': subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True)}


def verify_packages(root, target, kinds, adapter):
    """Check the staged adapter and retained existing notices in each bundle."""
    artifacts = {}
    bundle = target / 'release/bundle'
    notices = list((root / 'rapidroom/assets/poppins').glob('*.txt'))
    for kind in kinds:
        candidates = list((bundle / kind).glob('*.deb' if kind == 'deb' else '*.AppImage'))
        if len(candidates) != 1:
            raise RuntimeError('Expected exactly one ' + kind + ' package')
        package = candidates[0]
        with tempfile.TemporaryDirectory(prefix='mcp-check-', dir=bundle) as temporary:
            folder = Path(temporary)
            if kind == 'deb':
                subprocess.run(['dpkg-deb', '--extract', str(package), str(folder)], check=True)
                contents = folder
            else:
                subprocess.run([str(package), '--appimage-extract'], cwd=folder,
                               stdout=subprocess.DEVNULL, check=True)
                contents = folder / 'squashfs-root'
            installed = contents / 'usr/bin/rapidroom-mcp-stdio'
            if not installed.is_file() or not os.access(installed, os.X_OK) or sha(installed) != sha(adapter):
                raise RuntimeError(kind + ' adapter missing, not executable or changed')
            for notice in notices:
                copies = list(contents.rglob(notice.name))
                if not any(copy.is_file() and sha(copy) == sha(notice) for copy in copies):
                    raise RuntimeError(kind + ' is missing retained notice ' + notice.name)
            artifacts[kind] = {'path': str(package), 'sha256': sha(package),
                               'adapter_sha256': sha(installed), 'adapter_executable': True,
                               'existing_font_notices_retained': True}
    return artifacts

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundles', default='deb,appimage')
    parser.add_argument('--version')
    parser.add_argument('--record', type=Path)
    parser.add_argument('--lock', type=Path, default=Path('/tmp/rapidroom-build.lock'))
    args = parser.parse_args()
    if sys.platform != 'linux' or set(args.bundles.split(',')) - {'deb', 'appimage'}:
        parser.error('Linux deb and/or appimage bundles only')
    if args.version and not re.fullmatch(r'\d+\.\d+\.\d+', args.version):
        parser.error('Version must be MAJOR.MINOR.PATCH')
    root = Path(__file__).resolve().parent.parent
    env = os.environ.copy()
    target = Path(env.get('CARGO_TARGET_DIR', str(root / 'src-tauri/target'))).resolve()
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS='4', NO_STRIP='true')
    def run(command):
        subprocess.run(['nice', '-n', '10', *command], cwd=root, env=env, check=True)
    with args.lock.open('a') as lock:
        print('Waiting for Linux package build lock', flush=True)
        fcntl.flock(lock, fcntl.LOCK_EX)
        before = source(root)
        triple = next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
        if triple != 'x86_64-unknown-linux-gnu':
            parser.error('Official Linux packaging currently targets x86_64-unknown-linux-gnu')
        run(['cargo', 'build', '--manifest-path', 'rapidroom/mcp-client/Cargo.toml', '--release', '--locked'])
        adapter = target / 'release/rapidroom-mcp-stdio'
        staged = root / 'rapidroom/mcp-client/binaries' / ('rapidroom-mcp-stdio-' + triple)
        staged.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(adapter, staged)
        command = ['npm', 'run', 'tauri', 'build', '--', '--features', 'terminal,mcp', '--bundles', args.bundles, '--ci',
                   '--config', 'src-tauri/tauri.mcp-linux.conf.json']
        if args.version:
            command += ['--config', json.dumps({'version': args.version})]
        command += ['--', '--locked']
        run(command)
        if source(root) != before or sha(staged) != sha(adapter):
            raise RuntimeError('Source or staged adapter changed during packaging')
        artifacts = verify_packages(root, target, args.bundles.split(','), adapter)
        record = {'source': before, 'features': ['terminal', 'mcp'], 'mcp_default_enabled': False,
                  'adapter_sha256': sha(adapter), 'engine_sha256': sha(target / 'release/rapidroom'),
                  'bundles': args.bundles.split(','), 'verified_artifacts': artifacts, 'cargo_locked': True, 'jobs': 4, 'target': triple}
        output = args.record or target / 'release/bundle/mcp-build.json'
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(record, indent=2) + '\n')
        print('MCP-enabled Linux packages built; control remains off until enabled', flush=True)


if __name__ == '__main__':
    main()
