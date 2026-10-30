"""Real upstream compare menu and compositor views; no React store access."""
import json
import time
from smoke import save

LABELS = {"toggle": "Toggle", "vertical": "Vertical", "horizontal": "Horizontal", "sideBySide": "Side by side"}


def select_mode(smoke, wait_for, mode):
    smoke.execute("""const button=document.querySelector('[data-testid=editor-compare-toggle]');
      button.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2}));return true;""")
    wait_for(lambda: smoke.execute("return document.querySelectorAll('[role=menuitemradio]').length===4;"), "compare mode menu")
    before = smoke.execute("return document.querySelector('[data-editor-viewport] svg image')?.getAttribute('href');")
    smoke.execute("""const buttons=[...document.querySelectorAll('[role=menuitemradio]')];
      buttons[arguments[0]].click();return true;""", [list(LABELS).index(mode)])
    if mode == "toggle":
        wait_for(lambda: smoke.execute("return document.querySelector('[data-editor-viewport] svg image')?.getAttribute('href')!==arguments[0];", [before]), "original toggle render", 90)
    expected = "off" if mode == "toggle" else mode
    wait_for(lambda: smoke.execute("""return document.querySelector('[data-compare-view]')?.dataset.compareView===arguments[0]
      &&document.querySelector('[data-testid=editor-compare-toggle]')?.getAttribute('aria-pressed')==='true';""", [expected]), "compare " + mode, 90)


def viewport(smoke):
    return smoke.execute("""const v=document.querySelector('[data-editor-viewport]'),r=v.getBoundingClientRect();
      const image=[...v.querySelectorAll('svg image')].find(e=>e.getAttribute('href')?.startsWith('blob:'));
      const m=new DOMMatrix(v.querySelector(':scope > div.w-full').style.transform);
      if(!image)throw Error('Edited canvas image missing');
      const svg=image.closest('svg');
      const x=parseFloat(svg.style.left),y=parseFloat(svg.style.top);
      const w=parseFloat(svg.style.width),h=parseFloat(svg.style.height);
      if(![x,y,w,h,m.a,m.m41,m.m42].every(Number.isFinite)||w<=0||h<=0)throw Error('Invalid viewport geometry');
      return {pixelWidth:w*m.a, centre:[((r.width/2-m.m41)/m.a-x)/w,((r.height/2-m.m42)/m.d-y)/h],
        scale:m.a,position:[m.m41,m.m42]};""")


def run_compare_modes(smoke, wait_for):
    smoke.drag("Exposure", 1)
    smoke.stable_preview("compare-edited")
    settings = smoke.case / "data/io.github.CyberTimon.RapidRAW/settings.json"
    captures = {}
    for mode in LABELS:
        select_mode(smoke, wait_for, mode)
        wait_for(lambda: json.loads(settings.read_text()).get("compareMode") == mode, "persisted " + mode)
        time.sleep(1)
        captures[mode] = str(smoke.capture("compare-" + mode).relative_to(smoke.case))
        smoke.execute("document.querySelector('[data-testid=editor-compare-toggle]').click();return true;")
        wait_for(lambda: smoke.execute("return document.querySelector('[data-testid=editor-compare-toggle]').getAttribute('aria-pressed')==='false';"), "compare off")
    # Zoom through the existing real keyboard handler, then move between layouts.
    smoke.key("Digit1", "1", ctrl=True)
    time.sleep(1)
    before = viewport(smoke)
    select_mode(smoke, wait_for, "sideBySide")
    time.sleep(1)
    beside = viewport(smoke)
    select_mode(smoke, wait_for, "vertical")
    time.sleep(1)
    after = viewport(smoke)
    for row in (beside, after):
        if abs(row["pixelWidth"] - before["pixelWidth"]) > 2 or max(abs(a-b) for a,b in zip(row["centre"], before["centre"])) > .015:
            raise RuntimeError("Compare layout changed zoom or focal point: " + str((before, beside, after)))
    smoke.execute("document.querySelector('[data-testid=editor-compare-toggle]').click();return true;")
    smoke.key("Digit0", "0", ctrl=True)
    # Undo the owned exposure edit and leave the ordinary editor ready for the full smoke.
    smoke.key("KeyZ", "z", ctrl=True)
    wait_for(lambda: abs(smoke.slider("Exposure")["value"]) < .001, "comparison exposure restored")
    evidence = {"captures": captures, "zoom": {"before": before, "sideBySide": beside, "after": after}}
    save(smoke.case / "compare-modes.json", evidence)
    smoke.step("all upstream compare modes persist and preserve zoom", evidence)
