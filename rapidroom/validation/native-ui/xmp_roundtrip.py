"""Photo XMP export/import through public UI and authenticated local MCP."""
import base64
import hashlib
import json
import xml.etree.ElementTree as ET
import zlib

from PIL import ImageChops
from mcp_clients import image_state, rpc, run_client_checks, tool_data

RR = 'urn:rapidroom:edit:1.0'
CRS = 'http://ns.adobe.com/camera-raw-settings/1.0/'
RDF = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'


def payload(path):
    tree = ET.parse(path)
    description = tree.getroot().find('.//{' + RDF + '}Description')
    raw = zlib.decompress(base64.b64decode(description.attrib['{' + RR + '}Recipe']))
    assert description.attrib['{' + RR + '}Format'] == '1'
    assert hashlib.sha256(raw).hexdigest() == description.attrib['{' + RR + '}RecipeSha256']
    return tree, description, json.loads(raw)


def run_xmp_checks(case, smoke, wait_for, baseline):
    endpoint = case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json'
    wait_for(endpoint.exists, 'private authenticated MCP endpoint')
    connection = json.loads(endpoint.read_text())
    url, token = connection['url'], connection['token']
    image = str(case / 'input/smoke.ARW')
    initial = image_state(url, image, token=token)

    def call(name, args):
        result = rpc(url, 'tools/call', {'name': name, 'arguments': args}, token=token)
        if result.get('isError'):
            raise RuntimeError(name + ' failed: ' + str(result))
        return tool_data(result)

    call('update_adjustments', {'imagePath': image, 'expectedRevision': initial['editRevision'],
                              'changes': {'exposure': 0.5, 'halationAmount': 25}})
    wait_for(lambda: image_state(url, image, token=token)['adjustments']['exposure'] == 0.5, 'draft change')
    edited = image_state(url, image, token=token)
    destination = case / 'mcp-export.xmp'
    report = call('export_xmp_sidecar', {'imagePath': image, 'filePath': str(destination)})
    assert report['report']['written'] is False and not destination.exists()
    assert any(loss['field'] == 'halationAmount' and loss['preservedInPayload'] for loss in report['report']['losses'])
    assert image_state(url, image, token=token) == edited
    smoke.step('MCP named-loss preview makes no file or draft change')

    stale = rpc(url, 'tools/call', {'name': 'export_xmp_sidecar', 'arguments': {
        'imagePath': image, 'filePath': str(destination), 'write': True,
        'expectedRevision': initial['editRevision']}}, token=token)
    assert stale.get('isError') and not destination.exists()
    smoke.step('stale MCP XMP write is refused before file creation')
    call('export_xmp_sidecar', {'imagePath': image, 'filePath': str(destination), 'write': True,
                              'expectedRevision': edited['editRevision']})
    assert payload(destination)[2]['adjustments'] == edited['adjustments']
    assert image_state(url, image, token=token) == edited
    smoke.step('MCP XMP write carries the full current draft without changing revision')

    smoke.key('KeyE', 'e')
    wait_for(lambda: smoke.execute("return !!document.querySelector('section[aria-label=\"Lightroom XMP sidecar\"]');"), 'XMP export section')
    smoke.execute("""const b=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Review sidecar next to photo');
      if(!b)throw Error('XMP review button missing');b.scrollIntoView({block:'center'});b.click();return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='Write XMP sidecar');"), 'GUI named loss report')
    sidecar = case / 'input/smoke.xmp'
    assert not sidecar.exists()
    smoke.capture('xmp-loss-report')
    smoke.step('GUI displays named losses before sidecar write')
    smoke.execute("""const b=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Write XMP sidecar');
      b.scrollIntoView({block:'center'});b.click();return true;""")
    wait_for(sidecar.exists, 'GUI XMP sidecar written')
    assert payload(sidecar)[2]['adjustments'] == edited['adjustments']
    assert image_state(url, image, token=token) == edited
    smoke.step('public GUI XMP export preserves the full current edit')

    tree, description, stored = payload(sidecar)
    description.set('{' + CRS + '}Exposure2012', '1.25')
    # ElementTree deliberately rewrites prefixes. This is a synthetic rewrite,
    # not evidence of actual Lightroom preservation.
    tree.write(sidecar, encoding='utf-8', xml_declaration=True)
    assert payload(sidecar)[2] == stored
    rect = smoke.roi
    smoke.execute("""const x=arguments[0],y=arguments[1];const e=document.elementFromPoint(x,y);
      e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:x,clientY:y}));return true;""",
                  [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2])
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'public XMP import menu')
    smoke.execute("""[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;""")
    wait_for(lambda: image_state(url, image, token=token)['adjustments']['exposure'] == 1.25, 'XMP import reconciles Lightroom exposure')
    imported = image_state(url, image, token=token)
    expected = dict(edited['adjustments'], exposure=1.25)
    assert imported['adjustments'] == expected
    smoke.stable_preview('xmp-lightroom-imported')
    smoke.step('public GUI XMP import restores native fields and accepts Lightroom exposure change')

    smoke.execute("""document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());
      document.querySelector('[data-tab-id=layout-tab-adjustments]').click();return true;""")
    call('set_adjustments', {'imagePath': image, 'expectedRevision': imported['editRevision'],
                            'adjustments': initial['adjustments']})
    assert image_state(url, image, token=token)['adjustments'] == initial['adjustments']
    restored = smoke.stable_preview('xmp-original-restored')
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(restored)).getbbox() is not None:
        raise RuntimeError('Restoring the original draft failed to restore exact pixels')
    smoke.step('original draft and exact native pixels restored after XMP round trip')
    run_client_checks(case, smoke, wire_only=True, folder_name="xmp-client-tests")
    (case / 'xmp-round-trip-result.json').write_text(json.dumps({
        'real_model_requests': False, 'real_lightroom_tested': False,
        'gui_export_and_import': True, 'mcp_export_and_losses': True,
        'named_losses': report['report']['losses'], 'native_restored_exactly': True,
        'lightroom_exposure_change_reconciled': True,
    }, indent=2) + '\n')
