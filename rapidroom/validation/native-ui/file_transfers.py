"""Real native file transfer commands, paired metadata and visible UI summaries."""
import hashlib
import json
import time
from PIL import Image


def prepare_transfers(case):
    root = case / 'transfers'
    for folder in ['card-a', 'card-b', 'copy', 'import']:
        (root / folder).mkdir(parents=True)
    for folder, color, rating in [('card-a', '#304a66', 2), ('card-b', '#783b28', 4)]:
        path = root / folder / 'A7C01234.jpg'
        Image.new('RGB', (80, 60), color).save(path, quality=90)
        path.with_name(path.name + '.rrdata').write_text(json.dumps({'version': 1, 'rating': rating, 'adjustments': None}))
        path.with_name(path.name + '.vc1.rrdata').write_text(json.dumps({'version': 1, 'rating': rating + 1, 'adjustments': None}))
        path.with_suffix('.XMP').write_text(f'<x:xmpmeta xmlns:x="adobe:ns:meta/">card-{rating}</x:xmpmeta>')
    (case / 'file-transfers-test.json').write_text(json.dumps({'issue': 325, 'owned_synthetic_photos': True}))


def run_transfer_checks(smoke, wait_for):
    root = smoke.case / 'transfers'
    sources = [str(root / c / 'A7C01234.jpg') for c in ['card-a', 'card-b']]
    original = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                for c in ['card-a', 'card-b'] for p in (root / c).iterdir()}

    def invoke(command, args):
        smoke.execute("""window.__transferCall={pending:true};window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1])
          .then(value=>window.__transferCall={value},error=>window.__transferCall={error:String(error)});return true;""", [command, args])
        result = wait_for(lambda: smoke.execute('return window.__transferCall.pending?null:window.__transferCall;'), command)
        assert 'error' not in result, result
        return result.get('value')

    def capture_summary(name, text):
        # Observe the real toast animation without changing layout or styles.
        # Require the matching newest toast wholly visible and stable for 0.75s.
        previous, stable_since = None, None

        def settled():
            nonlocal previous, stable_since
            geometry = smoke.execute("""const e=[...document.querySelectorAll('.Toastify [role="alert"]')]
              .filter(e=>e.innerText.includes(arguments[0])).at(-1);
              if(!e)return null;const r=e.getBoundingClientRect();
              return {text:e.innerText,left:r.left,right:r.right,top:r.top,bottom:r.bottom,
                width:r.width,height:r.height,viewportWidth:innerWidth,viewportHeight:innerHeight,
                animations:e.getAnimations().filter(a=>a.playState==='running'&&
                  a.effect?.getTiming().iterations!==Infinity).length};""", [text])
            if not geometry or geometry['left'] < 0 or geometry['right'] > geometry['viewportWidth'] or geometry['top'] < 0 or geometry['bottom'] > geometry['viewportHeight'] or geometry['animations']:
                previous, stable_since = None, None
                return False
            if geometry != previous:
                previous, stable_since = geometry, time.monotonic()
                return False
            return geometry if time.monotonic() - stable_since >= .75 else False

        geometry = wait_for(settled, name + ' toast fully visible and settled', 12)
        path = smoke.capture(name)
        (smoke.case / (name + '-geometry.json')).write_text(json.dumps(geometry, indent=2))
        return str(path.relative_to(smoke.case))

    def bundle(folder):
        for original_folder, stem in [('card-a', 'A7C01234'), ('card-b', 'A7C01234-2')]:
            for suffix in ['.jpg', '.jpg.rrdata', '.jpg.vc1.rrdata', '.XMP']:
                assert (root / original_folder / ('A7C01234' + suffix)).read_bytes() == (folder / (stem + suffix)).read_bytes(), (folder, stem, suffix)
        assert len(list(folder.iterdir())) == 8

    # Native IPC includes an invalid first source: a later collision and later
    # valid source must still complete, with accurate per-photo counts.
    result = invoke('copy_files', {'sourcePaths': [str(root / 'missing.jpg'), *sources], 'destinationFolder': str(root / 'copy')})
    assert {k: result[k] for k in ['completed', 'renamed', 'identicalSkipped', 'failed']} == {'completed': 2, 'renamed': 1, 'identicalSkipped': 0, 'failed': 1}, result
    bundle(root / 'copy')
    repeated = invoke('copy_files', {'sourcePaths': sources, 'destinationFolder': str(root / 'copy')})
    assert (repeated['completed'], repeated['identicalSkipped'], repeated['failed']) == (0, 2, 0), repeated
    bundle(root / 'copy')
    smoke.step('native copy continues after failure and preserves both colliding bundles', {'first': result, 'reimport': repeated})

    # Exercise the actual keyboard copy/paste handler and its summary toast.
    smoke.execute("""const e=[...document.images].find(e=>e.alt==='smoke.ARW');if(!e)throw Error('Missing thumbnail');
      e.dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));return true;""")
    for key in ['C', 'V']:
        smoke.execute("""document.body.dispatchEvent(new KeyboardEvent('keydown',{
          key:arguments[0].toLowerCase(),code:'Key'+arguments[0],ctrlKey:true,shiftKey:true,bubbles:true,cancelable:true}));return true;""", [key])
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('1 copied (1 renamed), 0 identical skipped, 0 failed');"), 'real paste summary')
    assert (smoke.case / 'input/smoke_copy_1.ARW').read_bytes() == (smoke.case / 'input/smoke.ARW').read_bytes()
    smoke.step('real keyboard paste shows copy summary', {'capture': capture_summary('copy-summary', '1 copied (1 renamed), 0 identical skipped, 0 failed')})

    settings = {'filenameTemplate': '{original_filename}', 'organizeByDate': False, 'dateFolderFormat': 'YYYY/MM/DD', 'deleteAfterImport': False}
    invoke('import_files', {'sourcePaths': sources, 'destinationFolder': str(root / 'import'), 'settings': settings})
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('2 imported (1 renamed), 0 identical skipped, 0 failed');"), 'import collision summary')
    bundle(root / 'import')
    smoke.step('native import preserves colliding bundles and shows summary', {'capture': capture_summary('import-summary', '2 imported (1 renamed), 0 identical skipped, 0 failed')})
    settings['deleteAfterImport'] = True
    invoke('import_files', {'sourcePaths': sources, 'destinationFolder': str(root / 'import'), 'settings': settings})
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('0 imported (0 renamed), 2 identical skipped, 0 failed');"), 'identical import summary')
    bundle(root / 'import')
    # An error must remain an error at completion, rather than being hidden by
    # import-complete. Actual event listeners/toast run; no IPC is intercepted.
    invoke('import_files', {'sourcePaths': [str(root / 'missing.jpg'), *sources], 'destinationFolder': str(root / 'import'), 'settings': settings})
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('0 imported (0 renamed), 2 identical skipped, 1 failed');"), 'partial import error summary')
    assert original == {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                        for c in ['card-a', 'card-b'] for p in (root / c).iterdir()}
    smoke.step('identical skip never deletes source and partial import retains failure summary', {'capture': capture_summary('import-partial-summary', '0 imported (0 renamed), 2 identical skipped, 1 failed'), 'source_bytes_unchanged': True})
    smoke.execute('delete window.__transferCall;return true;')
