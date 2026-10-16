"""Owned JPEG metadata and real native Library filter controls for issue312."""
import json
from PIL import Image, ImageDraw


def prepare_filter_chips(case):
    for name, iso, lens in [('a', 6400, '35mm'), ('b', 12800, '50mm'), ('c', 100, '35mm')]:
        image = Image.new('RGB', (960, 640), '#395a74')
        ImageDraw.Draw(image).text((80, 200), f'{name}: ISO {iso}, lens {lens}', fill='white', font_size=42)
        path = case / f'input/z-query-{name}.jpg'
        exif = Image.Exif()
        exif[34855] = iso
        exif[42036] = lens
        image.save(path, exif=exif, quality=90)
        path.with_name(path.name + '.rrdata').write_text(json.dumps({
            'version': 1, 'rating': 0, 'adjustments': None,
            'exif': {'PhotographicSensitivity': str(iso), 'ISOSpeedRatings': str(iso), 'LensModel': lens}
        }))


def run_filter_chip_checks(smoke, wait_for):
    from smoke import request
    input_selector = 'input[aria-label="Search or Filter"]'

    def type_text(text):
        smoke.execute("""const e=document.querySelector(arguments[0]);if(!e)throw Error('Search input missing');
          e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[1]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;""", [input_selector, text])

    def key(value):
        smoke.execute("""const e=document.querySelector(arguments[0]);e.focus();
          e.dispatchEvent(new KeyboardEvent('keydown',{key:arguments[1],bubbles:true,cancelable:true}));return true;""", [input_selector, value])

    def chips():
        return smoke.execute("return [...document.querySelectorAll('[data-filter-chip-edit]')].map(e=>e.getAttribute('aria-label').replace('Edit filter ',''));")

    def value():
        return smoke.execute('return document.querySelector(arguments[0]).value;', [input_selector])

    def edit(index):
        smoke.execute("document.querySelectorAll('[data-filter-chip-edit]')[arguments[0]].click();return true;", [index])

    def remove(index):
        smoke.execute("document.querySelectorAll('button[aria-label^=\"Remove filter \"]')[arguments[0]].click();return true;", [index])

    def fixtures():
        return smoke.execute("return [...document.querySelectorAll('[data-image-path]')].map(e=>e.dataset.imagePath.split('/').pop()).filter(p=>p.startsWith('z-query-')).sort();")

    def capture(name):
        request(smoke.url + '/execute/async', {'script': 'const done=arguments[arguments.length-1];requestAnimationFrame(()=>requestAnimationFrame(()=>setTimeout(()=>done(true),100)));', 'args': []})
        path = smoke.capture(name)
        smoke.snapshot(name)
        return str(path.relative_to(smoke.case))

    wait_for(lambda: fixtures() == ['z-query-a.jpg', 'z-query-b.jpg', 'z-query-c.jpg'], 'owned filter fixtures loaded')
    type_text('iso:>3200'); key('Enter')
    wait_for(lambda: chips() == ['iso:>3200'] and fixtures() == ['z-query-a.jpg', 'z-query-b.jpg'], 'numeric ISO query filters real photos')
    edit(0)
    wait_for(lambda: value() == 'iso:>3200' and chips() == [], 'chip body starts editing')
    type_text('iso:>10000'); key('Enter')
    wait_for(lambda: chips() == ['iso:>10000'] and fixtures() == ['z-query-b.jpg'], 'edited numeric query updates result')
    smoke.step('numeric filter chip edits and filters native library', {'capture': capture('filters-numeric')})

    # Match all preserves the numeric constraint when adding the lens query.
    smoke.execute("const e=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='OR');if(!e)throw Error('OR mode missing');e.click();return true;")
    type_text('lens:35mm'); key('Enter')
    wait_for(lambda: len(chips()) == 2 and fixtures() == [], 'AND lens query excludes the high-ISO 50mm photo')
    edit(1); type_text('lens:50mm'); key('Enter')
    wait_for(lambda: chips() == ['iso:>10000', 'lens:50mm'] and fixtures() == ['z-query-b.jpg'], 'edited text query updates native result')
    smoke.step('text filter chip edits and preserves query order', {'capture': capture('filters-text')})

    edit(0); type_text('iso:<200'); key('Escape')
    wait_for(lambda: chips() == ['iso:>10000', 'lens:50mm'] and value() == '' and fixtures() == ['z-query-b.jpg'], 'Escape restores original condition and index')
    key('Backspace')
    wait_for(lambda: chips() == ['iso:>10000'] and value() == 'lens:50mm', 'Backspace edits the last chip')
    key('Escape')
    wait_for(lambda: chips() == ['iso:>10000', 'lens:50mm'], 'Escape restores Backspace-edited chip')
    smoke.step('Escape cancels numeric and Backspace edits without losing chips')

    type_text('iso:>10000'); key('Enter')
    wait_for(lambda: chips() == ['iso:>10000', 'lens:50mm', 'iso:>10000'], 'duplicate chip added')
    remove(2)
    wait_for(lambda: chips() == ['iso:>10000', 'lens:50mm'] and value() == '', 'x deletes only its indexed duplicate and does not edit')
    remove(0)
    wait_for(lambda: chips() == ['lens:50mm'] and value() == '' and fixtures() == ['z-query-b.jpg'], 'x has independent delete action')
    smoke.step('separate x deletes one indexed chip without editing', {'capture': capture('filters-delete')})

    smoke.execute("const e=document.querySelector(arguments[0]);e.focus();e.setSelectionRange(0,0);e.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true,cancelable:true}));return true;", [input_selector])
    wait_for(lambda: smoke.execute("return document.activeElement?.hasAttribute('data-filter-chip-edit');"), 'Left moves input focus to chip')
    smoke.execute("document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));return true;")
    wait_for(lambda: smoke.execute('return document.activeElement===document.querySelector(arguments[0]);', [input_selector]), 'Right returns focus to input')
    smoke.step('native Left and Right navigate chip controls')
    remove(0)
    wait_for(lambda: chips() == [] and value() == '' and len(fixtures()) == 3, 'filter cleanup restores the library')
