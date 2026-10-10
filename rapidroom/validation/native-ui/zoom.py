"""Exercise native zoom handlers without accessing React stores."""
import time


def run_zoom_checks(smoke, wait_for):
    def read():
        return smoke.execute("""const e=[...document.querySelectorAll('[data-tooltip]')]
          .find(e=>e.textContent.trim().match(/^\\d+%$/));
          if(!e)throw Error('Zoom readout missing');
          const v=document.querySelector('[data-editor-viewport]'),r=v.getBoundingClientRect();
          return {percent:Number(e.textContent.trim().slice(0,-1)),width:r.width,height:r.height,
            transform:v.firstElementChild?.style.transform,dpr:devicePixelRatio};""")

    def percent(value):
        smoke.execute("""const e=[...document.querySelectorAll('[data-tooltip]')]
          .find(e=>e.textContent.trim().match(/^\\d+%$/));e.click();return true;""")
        time.sleep(0.1)
        smoke.execute("""const e=document.activeElement;
          if(e.tagName!=='INPUT')throw Error('Zoom input not focused');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,String(arguments[0]));
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;""", [value])
        time.sleep(0.1)
        smoke.execute("""document.activeElement.dispatchEvent(new KeyboardEvent('keydown',
          {key:'Enter',code:'Enter',bubbles:true}));return true;""")

    def at(value):
        return wait_for(lambda: (r if (r := read())["percent"] == value else None), f"zoom {value}%")

    def wheel(delta, ctrl=False):
        smoke.execute("""const e=document.querySelector('[data-editor-viewport]'),r=e.getBoundingClientRect();
          e.dispatchEvent(new WheelEvent('wheel',{bubbles:true,cancelable:true,deltaMode:0,
            deltaY:arguments[0],ctrlKey:arguments[1],clientX:r.left+r.width/2,clientY:r.top+r.height/2}));return true;""", [delta, ctrl])
        time.sleep(0.2)

    def native_notch(direction, expected):
        before = read()
        point = smoke.execute("""const v=document.querySelector('[data-editor-viewport]'),r=v.getBoundingClientRect();
          const x=r.left+r.width*0.55,y=r.top+r.height*0.55;
          window.__RR_NATIVE_WHEEL__=null;
          v.addEventListener('wheel',e=>{window.__RR_NATIVE_WHEEL__={trusted:e.isTrusted,
            deltaY:e.deltaY,deltaMode:e.deltaMode,wheelDeltaY:e.wheelDeltaY,innerHeight,
            clientX:e.clientX,clientY:e.clientY};},{once:true});
          const m=new DOMMatrix(v.firstElementChild.style.transform);
          return {x,y,anchor:[(x-r.left-m.m41)/m.a,(y-r.top-m.m42)/m.d]};""")
        command = smoke.case / 'native-wheel.command'
        temporary = command.with_suffix('.tmp')
        temporary.write_text(f"{direction} {point['x']} {point['y']}\n")
        temporary.replace(command)
        event = wait_for(lambda: smoke.execute('return window.__RR_NATIVE_WHEEL__;'), 'trusted native wheel event')
        if not event['trusted'] or event['deltaMode'] != 0:
            raise RuntimeError('GTK wheel did not pass through native conversion: ' + str(event))
        result = at(expected)
        after_anchor = smoke.execute("""const v=document.querySelector('[data-editor-viewport]'),r=v.getBoundingClientRect();
          const m=new DOMMatrix(v.firstElementChild.style.transform);
          return [(arguments[0]-r.left-m.m41)/m.a,(arguments[1]-r.top-m.m42)/m.d];""", [point['x'], point['y']])
        error = max(abs(a-b) for a,b in zip(point['anchor'],after_anchor))
        if error > 0.02:
            raise RuntimeError('Native wheel moved cursor anchor: ' + str(error))
        smoke.step(f'native {direction} wheel adjacent stop {expected} percent', {
            'before': before, 'after': result, 'native_event': event, 'anchor_error_css_px': error})

    percent(100)
    at(100)
    native_notch('u', 150)
    native_notch('d', 100)
    percent(100)
    at(100)
    wheel(-100)
    smoke.step('wheel notch 100 to 150 percent', at(150))
    for _ in range(12):
        wheel(-100)
    smoke.step('wheel ceiling 400 percent', at(400))
    wheel(-100, True)
    at(400)
    percent(10000)
    smoke.step('pinch and typed zoom share 400 percent ceiling', at(400))
    percent(100)
    at(100)
    wheel(-10, True)
    smooth = read()
    if not 100 < smooth['percent'] < 150:
        raise RuntimeError('Pinch unexpectedly snapped: ' + str(smooth))
    smoke.step('pinch remains continuous', smooth)
    percent(100)
    at(100)
    wheel(-10)
    fine = read()
    if not 100 < fine['percent'] < 150:
        raise RuntimeError('Fine scroll unexpectedly snapped: ' + str(fine))
    smoke.step('fine pixel scroll remains continuous', fine)
    percent(100)
    at(100)
    smoke.key('ArrowUp', 'ArrowUp')
    smoke.step('keyboard step 100 to 150 percent', at(150))
    percent(100)
    at(100)
    for _ in range(3):
        smoke.key('ArrowUp', 'ArrowUp')
    smoke.step('rapid keyboard steps do not wait for readout', at(300))
    for _ in range(8):
        smoke.key('Equal', '=', ctrl=True)
        time.sleep(0.3)
    smoke.step('keyboard ceiling 400 percent', at(400))
    slider = smoke.slider('Zoom')
    if slider['max'] != 4:
        raise RuntimeError('Slider has inconsistent maximum: ' + str(slider))
    smoke.step('slider maximum matches ceiling', slider)
    # Reset through the public Zoom label, restoring fit before the usual smoke.
    smoke.execute("""const e=[...document.querySelectorAll('[data-tooltip]')]
      .find(e=>e.textContent.trim()==='Zoom');if(!e)throw Error('Zoom reset missing');e.click();return true;""")
    time.sleep(0.5)
    smoke.step('zoom restored to fit', read())
