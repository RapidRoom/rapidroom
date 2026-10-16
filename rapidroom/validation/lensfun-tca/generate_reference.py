#!/usr/bin/env python3
"""Capture independent numeric coordinates from Lensfun 0.3.4's public C API."""
import ctypes as C
import hashlib
import json
from pathlib import Path
import subprocess
import argparse
import tempfile
import xml.etree.ElementTree as E

ROOT = Path(__file__).resolve().parent
P, F, I = C.c_void_p, C.c_float, C.c_int


class Calibration(C.Structure):
    _fields_ = [('model', I), ('focal', F), ('terms', F * 6)]


lib = C.CDLL('liblensfun.so')
specs = {
    'lf_db_new': ([], P), 'lf_db_load_file': ([P, C.c_char_p], I),
    'lf_db_get_lenses': ([P], C.POINTER(P)), 'lf_db_destroy': ([P], None),
    'lf_modifier_new': ([P, F, I, I], P),
    'lf_modifier_initialize': ([P, P, I, F, F, F, F, I, I, I], I),
    'lf_modifier_apply_subpixel_distortion': ([P, F, F, I, I, C.POINTER(F)], I),
    'lf_modifier_destroy': ([P], None),
    'lf_lens_interpolate_tca': ([P, F, C.POINTER(Calibration)], I),
}
for name, (args, result) in specs.items():
    function = getattr(lib, name)
    function.argtypes, function.restype = args, result
version = subprocess.check_output(['pkg-config', '--modversion', 'lensfun'], text=True).strip()
if version != '0.3.4.0':
    raise RuntimeError('Reference generator requires pinned Lensfun 0.3.4.0')
def generate():
    fixture = ROOT / 'fixture.xml'
    db = lib.lf_db_new()
    assert lib.lf_db_load_file(db, str(fixture).encode()) == 0
    lenses = lib.lf_db_get_lenses(db)
    assert lenses[0] and not lenses[1]
    lens = lenses[0]
    rows = []
    focals = [10, 20, 23.75, 27.5, 35, 43.75, 52.5, 70, 86.25, 102.5, 135, 151.25, 167.5, 200, 240]
    for focal in focals:
        calibration = Calibration()
        assert lib.lf_lens_interpolate_tca(lens, focal, C.byref(calibration))
        for width, height, crop in [(6000, 4000, 1), (4000, 6000, 1.6), (1200, 900, 1), (1024, 1024, 2), (160, 120, 1)]:
            modifier = lib.lf_modifier_new(lens, crop, width, height)
            assert lib.lf_modifier_initialize(modifier, lens, 3, focal, 4, 10, 1, 1, 1, 0) & 1
            cx, cy = (width - 1) / 2, (height - 1) / 2
            for x, y in [(cx, cy), (0, 0), (width-1, height-1), (cx, 0), (cx*.4, cy*1.6), (cx*1.6, cy*.7), (17, 23), (width-19, height-27)]:
                coords = (F * 6)()
                assert lib.lf_modifier_apply_subpixel_distortion(modifier, x, y, 1, 1, coords)
                rows.append(dict(focal=focal, crop=crop, width=width, height=height,
                                 input=[x, y], coefficients=list(calibration.terms), output=list(coords)))
            lib.lf_modifier_destroy(modifier)
    lib.lf_db_destroy(db)
    result = dict(library=version, synthetic=True, fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
                  method='header-defined Lensfun C API; no RapidRoom evaluator used', records=rows)
    (ROOT / 'coordinates.json').write_text(json.dumps(result, separators=(',', ':')) + '\n')
    print(f'Captured {len(rows)} independent channel-coordinate records')


def generate_trip():
    source = ROOT.parents[2] / 'src-tauri/lensfun_db/mil-sony.xml'
    root = E.parse(source).getroot()
    wanted = {'FE 35mm f/1.4 GM (SEL35F14GM)', 'FE 24-70mm f/2.8 GM II'}
    results = []
    with tempfile.TemporaryDirectory(prefix='lensfun-trip-') as directory:
        for element in root.findall('lens'):
            model = element.findtext('model') or ''
            if model not in wanted:
                continue
            file = Path(directory) / 'lens.xml'
            database = E.Element('lensdatabase', version='1')
            database.append(element)
            file.write_bytes(E.tostring(database))
            db = lib.lf_db_new()
            try:
                assert lib.lf_db_load_file(db, str(file).encode()) == 0
                lens = lib.lf_db_get_lenses(db)[0]
                assert lens
                rows = []
                focals = [35] if '35mm' in model else [20, 24, 24.1, 30, 34, 35, 48, 50, 60, 70, 80]
                width, height = 7008, 4672
                for focal in focals:
                    calibration = Calibration()
                    assert lib.lf_lens_interpolate_tca(lens, focal, C.byref(calibration))
                    modifier = lib.lf_modifier_new(lens, 1, width, height)
                    try:
                        assert lib.lf_modifier_initialize(modifier, lens, 3, focal, 4, 10, 1, 1, 1, 0) & 1
                        points = [(0, 0), (width-1, height-1),
                                  ((width-1)/2, (height-1)/2), (width*.2, height*.8)]
                        for x, y in points:
                            coordinates = (F * 6)()
                            assert lib.lf_modifier_apply_subpixel_distortion(
                                modifier, x, y, 1, 1, coordinates)
                            rows.append(dict(focal=focal, width=width, height=height, crop=1,
                                             input=[x, y], coefficients=list(calibration.terms),
                                             output=list(coordinates)))
                    finally:
                        lib.lf_modifier_destroy(modifier)
                results.append(dict(model=model, records=rows))
            finally:
                lib.lf_db_destroy(db)
    assert len(results) == 2
    report = dict(library=version, source_file='src-tauri/lensfun_db/mil-sony.xml',
                  source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                  method='Lensfun public C API on bundled Sony XML; no RapidRoom evaluator',
                  lenses=results)
    (ROOT / 'trip-coordinates.json').write_text(json.dumps(report, indent=2) + '\n')
    print('Captured independent bundled Sony trip-lens records')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--trip', action='store_true', help='Capture bundled Sony trip-lens calibrations')
    args = parser.parse_args()
    generate_trip() if args.trip else generate()
