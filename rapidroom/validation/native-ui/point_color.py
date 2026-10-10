"""Point Color picking and undo through the native app's public UI."""
import time
from PIL import ImageChops


def run_point_color_checks(smoke, wait_for, baseline):
    def palette_command(query):
        smoke.key('KeyK', 'k', True)
        wait_for(lambda: smoke.execute("return !!document.querySelector('[role=dialog][aria-label=\"Command palette\"]');"), 'Point Color palette opens')
        smoke.execute("""const e=document.querySelector('[role=combobox]');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;""", [query])
        time.sleep(0.2)
        smoke.execute("""document.activeElement.dispatchEvent(new KeyboardEvent('keydown',
          {key:'Enter',code:'Enter',bubbles:true}));return true;""")
        wait_for(lambda: not smoke.execute("return !!document.querySelector('[role=dialog][aria-label=\"Command palette\"]');"), 'Point Color palette commits')

    palette_command('Pick a Point Color on the photo')
    wait_for(lambda: smoke.execute("return !!document.querySelector('button[aria-label=\"Pick a Point Color on the photo\"]');"), 'palette exposes Point Color picker')
    smoke.step('shared palette picker opens Point Color mode')
    time.sleep(0.2)
    x = smoke.roi[0] + (smoke.roi[2]-smoke.roi[0])*0.35
    y = smoke.roi[1] + (smoke.roi[3]-smoke.roi[1])*0.65
    smoke.execute("""const x=arguments[0],y=arguments[1];
      const canvas=[...document.querySelectorAll('.konvajs-content canvas')].reverse().find(e=>{
        const r=e.getBoundingClientRect();return r.left<=x&&r.right>=x&&r.top<=y&&r.bottom>=y;});
      if(!canvas)throw Error('Point Color canvas unavailable');
      for(const type of ['mousedown','mouseup'])canvas.dispatchEvent(new MouseEvent(type,{bubbles:true,button:0,
        buttons:type==='mousedown'?1:0,clientX:x,clientY:y}));return true;""", [x,y])
    wait_for(lambda: smoke.execute("return !!document.querySelector('button[aria-label=\"Select Point Color 1\"]');"), 'image picker adds Point Color', 60)
    swatch = smoke.stable_preview('point-color-zero')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(swatch)).getbbox() is not None:
        raise RuntimeError('Zero-shift Point Color changed preview pixels')
    smoke.step('image picker adds zero-shift swatch without changing pixels')
    smoke.drag('Hue Range', 100)
    smoke.drag('Saturation Range', 0.3)
    smoke.drag('Luminance Range', 0.6)
    palette_command('point color hue shift 70')
    wait_for(lambda: smoke.slider('Hue Shift')['value']==70, 'palette shifts selected Point Color')
    edited = smoke.stable_preview('point-color-shift')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(edited)).getbbox() is None:
        raise RuntimeError('Point Color Hue Shift did not change preview')
    smoke.step('Point Color shifts selected range in real render')
    smoke.execute("""const e=document.querySelector('button[data-tooltip^="Undo ("]');if(!e||e.disabled)throw Error('Undo unavailable');e.click();return true;""")
    wait_for(lambda: smoke.slider('Hue Shift')['value']==0, 'Point Color Undo restores shift')
    undone = smoke.stable_preview('point-color-undo')
    if ImageChops.difference(smoke.crop(baseline),smoke.crop(undone)).getbbox() is not None:
        raise RuntimeError('One Undo failed to restore Point Color pixels')
    smoke.step('one Undo restores Point Color shift and exact pixels')
    palette_command('Delete selected Point Color')
    wait_for(lambda: not smoke.execute("return !!document.querySelector('button[aria-label=\"Select Point Color 1\"]');"), 'palette removes selected Point Color')
    smoke.step('shared palette removes selected Point Color')
    smoke.execute("""[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='HSL').click();return true;""")
    time.sleep(0.3)
