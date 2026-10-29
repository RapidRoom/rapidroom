"""Restore the pinned runtime after Tauri/linuxdeploy and verify shipped trees.

Tauri dereferences resource aliases; linuxdeploy may also rewrite ELF files.
The runtime's original $ORIGIN layout must survive both operations byte for byte.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

SPEC = importlib.util.spec_from_file_location('stage_openvino', Path(__file__).parents[1] / 'stage-openvino.py')
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)
RUNTIME = Path('usr/lib/RapidRoom/resources/openvino-cpu/runtime')
SUFFIX = {'deb': '*.deb', 'rpm': '*.rpm', 'appimage': '*.AppImage'}
# LICENSE is the plain AGPL v3 text; nothing in the project grants "or later".
RPM_LICENSE = 'AGPL-3.0-only'


def manifest(root):
    return json.loads((root / 'rapidroom/openvino/runtime-manifest.json').read_text())


def staged(root):
    source = root / 'src-tauri/resources/openvino-cpu/runtime'
    STAGE.verify_staged(source, manifest(root))
    return source


def verify_tree(root, contents):
    for parent in RUNTIME.parents:
        if (contents / parent).is_symlink():
            raise ValueError('Symlink in packaged runtime parent: ' + str(parent))
    size = STAGE.verify_staged(contents / RUNTIME, manifest(root))
    return {'bytes_unique': size, 'aliases_verified': len(manifest(root)['aliases']),
            'notices_verified': len(manifest(root)['notices']),
            'binary_fingerprint': manifest(root)['binary_fingerprint']}


def restore_tree(root, contents):
    source = staged(root)
    runtime = contents / RUNTIME
    # Missing payload is a bundler/config error, not something to hide by adding it.
    if not runtime.is_dir() or runtime.is_symlink():
        raise ValueError('Bundler omitted the staged OpenVINO runtime')
    for parent in RUNTIME.parents:
        if (contents / parent).is_symlink():
            raise ValueError('Symlink in packaged runtime parent: ' + str(parent))
    shutil.rmtree(runtime)
    shutil.copytree(source, runtime, symlinks=True)
    verify_tree(root, contents)


def package_path(target, kind):
    candidates = list((target / 'release/bundle' / kind).glob(SUFFIX[kind]))
    if len(candidates) != 1:
        raise ValueError('Expected exactly one ' + kind + ' package')
    return candidates[0]


def appimagetool():
    path = Path(os.environ.get('APPIMAGETOOL', ''))
    pinned = json.loads(Path(__file__).with_name('appimage-tool.json').read_text())
    if (not path.is_file() or not os.access(path, os.X_OK) or path.stat().st_size != pinned['bytes']
            or STAGE.digest(path) != pinned['sha256']):
        raise ValueError('Set APPIMAGETOOL to the checksum-pinned executable from fetch-openvino.py --appimagetool-out')
    return path.resolve()


def extract(package, kind, folder, control=False):
    if kind == 'rpm':
        payload = subprocess.run(['rpm2cpio', str(package)], stdout=subprocess.PIPE, check=True).stdout
        subprocess.run(['cpio', '-idm', '--quiet', '--no-absolute-filenames'], input=payload, cwd=folder, check=True)
        return folder
    if kind == 'deb':
        subprocess.run(['dpkg-deb', '--raw-extract' if control else '--extract', str(package), str(folder)], check=True)
        return folder
    subprocess.run([str(package), '--appimage-extract'], cwd=folder,
                   stdout=subprocess.DEVNULL, check=True)
    return folder / 'squashfs-root'


def appimage_prefix(package):
    """Keep Tauri's exact Type-2 runtime; reject signed/update-indexed inputs.

The official workflow signs neither format. Final SHA256SUMS are generated only
after this step. Refuse signatures/zsync metadata rather than invalidate them.
"""
    offset = int(subprocess.check_output([str(package), '--appimage-offset'], text=True).strip())
    if not 64 <= offset <= 16 * 1024 * 1024:
        raise ValueError('Invalid AppImage filesystem offset')
    with package.open('rb') as stream:
        prefix = bytearray(stream.read(offset))
        if stream.read(4) != b'hsqs' or prefix[:6] != b'\x7fELF\x02\x01' or prefix[8:11] != b'AI\x02':
            raise ValueError('Expected a Type-2 ELF64 little-endian AppImage')
    header = struct.unpack_from('<HHIQQQIHHHHHH', prefix, 16)
    sections = [struct.unpack_from('<IIQQQQIIQQ', prefix, header[5] + n * header[10])
                for n in range(header[11])]
    strings = sections[header[12]]
    names = prefix[strings[4]:strings[4] + strings[5]]
    digest_offset = None
    for section in sections:
        name = names[section[0]:].split(b'\0', 1)[0].decode()
        start, size = section[4:6]
        if name in ('.sha256_sig', '.sig_key', '.upd_info') and any(prefix[start:start + size]):
            raise ValueError('Finalize the runtime before adding AppImage signatures or update information')
        if name == '.digest_md5':
            if size != 16 or start + size > offset:
                raise ValueError('Invalid AppImage MD5 section')
            digest_offset = start
            prefix[start:start + size] = b'\0' * size
    if digest_offset is None:
        raise ValueError('Missing AppImage MD5 section')
    return prefix


def rebuild_appimage(package, contents, output):
    runtime = output.with_suffix('.runtime')
    runtime.write_bytes(appimage_prefix(package))
    # Use the standard tool for compression/digest generation; supplying the
    # original runtime prevents its normal runtime download/replacement.
    subprocess.run([str(appimagetool()), '--appimage-extract-and-run', '--runtime-file', str(runtime),
                    '--no-appstream', '--mksquashfs-opt', '-processors', '--mksquashfs-opt', '4',
                    str(contents), str(output)],
                   env={**os.environ, 'TMPDIR': str(output.parent), 'ARCH': 'x86_64'}, check=True)
    runtime.unlink()
    output.chmod(package.stat().st_mode & 0o777)


def rebuild_deb(contents, output):
    control = contents / 'DEBIAN/control'
    size = subprocess.check_output(['du', '-sk', '--exclude=DEBIAN', str(contents)], text=True).split()[0]
    text, count = re.subn(r'^Installed-Size:.*$', 'Installed-Size: ' + size, control.read_text(), flags=re.M)
    if count != 1:
        raise ValueError('Expected one Debian Installed-Size field')
    control.write_text(text)
    # Keep Debian's package-integrity inventory in sync with the restored files.
    rows = []
    for path in sorted(contents.rglob('*')):
        relative = path.relative_to(contents)
        if relative.parts[0] == 'DEBIAN' or path.is_symlink() or not path.is_file():
            continue
        digest = hashlib.md5()
        with path.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(chunk)
        rows.append(digest.hexdigest() + '  ' + relative.as_posix() + '\n')
    (contents / 'DEBIAN/md5sums').write_text(''.join(rows))
    subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(contents), str(output)], check=True)


def rpm_query(package, form):
    return subprocess.check_output(['rpm', '-qp', '--qf', form, str(package)], text=True,
                                   stderr=subprocess.DEVNULL)


def rebuild_rpm(package, contents, output):
    """Repackage the restored tree with rpmbuild, keeping every file byte for byte.

