"""Optional workspace, snapshot and keymap checks through the public native UI."""
import json
import re
import time
from PIL import ImageChops


def run_lightroom_checks(smoke, wait_for, baseline):
    original_roi=smoke.roi

    def preview_size():
        sizes=re.findall(r'\[process_preview_job\] full (\d+)x(\d+)', (smoke.case / 'app.log').read_text())
        if not sizes:
            raise RuntimeError('Processed preview dimensions unavailable')
        return tuple(map(int,sizes[-1]))

    baseline_size=preview_size()
    settings_path = smoke.case / 'data/io.github.CyberTimon.RapidRAW/settings.json'

    def settings():
        smoke.key('Comma', ',', True)
        wait_for(lambda: smoke.execute('return !!document.querySelector("select[aria-label=\\"Workspace preset\\"]");'), 'workspace settings visible')

    def choose(value):
        smoke.execute('''const select=document.querySelector('select[aria-label="Workspace preset"]');
          Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(select,arguments[0]);
          select.dispatchEvent(new Event('change',{bubbles:true}));return true;''',[value])
        wait_for(lambda: json.loads(settings_path.read_text()).get('workspacePreset') == value, 'workspace preset persisted')

    settings()
    original=json.loads(settings_path.read_text())['workspace']
    choose('lightroom')
    smoke.execute('''const e=document.querySelector('#switch-lightroom-shortcuts');
      if(!e.checked)e.click();return true;''')
    smoke.key('Escape','Escape')
    wait_for(lambda: smoke.execute('return !!document.querySelector("[data-workspace=lightroom-sidebar]");'), 'Develop sidebar visible')
    wait_for(lambda: smoke.execute('return !!document.querySelector("select[aria-label=Treatment]");'), 'Basic controls visible')
    smoke.roi=tuple(round(v) for v in smoke.execute("""const e=[...document.querySelectorAll('svg')].filter(e=>e.getBoundingClientRect().width>500 && e.getBoundingClientRect().height>300)[0];
      if(!e)throw Error('Lightroom preview bounds missing');const r=e.getBoundingClientRect();return [r.left,r.top,r.right,r.bottom];"""))
    smoke.stable_preview('lightroom-develop')
    smoke.capture('lightroom-develop-full')
    smoke.step('Lightroom workspace shows existing grouped controls, Navigator and real History')

    smoke.drag('Exposure',1)
    exposure=smoke.slider('Exposure')['value']
    edited=smoke.stable_preview('lightroom-exposure')
    smoke.execute('''const summary=[...document.querySelectorAll('summary')].find(e=>e.textContent==='Snapshots');
      if(!summary.parentElement.open)summary.click();
      const e=document.querySelector('input[aria-label="Snapshot name"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Native warm');
      e.dispatchEvent(new Event('input',{bubbles:true}));return true;''')
    smoke.execute('''const input=document.querySelector('input[aria-label="Snapshot name"]');
      const button=input.closest('form').querySelector('button');if(button.disabled)throw Error('Snapshot save unavailable');button.click();return true;''')
    wait_for(lambda: smoke.execute('return [...document.querySelectorAll("button")].some(e=>e.textContent==="Native warm");'), 'snapshot saved')
    smoke.key('KeyZ','z',True)
    wait_for(lambda: smoke.slider('Exposure')['value']==0,'snapshot creation does not add edit history')
    smoke.execute('''[...document.querySelectorAll('button')].find(e=>e.textContent==='Native warm').click();return true;''')
    wait_for(lambda: abs(smoke.slider('Exposure')['value']-exposure)<.001,'snapshot restored')
    restored=smoke.stable_preview('snapshot-restored')
    if ImageChops.difference(smoke.crop(edited),smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('Restored snapshot changed rendered pixels')
    smoke.key('KeyZ','z',True)
    wait_for(lambda: smoke.slider('Exposure')['value']==0,'snapshot restoration has one Undo')
    smoke.execute('''document.querySelector('button[aria-label="Delete snapshot Native warm"]').click();return true;''')
    wait_for(lambda: smoke.execute('return !document.querySelector("button[aria-label=\\"Delete snapshot Native warm\\"]");'), 'snapshot deleted')
    smoke.step('Snapshot save, restore exact pixels, one Undo and delete')

    smoke.key('KeyK','k',True)
    wait_for(lambda: smoke.execute("return !!document.querySelector('[role=dialog][aria-label=\"Command palette\"]');"), 'Lightroom command palette opens')
    smoke.execute("""const e=document.querySelector('[role=combobox]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'Before / After');
      e.dispatchEvent(new Event('input',{bubbles:true}));return true;""")
    time.sleep(.2)
    smoke.execute("""document.activeElement.dispatchEvent(new KeyboardEvent('keydown',
      {key:'Enter',code:'Enter',bubbles:true}));return true;""")
    wait_for(lambda: smoke.execute('return document.body.innerText.includes("Before (Original)");'), 'palette shares Before/After action')
    smoke.key('KeyY','y')
    wait_for(lambda: not smoke.execute('return document.body.innerText.includes("Before (Original)");'), 'optional key exits palette comparison')
    smoke.step('Lightroom palette and optional key share Before/After action')

    smoke.key('KeyY','y')
    wait_for(lambda: smoke.execute('return document.body.innerText.includes("Before (Original)");'),'Y opens before/after')
    smoke.capture('lightroom-before-after')
    smoke.key('KeyY','y')
    wait_for(lambda: smoke.execute('return !document.body.innerText.includes("Before (Original)");'),'Y exits before/after')
    smoke.key('KeyG','g')
    wait_for(lambda: smoke.execute('return document.body.innerText.includes("Quick Develop");'),'G opens Library')
    smoke.capture('lightroom-library')
    smoke.key('KeyD','d')
    wait_for(lambda: smoke.execute('return !!document.querySelector("[data-workspace=lightroom-sidebar]");'),'D opens Develop')
    smoke.step('Optional module and Before/After keys work through public UI')

    settings()
    choose('rapidroom')
    smoke.execute('''const e=document.querySelector('#switch-lightroom-shortcuts');if(e.checked)e.click();return true;''')
    smoke.key('Escape','Escape')
    wait_for(lambda: not smoke.execute('return !!document.querySelector("[data-workspace=lightroom-sidebar]");'),'original workspace restored')
    current=json.loads(settings_path.read_text())['workspace']
    for key in ['leftPanelWidth','rightPanelWidth','panelLayout','panelSwitcherPlacement','bottomPanelHeight']:
        if current.get(key) != original.get(key):
            raise RuntimeError('Workspace did not restore '+key)
    smoke.roi=original_roi
    # Resizing/module changes retain the larger HiFi preview until an edit.
    # A public Exposure edit and one Undo restore the same fit resolution without
    # resetting any other adjustment; hidden changes still fail the pixel check.
    restored_roi=tuple(round(v) for v in smoke.execute("""const e=[...document.querySelectorAll('svg')].filter(e=>e.getBoundingClientRect().width>500 && e.getBoundingClientRect().height>300)[0];
      if(!e)throw Error('Restored photo unavailable');const r=e.getBoundingClientRect();return [r.left,r.top,r.right,r.bottom];"""))
    if restored_roi != original_roi:
        raise RuntimeError('Workspace did not restore photo bounds')
    wait_for(lambda: smoke.slider('Exposure')['value']==0, 'snapshot Undo retained original Exposure')
    smoke.drag('Exposure',0.5)
    smoke.stable_preview('workspace-fit-edit')
    smoke.execute("""const e=document.querySelector('button[data-tooltip^=\"Undo (\"]');
      if(!e||e.disabled)throw Error('Workspace normalization Undo unavailable');e.click();return true;""")
    wait_for(lambda: smoke.slider('Exposure')['value']==0, 'one Undo restores fit normalization')
    final=smoke.stable_preview('workspace-restored')
    if preview_size() != baseline_size:
        raise RuntimeError('Workspace comparison uses different preview dimensions')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(final)).getbbox() is not None:
        raise RuntimeError('Workspace switching or snapshot Undo changed photo pixels')
    smoke.step('RapidRoom workspace and original photo restored exactly')
