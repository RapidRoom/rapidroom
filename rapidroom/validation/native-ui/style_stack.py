"""Ordered Styles on a real CC0 photo through public GUI and authenticated MCP."""
import json
import time

from PIL import ImageChops
from mcp_clients import image_state, rpc
from mcp_history import read_tool


def run_style_stack_checks(smoke, wait_for, baseline):
    path = str(smoke.case / 'input/smoke.ARW')
    endpoint_file = smoke.case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json'
    wait_for(endpoint_file.exists, 'private MCP endpoint')
    endpoint = json.loads(endpoint_file.read_text())
    url, token = endpoint['url'], endpoint['token']

    def state():
        return image_state(url, path, token=token)

    def tool(name, **args):
        return read_tool(url, token, name, {'imagePath': path, **args})

    def mutate(name, **args):
        return tool(name, expectedRevision=state()['editRevision'], **args)

    def applied():
        return tool('list_applied_styles')['styles']

    def panel():
        smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-presets\"]').click();return true;")
        wait_for(lambda: smoke.execute("return !!document.querySelector('input[aria-label=\"Search presets or groups\"]');"), 'Styles panel')

    def search(query):
        smoke.execute('''const e=document.querySelector('input[aria-label="Search presets or groups"]');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;''', [query])
        time.sleep(.3)

    def gui_entries():
        return smoke.execute('''return [...document.querySelectorAll('[data-applied-style]')].map(e=>({
          id:e.dataset.appliedStyle,amount:Number(e.querySelector('input[type=range]')?.value)}));''')

    original = state()
    assert applied() == []
    catalog = read_tool(url, token, 'list_presets', {'presetType': 'style', 'limit': 20})['presets']
    names = ['Teal & Orange', 'Neon Night', 'Dark Forest', 'Vivid Rainbow']
    ids = {name: next(p['id'] for p in catalog if p['name'] == name) for name in names}
    panel()
    search(names[0])
    smoke.execute('''const r=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim().startsWith(arguments[0]));
      if(!r)throw Error('Teal row missing');r.click();return true;''', [names[0]])
    wait_for(lambda: len(applied()) == 1, 'GUI Style visible to MCP')
    first_pixels = smoke.stable_preview('stack-first-gui-style')
    assert ImageChops.difference(smoke.crop(baseline), smoke.crop(first_pixels)).getbbox() is not None
    mutate('apply_preset', presetId=ids[names[1]], amount=60)
    mutate('apply_preset', presetId=ids[names[2]], amount=100)
    entries = applied()
    assert [p['presetId'] for p in entries] == [ids[n] for n in names[:3]]
    assert [p['position'] for p in entries] == [1, 2, 3]
    assert [p['amount'] for p in entries] == [100, 60, 100]
    wait_for(lambda: len(gui_entries()) == 3, 'MCP Styles visible in Applied folder')
    assert [p['amount'] for p in gui_entries()] == [100, 60, 100]
    assert smoke.execute('''const applied=document.querySelector('section[aria-label="Applied Styles"]');
      const search=document.querySelector('input[aria-label="Search presets or groups"]');
      return applied && search && applied.getBoundingClientRect().top>=search.getBoundingClientRect().bottom;''')
    smoke.capture('stack-three-applied')
    smoke.step('GUI and MCP share ordered three Styles with independent Amount and pinned Applied folder', entries)
    before = state()
    history = tool('history_list')
    rejected = rpc(url, 'tools/call', {'name': 'apply_preset', 'arguments': {
        'imagePath': path, 'expectedRevision': before['editRevision'], 'presetId': ids[names[3]]}}, token=token)
    assert rejected.get('isError') and 'Up to 3 Styles. Remove one first.' in json.dumps(rejected)
    assert state()['adjustments'] == before['adjustments'] and tool('history_list') == history
    smoke.step('fourth Style rejected without changing adjustments or history')
    # Search does not hide Applied. The first real slider changes only entry1.
    smoke.drag('Amount', 50)
    wait_for(lambda: abs(applied()[0]['amount'] - 50) <= 4, 'first independent Amount')
    assert [p['amount'] for p in applied()[1:]] == [60, 100]
    smoke.stable_preview('stack-independent-amount')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('Exposure');"), 'Develop controls')
    smoke.drag('Exposure', 1.2)
    smoke.drag('Contrast', 7)
    manual = state()['adjustments']
    time.sleep(.8)
    panel()
    # Remove the middle Style through its public X, retaining entry3's mask.
    smoke.execute('''const e=[...document.querySelectorAll('[data-applied-style]')][1];
      e.querySelector('button[aria-label="Remove Neon Night"]').click();return true;''')
    wait_for(lambda: len(applied()) == 2, 'middle Style removed')
    removed = state()
    assert removed['adjustments']['exposure'] == manual['exposure']
    assert removed['adjustments']['contrast'] == manual['contrast']
    assert [p['presetId'] for p in applied()] == [ids[names[0]], ids[names[2]]]
    assert removed['adjustments']['masks'] == manual['masks']
    removed_pixels = smoke.stable_preview('stack-middle-removed')
    mutate('undo')
    assert len(applied()) == 3 and state()['adjustments'] == manual
    mutate('redo')
    assert len(applied()) == 2 and state()['adjustments'] == removed['adjustments']
    assert ImageChops.difference(smoke.crop(removed_pixels), smoke.crop(smoke.stable_preview('stack-remove-redo'))).getbbox() is None
    smoke.step('middle GUI removal preserves manual Exposure/Contrast and editable mask; shared Undo/Redo restores exact pixels')
    sidecar = smoke.case / 'input/smoke.ARW.rrdata'
    wait_for(lambda: sidecar.exists() and json.loads(sidecar.read_text()).get('appliedPreset', {}).get('stack', {}).get('result') == removed['adjustments'], 'paired stack and controls persisted')
    saved = json.loads(sidecar.read_text())
    assert len(saved['appliedPreset']['stack']['entries']) == 2
    # Drop the frontend store/history, retaining the actual disk sidecar.
    smoke.execute('setTimeout(()=>location.reload(),0);return true;')
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.innerText.trim()==='Continue Session'&&!e.disabled);"), 'fresh frontend session')
    smoke.execute("[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Continue Session').click();return true;")
    wait_for(lambda: smoke.execute("return [...document.images].some(e=>e.complete&&e.naturalWidth>100);"), 'reloaded library')
    tool('select_image')
    wait_for(lambda: len(applied()) == 2, 'sidecar stack reopened')
    assert state()['adjustments'] == removed['adjustments']
    panel()
    wait_for(lambda: len(gui_entries()) == 2, 'Applied folder after restart')
    reloaded = smoke.stable_preview('stack-reopened')
    assert ImageChops.difference(smoke.crop(removed_pixels), smoke.crop(reloaded)).getbbox() is None
    smoke.step('fresh frontend reload restores actual sidecar stack, manual values and exact preview')
    # Removing all entries leaves manual edits; a subsequent Reset clears them.
    for entry in list(applied()):
        mutate('remove_style', presetId=entry['presetId'])
    assert applied() == []
    assert state()['adjustments']['exposure'] == manual['exposure']
    assert state()['adjustments']['contrast'] == manual['contrast']
    mutate('set_adjustments', adjustments=original['adjustments'])
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    restored = smoke.stable_preview('stack-restored-baseline')
    assert ImageChops.difference(smoke.crop(baseline), smoke.crop(restored)).getbbox() is None
    smoke.step('removing all Styles retains manual edits; restoring the original recipe restores exact baseline pixels')