Tauri's own RPM flattens the runtime's aliases, has no License and declares
only webkit/gtk. rpmbuild adds the binaries' real library requirements; the
bundled private libraries under /usr/lib/RapidRoom neither provide nor require
anything, so they can't satisfy or clash with system packages. No strip, no
debuginfo, no build-id links: the restored native bytes must survive as-is.
"""
    meta = {key: rpm_query(package, '%{' + key + '}') for key in
            ('NAME', 'VERSION', 'RELEASE', 'ARCH', 'SUMMARY', 'DESCRIPTION', 'URL')}
    owned = set(rpm_query(package, '[%{FILENAMES}\n]').split('\n')) - {''}
    # Files the restore added (the runtime aliases) belong to the package too.
    dirs = [d + '/' for d in owned if (contents / d.lstrip('/')).is_dir()]
    for path in contents.rglob('*'):
        name = '/' + path.relative_to(contents).as_posix()
        if any(name.startswith(d) for d in dirs):
            owned.add(name)
    def quoted(name):
        return '"' + name.replace('%', '%%') + '"'
    lines = []
    for name in sorted(owned):
        path = contents / name.lstrip('/')
        if not path.exists() and not path.is_symlink():
            raise ValueError('Packaged path missing from restored tree: ' + name)
        lines.append(('%dir ' if path.is_dir() and not path.is_symlink() else '') + quoted(name))
    recommends = [r for r in rpm_query(package, '[%{RECOMMENDNAME}\n]').split('\n') if r]
    top = output.parent / 'rpmbuild'
    spec = top / 'rapidroom.spec'
    spec.parent.mkdir(parents=True)
    summary = meta['SUMMARY'].replace('%', '%%')
    url = meta['URL'] if meta['URL'] != '(none)' else 'https://github.com/RapidRoom/rapidroom'
    spec.write_text('\n'.join([
        '%global __os_install_post %{nil}',
        '%global __spec_install_post %{nil}',
        '%global debug_package %{nil}',
        '%global source_date_epoch_from_changelog 0',
        '%global _build_id_links none',
        '%global _missing_build_ids_terminate_build 0',
        '%global __provides_exclude_from ^/usr/lib/RapidRoom/.*$',
        '%global __requires_exclude_from ^/usr/lib/RapidRoom/.*$',
        'Name: ' + meta['NAME'], 'Version: ' + meta['VERSION'], 'Release: ' + meta['RELEASE'],
        'Summary: ' + summary, 'License: ' + RPM_LICENSE, 'URL: ' + url,
        *['Recommends: ' + r for r in recommends],
        '', '%description', meta['DESCRIPTION'].replace('%', '%%'),
        '', '%install', 'cp -a "%{rr_tree}/." "%{buildroot}/"',
        '', '%files', '%defattr(-,root,root,-)', *lines, '']))
    subprocess.run(['rpmbuild', '-bb', '--quiet', '--target', meta['ARCH'],
                    '--define', '_topdir ' + str(top), '--define', 'rr_tree ' + str(contents),
                    str(spec)], check=True, stdout=subprocess.DEVNULL)
    built = list((top / 'RPMS' / meta['ARCH']).glob('*.rpm'))
    if len(built) != 1:
        raise ValueError('Expected exactly one rebuilt RPM')
    os.replace(built[0], output)


def finalize(root, target, kinds):
    staged(root)  # Fail before changing a package if staging is incomplete.
    for kind in kinds:
        package = package_path(target, kind).resolve()
        with tempfile.TemporaryDirectory(prefix='openvino-package-', dir=package.parent) as temporary:
            folder = Path(temporary)
            tree = folder / 'tree'
            tree.mkdir()
            contents = extract(package, kind, tree, control=True)
            restore_tree(root, contents)
            output = folder / package.name
            if kind == 'deb':
                rebuild_deb(contents, output)
            elif kind == 'rpm':
                rebuild_rpm(package, contents, output)
            else:
                rebuild_appimage(package, contents, output)
            # Independently reopen the finished archive before replacing it.
            check = folder / 'check'
            check.mkdir()
            verify_tree(root, extract(output, kind, check))
            os.replace(output, package)
