"""Public controls only: local ICC validation, proof view and tagged GUI export."""
import hashlib
import json
import time
from PIL import ImageChops, Image


def field_script(export=False):
    label = 'Convert to chosen ICC profile' if export else 'Preview printer/paper colours'
    return "const f=[...document.querySelectorAll('fieldset')].find(e=>e.innerText.includes(" + json.dumps(label) + "));if(!f)throw Error('ICC controls missing');"


def choose(smoke, wait_for, path, export=False, invalid=False):
    scope = field_script(export)
    smoke.execute(scope + "const e=f.querySelector('input[aria-label=\"ICC profile path\"]');"
                  "Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);"
                  "e.dispatchEvent(new Event('input',{bubbles:true}));return true;", [str(path)])
    time.sleep(0.1)
    smoke.execute(scope + "[...f.querySelectorAll('button')].find(e=>e.innerText==='Use profile').click();return true;")
    if invalid:
        wait_for(lambda: smoke.execute(scope + "return f.querySelector('[role=alert]')?.textContent;"), 'visible ICC validation error')
    else:
        wait_for(lambda: smoke.execute(scope + "return !f.disabled && !f.querySelector('input[type=checkbox]').disabled && !f.querySelector('[role=alert]');"), 'ICC profile validated')


def toggle(smoke, export=False):
    smoke.execute(field_script(export) + "f.querySelector('input[type=checkbox]').click();return true;")


def checked(smoke, export=False):
    return smoke.execute(field_script(export) + "return f.querySelector('input[type=checkbox]').checked;")


def open_menu(smoke):
    smoke.execute("const e=document.querySelector('[data-testid=soft-proof-menu]');if(!e)throw Error('Proof menu absent');e.closest('details').open=true;return true;")


def close_menu(smoke):
    smoke.execute("document.querySelector('[data-testid=soft-proof-menu]').closest('details').open=false;return true;")


def preview_src(smoke):
    return smoke.execute("return [...document.querySelectorAll('[data-editor-viewport] img')].filter(e=>e.src.startsWith('blob:')).map(e=>e.src).join('|');")


def run_proof_checks(smoke, wait_for, baseline):
    metadata = smoke.case / 'input/smoke.ARW.rrdata'
    before = metadata.read_bytes() if metadata.exists() else None
    undo_disabled = smoke.execute("return document.querySelector('button[data-tooltip^=\"Undo (\"]')?.disabled;")
    if not undo_disabled:
        raise RuntimeError('Proof scenario requires initial empty Undo history')
    open_menu(smoke)
    if checked(smoke):
        raise RuntimeError('Soft proof must default off')
    choose(smoke, wait_for, smoke.case / 'missing.icc', invalid=True)
    if checked(smoke):
        raise RuntimeError('Invalid profile enabled proofing')
    smoke.capture('proof-invalid-profile')
    choose(smoke, wait_for, smoke.case / 'profiles/proof.icc')
    if checked(smoke):
        raise RuntimeError('Choosing a profile must not enable proofing')
    old_src = preview_src(smoke)
    toggle(smoke)
    wait_for(lambda: checked(smoke), 'explicit proof enabled')
    wait_for(lambda: bool(preview_src(smoke)) and preview_src(smoke) != old_src, 'new proofed preview')
    close_menu(smoke)
    proofed = smoke.stable_preview('proof-enabled')
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(proofed)).getbbox() is None:
        raise RuntimeError('Printer profile did not visibly change preview')
    smoke.step('printer ICC proof visibly changes native preview', {'capture':str(proofed.relative_to(smoke.case))})
    open_menu(smoke)
    old_src = preview_src(smoke)
    toggle(smoke)
    wait_for(lambda: not checked(smoke), 'proof disabled')
    wait_for(lambda: bool(preview_src(smoke)) and preview_src(smoke) != old_src, 'unproofed preview restored')
    close_menu(smoke)
    restored = smoke.stable_preview('proof-disabled')
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('Disabling proof did not restore exact preview pixels')
    after = metadata.read_bytes() if metadata.exists() else None
    if before != after:
        raise RuntimeError('Proof changed photo sidecar')
    if not smoke.execute("return document.querySelector('button[data-tooltip^=\"Undo (\"]')?.disabled;"):
        raise RuntimeError('Proof entered edit history')
    smoke.step('proof off exactly restores pixels without sidecar or Undo change', {'sidecar_sha256':hashlib.sha256(after).hexdigest() if after is not None else None,'sidecar_presence_unchanged':True})


def prepare_export(smoke, wait_for):
    if checked(smoke, True):
        raise RuntimeError('ICC export must default off')
    choose(smoke, wait_for, smoke.case / 'profiles/output.icc', export=True)
    if checked(smoke, True):
        raise RuntimeError('Choosing output ICC must require enable')
    toggle(smoke, True)
    wait_for(lambda: checked(smoke, True), 'ICC export explicitly enabled')
    smoke.execute("const e=document.querySelector('#switch-save-with-metadata');if(!e)throw Error('Metadata switch missing');if(e.checked)e.click();return true;")
    smoke.step('GUI export selects and enables a local RGB profile with metadata off')


def check_export(smoke, path):
    with Image.open(path) as image:
        if image.info.get('icc_profile') != (smoke.case/'profiles/output.icc').read_bytes():
            raise RuntimeError('GUI JPEG embedded profile differs from chosen conversion profile')
    smoke.step('GUI JPEG embeds exact chosen RGB profile bytes')
