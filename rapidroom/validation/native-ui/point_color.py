"""Point Color picking and undo through the native app's public UI."""
import time
from PIL import ImageChops


def run_point_color_checks(smoke, wait_for, baseline):
    smoke.execute("""const button=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Point Color');
      if(!button)throw Error('Point Color mode missing');button.scrollIntoView({block:'center'});button.click();return true;""")
    time.sleep(0.3)
    smoke.execute("""const e=document.querySelector('button[aria-label="Pick a Point Color on the photo"]');
      if(!e)throw Error('Point Color picker missing');e.click();return true;""")
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
    smoke.drag('Hue Shift', 70)
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
    smoke.execute("""document.querySelector('button[aria-label="Delete selected Point Color"]').click();
      [...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='HSL').click();return true;""")
    time.sleep(0.3)
