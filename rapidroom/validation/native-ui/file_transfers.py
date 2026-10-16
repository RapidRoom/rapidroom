"""Real native file transfer commands, paired metadata and visible UI summaries."""
import hashlib
import json
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
    smoke.step('real keyboard paste shows copy summary', {'capture': str(smoke.capture('copy-summary').relative_to(smoke.case))})

    settings = {'filenameTemplate': '{original_filename}', 'organizeByDate': False, 'dateFolderFormat': 'YYYY/MM/DD', 'deleteAfterImport': False}
    invoke('import_files', {'sourcePaths': sources, 'destinationFolder': str(root / 'import'), 'settings': settings})
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('2 imported (1 renamed), 0 identical skipped, 0 failed');"), 'import collision summary')
    bundle(root / 'import')
    smoke.step('native import preserves colliding bundles and shows summary', {'capture': str(smoke.capture('import-summary').relative_to(smoke.case))})
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
    smoke.step('identical skip never deletes source and partial import retains failure summary', {'capture': str(smoke.capture('import-partial-summary').relative_to(smoke.case)), 'source_bytes_unchanged': True})
    smoke.execute('delete window.__transferCall;return true;')
