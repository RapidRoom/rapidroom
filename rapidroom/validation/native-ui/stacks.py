"""Real sidecars and native grid/filmstrip stack gestures on owned fixtures."""
import json
import time
from PIL import Image, ImageDraw, ImageStat


def prepare_stacks(case):
    for index, subsec in enumerate(('10', '20', '90'), 1):
        image = Image.new('RGB', (960, 640))
        draw = ImageDraw.Draw(image)
        for y in range(640):
            draw.line((0, y, 959, y), fill=(y % 256, (y * 2 + index * 40) % 256, (y * 3) % 256))
        path = case / f'input/stack-{index}.jpg'
        image.save(path, quality=90)
        path.with_name(path.name + '.rrdata').write_text(json.dumps({
            'version': 1, 'rating': 0, 'adjustments': None,
            'exif': {'DateTimeOriginal': '2026-10-12 12:00:00', 'SubSecTimeOriginal': subsec}
        }))


def run_stack_checks(smoke, wait_for):
    case = smoke.case
    paths = [str(case / f'input/stack-{i}.jpg') for i in range(1, 4)]
    def visible():
        return smoke.execute("return [...document.querySelectorAll('[data-image-path]')].map(e=>e.dataset.imagePath);")
    def click(path, ctrl=False, double=False):
        smoke.execute("""const e=[...document.querySelectorAll('[data-image-path]')].find(e=>e.dataset.imagePath===arguments[0]);
          if(!e)throw Error('Photo absent from native view');
          e.dispatchEvent(new MouseEvent(arguments[2]?'dblclick':'click',{bubbles:true,button:0,ctrlKey:arguments[1]}));return true;""", [path, ctrl, double])
    def stack_menu(path, label):
        smoke.execute("""const e=[...document.querySelectorAll('[data-image-path]')].find(e=>e.dataset.imagePath===arguments[0]);
          if(!e)throw Error('Context target absent');const r=e.getBoundingClientRect();
          e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:r.left+20,clientY:r.top+20}));return true;""", [path])
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Stacks');"), 'stacks context menu')
        smoke.execute("""const e=[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Stacks');
          e.dispatchEvent(new MouseEvent('mouseover',{bubbles:true}));return true;""")
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()===arguments[0]);", [label]), 'stack menu action '+label)
        smoke.execute("""const e=[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()===arguments[0]);
          if(e.disabled)throw Error('Stack action disabled');e.click();return true;""", [label])
    def saved():
        return [json.loads((case / f'input/stack-{i}.jpg.rrdata').read_text()).get('stack') for i in range(1, 4)]
    def capture(name):
        # DOM assertions can beat the compositor by one frame. Wait for two
        # browser frames and the presentation handoff before taking evidence.
        from smoke import request
        request(smoke.url + '/execute/async', {'script': """
          const done=arguments[arguments.length-1];
          requestAnimationFrame(()=>requestAnimationFrame(()=>setTimeout(()=>done(true),100)));""", 'args': []})
        path = smoke.capture(name)
        with Image.open(path) as image:
            if max(ImageStat.Stat(image).stddev) < 10:
                raise RuntimeError('Stack evidence compositor is blank')
        smoke.snapshot(name)
        return str(path.relative_to(case))

    wait_for(lambda: all(p in visible() for p in paths), 'three real native stack thumbnails')
    capture('stacks-before')
    click(paths[0]); click(paths[1], ctrl=True)
    smoke.key('KeyG', 'g', True)
    wait_for(lambda: saved()[0] and saved()[1] and saved()[0]['id'] == saved()[1]['id'], 'Ctrl+G writes matching portable stack IDs')
    wait_for(lambda: paths[0] in visible() and paths[1] not in visible(), 'collapsed grid hides member')
    smoke.step('manual stack writes sidecars and collapses grid', {'capture': capture('stacks-collapsed')})
    smoke.execute("document.querySelector('button[aria-label=\"Expand or collapse stack (2 photos)\"]').click();return true;")
    wait_for(lambda: paths[1] in visible(), 'badge expands grid')
    stack_menu(paths[1], 'Use this photo as the cover')
    wait_for(lambda: saved()[1] and saved()[1]['position'] == 0 and paths[0] not in visible(), 'member becomes persisted cover')
    smoke.step('expanded grid and cover selection', {'capture': capture('stacks-new-cover')})
    click(paths[1], double=True)
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('960 × 640') && document.body.innerText.includes('Exposure');"), 'stack cover opens editor')
    # Dimensions and sliders appear while the first GPU pipeline is still
    # compiling. Keep this editor open until its actual processed preview is
    # displayed; a thumbnail is insufficient evidence for filmstrip review.
    wait_for(lambda: '[process_preview_job] full 960x640' in (case / 'app.log').read_text(),
             'stack cover GPU preview completed', 60)
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('svg image')].some(e=>e.getAttribute('href')?.startsWith('blob:'));"),
             'stack cover processed preview displayed')
    smoke.roi = tuple(round(v) for v in smoke.execute("""const e=[...document.querySelectorAll('svg')]
      .find(e=>e.getBoundingClientRect().width>500 && e.getBoundingClientRect().height>300);
      if(!e)throw Error('Stack preview bounds missing');const r=e.getBoundingClientRect();
      return [r.left,r.top,r.right,r.bottom];"""))
    smoke.stable_preview('stack-cover-ready')
    wait_for(lambda: smoke.execute("return !!document.querySelector('button[aria-label=\"Expand or collapse stack (2 photos)\"]');"), 'filmstrip has stack badge')
    smoke.execute("document.querySelector('button[aria-label=\"Expand or collapse stack (2 photos)\"]').click();return true;")
    wait_for(lambda: paths[0] in visible() and paths[1] in visible(), 'filmstrip expands shared stack')
    smoke.step('native filmstrip expand', {'capture': capture('stacks-filmstrip')})
    smoke.key('Escape', 'Escape')
    wait_for(lambda: paths[0] in visible(), 'library retains expanded stack')
    stack_menu(paths[1], 'Unstack (Ctrl+Shift+G)')
    wait_for(lambda: saved()[0] is None and saved()[1] is None, 'unstack clears every member sidecar')
    stack_menu(paths[0], 'Auto-stack by capture time…')
    wait_for(lambda: smoke.execute("return !!document.querySelector('#auto-stack-title');"), 'auto-stack dialog opens')
    # Set the public seconds input through its native React input handler.
    smoke.execute("""const e=document.querySelector('[role=dialog] input[type=number]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'0.15');
      e.dispatchEvent(new Event('input',{bubbles:true}));return true;""")
    smoke.execute("[...document.querySelectorAll('[role=dialog] button')].find(e=>e.textContent==='Preview').click();return true;")
    wait_for(lambda: smoke.execute("return document.querySelector('[role=dialog] ul')?.textContent.includes('stack-2.jpg');"), 'sub-second auto preview lists first pair')
    if smoke.execute("return document.querySelector('[role=dialog] ul').textContent.includes('stack-3.jpg');"):
        raise RuntimeError('Sub-second gap grouped late shot')
    if any(saved()):
        raise RuntimeError('Auto preview changed stack sidecars before Apply')
    smoke.step('capture-time preview is read-only and respects subseconds', {'capture': capture('stacks-auto-preview')})
    smoke.execute("[...document.querySelectorAll('[role=dialog] button')].find(e=>e.textContent==='Apply stacks').click();return true;")
    wait_for(lambda: saved()[0] and saved()[1] and saved()[2] is None, 'apply saves only previewed pair')
    wait_for(lambda: paths[1] not in visible(), 'auto apply collapses grid')
    stack_menu(paths[0], 'Apply stars and flags to whole stacks (this session)')
    click(paths[0]); smoke.key('Digit3','3')
    wait_for(lambda: all(json.loads((case / f'input/stack-{i}.jpg.rrdata').read_text()).get('rating') == 3 for i in (1,2)), 'whole-stack stars persist to both members')
    smoke.step('auto apply and opt-in whole-stack rating', {'capture': capture('stacks-auto-applied')})
    stack_menu(paths[0], 'Unstack (Ctrl+Shift+G)')
    wait_for(lambda: not any(saved()), 'native test cleanup unstacks fixtures')
    # Leave the existing full smoke scenario focused on the real CC0 RAW.
    click(str(case / 'input/smoke.ARW'))
    smoke.step('stack sidecar disk persistence contract', {'positions_written_and_reloaded_from_disk': True, 'full_app_restart': False})
