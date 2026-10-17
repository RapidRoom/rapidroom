#!/usr/bin/env python3
"""Fetch the pinned release artifact and stage only its verified native CPU files.

Build time only: the wheel is never installed, imported or executed. Its Python
ABI is irrelevant to the eight C libraries; each must match runtime-manifest.json.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import urllib.request
import zipfile

SPEC = importlib.util.spec_from_file_location('stage_openvino', Path(__file__).with_name('stage-openvino.py'))
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)
ARTIFACT = Path(__file__).parent / 'openvino/source-artifact.json'
APPIMAGETOOL = Path(__file__).parent / 'openvino/appimage-tool.json'


def download(artifact, destination):
    with urllib.request.urlopen(artifact['url'], timeout=120) as response, destination.open('xb') as output:
        remaining = artifact['bytes']
        while chunk := response.read(min(1024 * 1024, remaining + 1)):
            remaining -= len(chunk)
            if remaining < 0:
                raise ValueError('Release artifact exceeds pinned size')
            output.write(chunk)
    if destination.stat().st_size != artifact['bytes'] or STAGE.digest(destination) != artifact['sha256']:
        raise ValueError('Release artifact size/checksum mismatch')


def fetch(out, wheel=None):
    artifact = json.loads(ARTIFACT.read_text())
    manifest = json.loads(STAGE.MANIFEST.read_text())
    if out.exists() or out.is_symlink():
        raise FileExistsError(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='openvino-source-', dir=out.parent) as temporary:
        folder = Path(temporary)
        if wheel is None:
            wheel = folder / 'source.whl'
            download(artifact, wheel)
        if wheel.is_symlink() or wheel.stat().st_size != artifact['bytes'] or STAGE.digest(wheel) != artifact['sha256']:
            raise ValueError('OpenVINO source artifact size/checksum mismatch')
        source = folder / 'capi'
        source.mkdir()
        with zipfile.ZipFile(wheel) as archive:
            for row in manifest['files']:
                member = 'onnxruntime/capi/' + row['file']
                matches = [info for info in archive.infolist() if info.filename == member]
                if len(matches) != 1 or matches[0].file_size != row['bytes']:
                    raise ValueError('Missing, duplicate or wrong-sized native wheel member: ' + member)
                # Never extract paths or other files supplied by the wheel.
                with archive.open(matches[0]) as stream, (source / row['file']).open('xb') as destination:
                    shutil.copyfileobj(stream, destination)
        size = STAGE.stage(source, out, manifest)
    return {'artifact_sha256': artifact['sha256'], 'bytes_unique': size,
            'binary_fingerprint': manifest['binary_fingerprint'], 'verified': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--wheel', type=Path, help='use a local copy of the same pinned artifact')
    parser.add_argument('--appimagetool-out', type=Path, help='also fetch the pinned build-only AppImage packaging tool')
    args = parser.parse_args()
    result = fetch(args.out.absolute(), args.wheel)
    if args.appimagetool_out:
        download(json.loads(APPIMAGETOOL.read_text()), args.appimagetool_out)
        args.appimagetool_out.chmod(0o755)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
