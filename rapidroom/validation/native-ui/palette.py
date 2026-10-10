"""Keyboard-only command-palette scenario in the native WebKit app."""
import time
from PIL import ImageChops


def run_palette_checks(smoke, wait_for, baseline):
    def shown():
        return smoke.execute("return !!document.querySelector('[role=dialog][aria-label=\"Command palette\"]');")

    def open_palette():
        smoke.key('KeyK', 'k', True)
        wait_for(shown, 'Ctrl+K opens palette')

    def type_query(value):
        smoke.execute("""const e=document.querySelector('[role=combobox]');
          if(!e)throw Error('Palette input missing');e.focus();
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;""", [value])
        time.sleep(0.1)

    def key(value, shift=False):
        smoke.execute("""document.activeElement.dispatchEvent(new KeyboardEvent('keydown',
          {key:arguments[0],code:arguments[0],shiftKey:arguments[1],bubbles:true}));return true;""", [value, shift])

    def undo():
        smoke.execute("""const e=document.querySelector('button[data-tooltip^="Undo ("]');
          if(!e||e.disabled)throw Error('Undo unavailable');e.click();return true;""")

    open_palette()
    type_query('exposure 0.7')
    key('Enter')
    wait_for(lambda: not shown() and abs(smoke.slider('Exposure')['value']-0.7)<0.001, 'typed palette Exposure')
    edited = smoke.stable_preview('palette-exposure')
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(edited)).getbbox() is None:
        raise RuntimeError('Palette Exposure did not change real preview')
    smoke.step('keyboard palette Exposure 0.7', {'slider': smoke.slider('Exposure')})
    undo()
    wait_for(lambda: smoke.slider('Exposure')['value']==0, 'one Undo restores palette Exposure')
    restored = smoke.stable_preview('palette-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('One palette Undo did not restore exact preview')
    smoke.step('one Undo restores palette edit and pixels')

    open_palette()
    type_query('presence')
    aliases = smoke.execute("return [...document.querySelectorAll('[role=option]')].map(e=>e.textContent);")
    if not any('Clarity' in title for title in aliases) or not any('Structure' in title for title in aliases):
        raise RuntimeError('Lightroom Presence aliases missing: ' + str(aliases))
    type_query('color mixer')
    mixer = smoke.execute("return [...document.querySelectorAll('[role=option]')].map(e=>e.textContent);")
    if not any('Orange Hue' in title for title in mixer):
        raise RuntimeError('Color Mixer alias missing: ' + str(mixer))
    smoke.step('Lightroom Presence and Color Mixer names', {'presence':aliases,'mixer':mixer})
    type_query('exposure')
    key('Enter')
    for _ in range(4):
        key('ArrowRight', True)
    time.sleep(0.2)
    preview = smoke.stable_preview('palette-slider-preview')
    if smoke.slider('Exposure')['value']!=0 or ImageChops.difference(smoke.crop(baseline),smoke.crop(preview)).getbbox() is None:
        raise RuntimeError('Keyboard preview changed edits or did not render')
    key('Escape')
    key('Escape')
    wait_for(lambda: not shown(), 'palette preview cancels')
    cancelled = smoke.stable_preview('palette-slider-cancel')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(cancelled)).getbbox() is not None:
        raise RuntimeError('Escape failed to restore preview pixels')
    smoke.step('keyboard slider preview and Escape restore without edit')

    open_palette()
    type_query('crop 4:3')
    key('Enter')
    wait_for(lambda: not shown(), 'typed crop command')
    crop = smoke.stable_preview('palette-crop')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(crop)).getbbox() is None:
        raise RuntimeError('Typed crop did not change preview')
    undo()
    time.sleep(0.3)
    uncropped = smoke.stable_preview('palette-crop-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(uncropped)).getbbox() is not None:
        raise RuntimeError('One Undo did not restore typed crop')
    smoke.step('typed crop 4:3 and one Undo')
