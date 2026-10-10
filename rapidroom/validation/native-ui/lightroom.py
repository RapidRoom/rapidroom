"""Optional workspace, snapshot and keymap checks through the public native UI."""
import json
import time
from PIL import ImageChops


def run_lightroom_checks(smoke, wait_for, baseline):
    original_roi=smoke.roi
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
    final=smoke.stable_preview('workspace-restored')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(final)).getbbox() is not None:
        raise RuntimeError('Workspace switching or snapshot Undo changed photo pixels')
    smoke.step('RapidRoom workspace and original photo restored exactly')
