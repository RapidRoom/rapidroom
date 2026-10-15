"""Original synthetic JPEGs; independently verify native culling writes (issue 213)."""

import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess

from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def protected(path):
    """Independent JPEG walker: retain everything except APP1, including scans/trailer."""
    data = path.read_bytes()
    assert data[:2] == b'\xff\xd8'
    kept, position = bytearray(data[:2]), 2
    while position < len(data):
        start = position
        assert data[position] == 255
        while data[position] == 255:
            position += 1
        marker = data[position]
        position += 1
        if marker == 0xD9:
            kept.extend(data[start:])
            return digest(kept)
        length = int.from_bytes(data[position:position + 2], 'big')
        assert length >= 2 and position + length <= len(data)
        end = position + length
        if marker != 0xE1:
            kept.extend(data[start:end])
        position = end
        if marker == 0xDA:
            start = position
            while True:
                boundary = data.index(b'\xff', position)
                position = boundary + 1
                while data[position] == 255:
                    position += 1
                if data[position] == 0 or 0xD0 <= data[position] <= 0xD7:
                    position += 1
                    continue
                kept.extend(data[start:boundary])
                position = boundary
                break
    raise AssertionError('No EOI')


def evidence(path):
    with Image.open(path) as image:
        pixels = digest(image.convert('RGB').tobytes())
        dimensions = image.size
    info = path.stat()
    return {'protected_sha256': protected(path), 'pixels_sha256': pixels,
            'dimensions': dimensions, 'mtime_ns': info.st_mtime_ns,
            'mode': stat.S_IMODE(info.st_mode),
            'xattrs': {key: os.getxattr(path, key).hex() for key in os.listxattr(path)}}


def tags(path):
    return json.loads(subprocess.check_output([
        'exiftool', '-j', '-n', '-s', '-XMP:Rating', '-XMP:Label', '-EXIF:Rating',
        '-EXIF:RatingPercent', '-XMP:Subject', '-XMP:CreatorTool', str(path)], text=True))[0]


def prepare(case):
    paths = [case / 'input/zz-cull-one.jpg', case / 'input/zz-cull-two.jpeg']
    for index, path in enumerate(paths):
        image = Image.new('RGB', (512, 384))
        image.putdata([((x * 7 + index) % 256, (y * 11) % 256, (x + y * 3) % 256)
                       for y in range(384) for x in range(512)])
        image.save(path, quality=89, progressive=bool(index))
    subprocess.run(['exiftool', '-overwrite_original', '-XMP:Rating=2', '-XMP:Label=Blue',
                    '-EXIF:Rating=2', '-EXIF:RatingPercent=25', '-XMP:Subject=foreign-keyword',
                    '-XMP:CreatorTool=Original synthetic fixture', str(paths[0])], check=True,
                   stdout=subprocess.DEVNULL)
    for path in paths:
        os.setxattr(path, 'user.rapidroom-test', b'original-owned-fixture')
    (case / 'jpeg-metadata-test.json').write_text(json.dumps({'issue': 213,
        'fixture': 'original synthetic pixels; no private photo',
        'before': {path.name: evidence(path) for path in paths}}, indent=2) + '\n')


