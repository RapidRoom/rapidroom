"""Keyboard-only command-palette scenario in the native WebKit app."""
import time
from PIL import Image, ImageChops, ImageDraw, ImageStat


def _photo_only(image, roi, dialog):
    image = image.copy()
    visible = Image.new('L', image.size, 255)
    left,top,right,bottom = dialog
    excluded = [left-roi[0],top-roi[1],right-roi[0],bottom-roi[1]]
    ImageDraw.Draw(image).rectangle(excluded, fill=0)
    ImageDraw.Draw(visible).rectangle(excluded, fill=0)
    return image, visible


def _require_visible_photo(image, visible):
    if visible.histogram()[255] < 256:
        raise RuntimeError('Palette leaves insufficient unobscured photo area to verify')
    stats = ImageStat.Stat(image, visible)
    if max(stats.stddev) < 10 or max(stats.mean) < 10:
        raise RuntimeError('Unobscured palette preview is blank or untextured')


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
    type_query('shadows 20')
    key('Enter')
    wait_for(lambda: not shown() and smoke.slider('Shadows')['value']==20, 'typed Basic Shadows wins exact match')
    undo()
    time.sleep(0.3)
    shadows_undo = smoke.stable_preview('palette-shadows-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(shadows_undo)).getbbox() is not None:
        raise RuntimeError('Basic Shadows Undo failed to restore preview')
    smoke.step('typed Shadows targets Basic control and Undo restores pixels')

    open_palette()
    type_query('parametricCurve.luma.shadows 20')
    key('Enter')
    wait_for(lambda: not shown(), 'typed parametric curve')
    parametric = smoke.stable_preview('palette-parametric')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(parametric)).getbbox() is None:
        raise RuntimeError('Parametric slider failed to change rendered curve')
    undo()
    time.sleep(0.3)
    parametric_undo = smoke.stable_preview('palette-parametric-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(parametric_undo)).getbbox() is not None:
        raise RuntimeError('Parametric Undo failed to restore preview')
    smoke.step('parametric curve updates real render and one Undo restores pixels')

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
    # The focused input caret blinks inside the photo ROI. Compare only the
    # unobscured photo, excluding the dialog and its painted shadow from BOTH
    # images so opening the palette cannot masquerade as a rendered preview.
    dialog = smoke.execute("""const e=document.querySelector('[role=dialog][aria-label="Command palette"]');
      if(!e)throw Error('Preview dialog missing');const r=e.getBoundingClientRect();
      return [r.left-100,r.top-100,r.right+100,r.bottom+100];""")
    before, visible = _photo_only(smoke.crop(baseline), smoke.roi, dialog)
    _require_visible_photo(before, visible)
    previous = None
    for attempt in range(12):
        time.sleep(0.5)
        preview, _ = _photo_only(smoke.crop(smoke.capture('palette-slider-preview-' + str(attempt))), smoke.roi, dialog)
        if previous is not None and ImageChops.difference(previous,preview).getbbox() is None:
            _require_visible_photo(preview, visible)
            break
        previous = preview
    else:
        raise RuntimeError('Unobscured palette preview did not settle')
    if smoke.slider('Exposure')['value']!=0 or ImageChops.difference(before,preview).getbbox() is None:
        raise RuntimeError('Keyboard preview changed edits or did not render outside the dialog')
    smoke.step('keyboard preview changes real pixels outside dialog', {'excluded_dialog_shadow':dialog})
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
