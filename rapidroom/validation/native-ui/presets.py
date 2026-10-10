"""Grouped browser, hover, Amount and preset creation through public UI."""
import time
from PIL import ImageChops


def run_preset_checks(smoke, wait_for, baseline):
    def shortcut(code, key, ctrl=False, shift=False):
        smoke.execute("""document.activeElement?.blur();
          document.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,cancelable:true,
            code:arguments[0],key:arguments[1],ctrlKey:arguments[2],shiftKey:arguments[3]}));return true;""", [code,key,ctrl,shift])

    shortcut('KeyN','N',True,True)
    wait_for(lambda: smoke.execute("return !!document.querySelector('[role=dialog] input');"), 'Ctrl+Shift+N opens new preset dialog from Develop')
    smoke.capture('preset-create-dialog')
    shortcut('Escape','Escape')
    # The modal handles keydown on its input/container, so activate its close button.
    smoke.execute("""const dialog=document.querySelector('[role=dialog]');
      const button=dialog && [...dialog.querySelectorAll('button')].find(e=>e.textContent.trim()==='Cancel');
      if(button)button.click();return true;""")
    wait_for(lambda: smoke.execute("return !document.querySelector('[role=dialog]');"), 'preset dialog cancels')
    wait_for(lambda: smoke.execute("return !!document.querySelector('input[aria-label=\"Search presets or groups\"]');"), 'preset browser visible')

    def search(query):
        smoke.execute("""const e=document.querySelector('input[aria-label="Search presets or groups"]');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;""",[query])
        time.sleep(.2)

    search('Color')
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('Warm') && !document.body.innerText.includes('Green Filter');"), 'group search filters browser')
    search('Punch')
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=button]')].some(e=>e.textContent.trim()==='Punch');"), 'name search expands group')
    smoke.execute("""const row=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='Punch');
      row.scrollIntoView({block:'center'});row.dispatchEvent(new MouseEvent('mouseover',{bubbles:true}));return true;""")
    hovered=smoke.stable_preview('preset-hover')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(hovered)).getbbox() is None:
        raise RuntimeError('Preset hover did not change photo')
    smoke.execute("""const row=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='Punch');
      row.dispatchEvent(new MouseEvent('mouseout',{bubbles:true,relatedTarget:document.body}));return true;""")
    restored=smoke.stable_preview('preset-hover-cancel')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('Preset hover cancellation changed pixels')
    smoke.step('group/name search and owned hover preview cancel exactly')
    smoke.execute("""const row=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='Punch');row.click();return true;""")
    applied=smoke.stable_preview('preset-applied')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(applied)).getbbox() is None:
        raise RuntimeError('Preset apply did not change pixels')
    smoke.execute("""document.querySelector('button[data-tooltip^="Undo ("]').click();return true;""")
    restored=smoke.stable_preview('preset-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('One Undo failed to restore preset pixels')
    smoke.step('preset click creates one Undo step restoring exact pixels')
    smoke.execute("""const row=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='Punch');row.click();return true;""")
    wait_for(lambda: smoke.slider('Amount')['value']==100, 'preset Amount starts at100')
    smoke.drag('Amount',0)
    restored=smoke.stable_preview('preset-amount-zero')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('Preset Amount0 does not restore before-preset pixels')
    smoke.step('preset Amount0 restores before-preset pixels')
    smoke.execute("""document.querySelector('[data-tab-id="layout-tab-adjustments"]').click();return true;""")
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('Exposure');"), 'return to Develop controls')