def run_jpeg_checks(smoke, wait_for):
    case = smoke.case
    paths = [case / 'input/zz-cull-one.jpg', case / 'input/zz-cull-two.jpeg']
    before = {path.name: evidence(path) for path in paths}
    settings = case / 'data/io.github.CyberTimon.RapidRAW/settings.json'

    def invoke(command, **arguments):
        return smoke.execute('return window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]);',
                             [command, arguments])

    def check(path, rating, label=None, exif=True):
        actual = tags(path)
        # -s unqualified duplicate Rating values are disambiguated with -G1 below.
        grouped = json.loads(subprocess.check_output(['exiftool', '-j', '-n', '-G1', '-s',
            '-XMP:Rating', '-EXIF:Rating', '-EXIF:RatingPercent', str(path)], text=True))[0]
        assert grouped['XMP-xmp:Rating'] == rating, grouped
        if exif:
            assert grouped['IFD0:Rating'] == max(rating, 0), grouped
            assert grouped['IFD0:RatingPercent'] == [0, 1, 25, 50, 75, 99][max(rating, 0)], grouped
            assert actual.get('Subject') == 'foreign-keyword', actual
            assert actual.get('CreatorTool') == 'Original synthetic fixture', actual
        else:
            assert 'IFD0:Rating' not in grouped, grouped
        assert actual.get('Label') == label, actual
        assert evidence(path) == before[path.name], (path.name, evidence(path), before[path.name])
        smoke.step('JPEG independent preservation', {'file': path.name, 'rating': rating,
                   'label': label, 'evidence': evidence(path), 'metadata': grouped})

    def toggle(enabled):
        smoke.key('Comma', ',', True)
        wait_for(lambda: smoke.execute("return !!document.querySelector('#write-jpeg-metadata-toggle');"),
                 'JPEG setting')
        state = smoke.execute("return document.querySelector('#write-jpeg-metadata-toggle').checked;")
        assert isinstance(state, bool), state
        if state != enabled:
            smoke.execute("document.querySelector('#write-jpeg-metadata-toggle').click();return true;")
        wait_for(lambda: json.loads(settings.read_text()).get('writeJpegMetadata', False) == enabled,
                 'persisted JPEG opt-in')
        smoke.capture('jpeg-setting-' + str(enabled).lower())
        smoke.key('Escape', 'Escape')
        wait_for(lambda: not smoke.execute("return !!document.querySelector('#write-jpeg-metadata-toggle');"),
                 'settings closed')

    def select(path):
        wait_for(lambda: smoke.execute('return [...document.images].some(e=>e.alt===arguments[0]);',
                                      [path.name]), 'JPEG thumbnail')
        smoke.execute("const e=[...document.images].find(e=>e.alt===arguments[0]);"
                      "e.dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));return true;", [path.name])

    def menu(label):
        smoke.execute("const e=[...document.images].find(e=>e.alt===arguments[0]);const r=e.getBoundingClientRect();"
                      "e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,button:2,clientX:r.x+5,clientY:r.y+5}));"
                      "return true;", [paths[0].name])
        wait_for(lambda: smoke.execute('return [...document.querySelectorAll("button")].some(e=>e.innerText.trim()===arguments[0]);',
                                      [label]), 'metadata context action')
        smoke.execute('const e=[...document.querySelectorAll("button")].find(e=>e.innerText.trim()===arguments[0]);'
                      'if(e.disabled)throw Error("Metadata action disabled");e.click();return true;', [label])

    original = [digest(path.read_bytes()) for path in paths]
    assert not json.loads(settings.read_text()).get('writeJpegMetadata', False)
    invoke('set_color_label_for_paths', paths=[str(paths[0])], color='blue')
    invoke('set_rating_for_paths', paths=[str(path) for path in paths], rating=3)
    assert original == [digest(path.read_bytes()) for path in paths], 'Default-off modified originals'
    toggle(True)
    smoke.execute("const e=document.querySelector('[data-tooltip=\"Back to Library\"]');"
                  "if(!e)throw Error('Back to Library missing');e.click();return true;")
    select(paths[0])
    smoke.key('Digit4', '4')
    wait_for(lambda: tags(paths[0]).get('Rating') == 4, 'native rating embed')
    check(paths[0], 4, 'Blue')
    invoke('set_color_label_for_paths', paths=[str(paths[0])], color='red')
    check(paths[0], 4, 'Red')
    select(paths[0])
    smoke.key('KeyX', 'x')
    wait_for(lambda: json.loads(subprocess.check_output(['exiftool','-j','-n','-XMP:Rating',str(paths[0])], text=True))[0].get('Rating') == -1,
             'native reject embed')
    check(paths[0], -1, 'Red')
    menu('Undo last metadata write')
    wait_for(lambda: tags(paths[0]).get('Rating') == 4, 'metadata Undo')
    check(paths[0], 4, 'Red')
    toggle(False)
    select(paths[0])
    smoke.key('Digit5', '5')
    sidecar = paths[0].with_name(paths[0].name + '.rrdata')
    wait_for(lambda: json.loads(sidecar.read_text()).get('rating') == 5, 'disabled culling still saves rrdata')
    check(paths[0], 4, 'Red')
    menu('Write metadata to files')
    wait_for(lambda: tags(paths[0]).get('Rating') == 5, 'manual write while automatic setting off')
    check(paths[0], 5, 'Red')
    menu('Undo last metadata write')
    wait_for(lambda: tags(paths[0]).get('Rating') == 4, 'manual-write Undo')
    assert json.loads(sidecar.read_text())['rating'] == 5
    check(paths[0], 4, 'Red')
    toggle(True)
    invoke('set_rating_for_paths', paths=[str(path) for path in paths], rating=0)
    check(paths[0], 0, 'Red')
    check(paths[1], 0, exif=False)
    invoke('set_color_label_for_paths', paths=[str(paths[0])], color=None)
    check(paths[0], 0)
    smoke.capture('jpeg-metadata-library')

    # Refusals are tested outside the displayed folder to avoid thumbnail decoder errors.
    malformed, readonly = case / 'malformed.jpg', case / 'readonly.jpg'
    malformed.write_bytes(b'not a JPEG')
    readonly.write_bytes(paths[1].read_bytes())
    readonly.chmod(0o444)
    failures = [malformed, readonly, case / 'input/smoke.ARW']
    hashes = [digest(path.read_bytes()) for path in failures]
    invoke('write_jpeg_metadata_for_paths', paths=[str(path) for path in failures])
    wait_for(lambda: smoke.execute('return arguments[0].every(n=>document.body.innerText.includes(n));',
                                  [[path.name for path in failures]]), 'one named batch refusal report')
    assert smoke.execute("return document.querySelectorAll('[id=\"jpeg-metadata-report\"]').length;") == 1
    assert hashes == [digest(path.read_bytes()) for path in failures]
    smoke.capture('jpeg-metadata-refusals')
    readonly.chmod(0o644)
    toggle(False)
    smoke.execute("const e=[...document.images].find(e=>e.alt==='smoke.ARW');"
                  "e.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0}));return true;")
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('4608 × 3072') && document.body.innerText.includes('Exposure');"),
             'return to RAW editor')
    (case / 'jpeg-metadata-result.json').write_text(json.dumps({'pass': True,
        'default_off_bytes_identical': True, 'automatic_native_rating_and_reject': True,
        'manual_write_setting_off': True, 'guarded_undo': True, 'named_batch_refusals': True,
        'independent': ['ExifTool', 'Pillow decoded RGB', 'non-APP1 bytes', 'mtime/mode/xattrs'],
        'after': {path.name: evidence(path) for path in paths}}, indent=2) + '\n')
