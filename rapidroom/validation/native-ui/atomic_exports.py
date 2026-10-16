"""Real GUI default exports and explicit native overwrite, without mocked IO."""
import hashlib
import json
import threading
from PIL import Image


def observe_atomic_exports(smoke):
    smoke.execute("""window.__atomicExportOriginalInvoke=window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke=function(...args){
        if(args[0]==='export_images')window.__atomicExportArgs=JSON.parse(JSON.stringify(args[1]));
        return window.__atomicExportOriginalInvoke.apply(this,args);
      };return true;""")


def run_atomic_export_checks(smoke, wait_for, first):
    folder = first.parent
    original = hashlib.sha256(first.read_bytes()).hexdigest()
    before = set(folder.glob('*.jpg'))
    observations, failures, seen = [], [], set()
    stop = threading.Event()

    def watch():
        while not stop.is_set():
            for path in folder.glob('*.jpg'):
                try:
                    stat = path.stat()
                    identity = (path.name, stat.st_ino, stat.st_size, stat.st_mtime_ns)
                    if identity in seen:
                        continue
                    seen.add(identity)
                    with Image.open(path) as image:
                        image.load()
                        assert image.format == 'JPEG' and image.size == (4608, 3072), identity
                    observations.append({'name': path.name, 'inode': stat.st_ino, 'size': stat.st_size})
                except Exception as error:
                    failures.append(str(error))
            stop.wait(.01)

    worker = threading.Thread(target=watch, daemon=True)
    worker.start()
    try:
        args = smoke.execute('return window.__atomicExportArgs;')
        assert args and args['exportSettings'].get('overwriteExisting', False) is False, args
        assert all(str(smoke.case / 'input') in source for source in args['paths']), args['paths']
        smoke.execute("""const e=[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Export Image');
          if(!e||e.disabled)throw Error('Export Image unavailable');e.click();return true;""")
        added = wait_for(lambda: set(folder.glob('*.jpg')) - before, 'second GUI export', 90)
        assert len(added) == 1
        second = next(iter(added))
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.innerText.trim()==='Export Image'&&!e.disabled);"), 'second export finished')
        assert hashlib.sha256(first.read_bytes()).hexdigest() == original
        with Image.open(second) as image:
            image.load()
            assert image.format == 'JPEG' and image.size == (4608, 3072)
        smoke.step('default GUI re-export preserves existing file and publishes a complete unique JPEG', {'first': first.name, 'second': second.name, 'capture': str(smoke.capture('atomic-export-renamed').relative_to(smoke.case))})
        second_hash = hashlib.sha256(second.read_bytes()).hexdigest()
        old_inode = first.stat().st_ino
        # An explicit owned-fixture overwrite request, using the exact settings
        # emitted by the real Export panel. All native work/IO remains real.
        args['exportSettings']['overwriteExisting'] = True
        smoke.execute("""window.__atomicOverwrite={pending:true};window.__TAURI_INTERNALS__.invoke('export_images',arguments[0])
          .then(value=>window.__atomicOverwrite={value},error=>window.__atomicOverwrite={error:String(error)});return true;""", [args])
        invocation = wait_for(lambda: smoke.execute('return window.__atomicOverwrite.pending?null:window.__atomicOverwrite;'), 'native overwrite accepted')
        assert 'error' not in invocation, invocation
        wait_for(lambda: first.stat().st_ino != old_inode, 'atomic overwrite replaced inode', 90)
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.innerText.trim()==='Export Image'&&!e.disabled);"), 'native overwrite finished')
        with Image.open(first) as image:
            image.load()
            assert image.format == 'JPEG' and image.size == (4608, 3072)
        assert len(list(folder.glob('*.jpg'))) == len(before) + 1
        assert hashlib.sha256(second.read_bytes()).hexdigest() == second_hash
        assert not list(folder.glob('.rapidroom-export-*.tmp'))
        wait_for(lambda: len(observations) >= 3 or failures, 'observer saw all three complete export states')
        smoke.step('explicit native overwrite atomically replaces one export and preserves its sibling', {'file': first.name, 'old_inode': old_inode, 'new_inode': first.stat().st_ino, 'capture': str(smoke.capture('atomic-export-overwrite').relative_to(smoke.case))})
    finally:
        stop.set()
        worker.join(timeout=15)
        smoke.execute("""window.__TAURI_INTERNALS__.invoke=window.__atomicExportOriginalInvoke;
          delete window.__atomicExportOriginalInvoke;delete window.__atomicExportArgs;delete window.__atomicOverwrite;return true;""")
    assert not worker.is_alive()
    assert not failures, failures
    assert len(observations) >= 3, observations
    (smoke.case / 'atomic-export-observations.json').write_text(json.dumps({'passed': True, 'observations': observations, 'invalid_final_files': failures}, indent=2))
    smoke.step('all observed final JPEG states were complete and decodable', {'observations': len(observations), 'invalid': 0})
