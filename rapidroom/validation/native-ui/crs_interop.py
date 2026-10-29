"""Public GUI import checks for synthetic CRS packets; no Adobe files or models."""
import json
from PIL import ImageChops
from mcp_clients import image_state, rpc, run_client_checks


def run_crs_checks(smoke, wait_for):
    case = smoke.case
    endpoint = json.loads((case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json').read_text())
    url, token = endpoint['url'], endpoint['token']
    image = str(case / 'input/smoke.ARW')
    sidecar = case / 'input/smoke.xmp'
    assert not sidecar.exists()
    initial = image_state(url, image, token=token)

    def dismiss_alerts():
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        wait_for(lambda: smoke.execute("return !document.querySelector('[role=alert]');"), 'CRS alerts dismissed before pixel comparison')

    dismiss_alerts()
    baseline = smoke.stable_preview('crs-original')

    def import_sidecar():
        rect = smoke.roi
        smoke.execute("""const x=arguments[0],y=arguments[1];const e=document.elementFromPoint(x,y);
          e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:x,clientY:y}));return true;""",
                      [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2])
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'public CRS import menu')
        smoke.execute("""[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;""")

    sidecar.write_text("""<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
      xmlns:a="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:crs="urn:foreign">
      <r:Description r:about="photo" a:Exposure2012='+1.25' crs:Contrast2012="99">
        <a:Look><r:Description><a:Shadows2012>85</a:Shadows2012></r:Description></a:Look>
        <a:ToneCurvePV2012><r:Seq><r:li>0, 0</r:li><r:li>128, 144</r:li><r:li>255, 255</r:li></r:Seq></a:ToneCurvePV2012>
      </r:Description><r:Description r:about="other" a:Blacks2012="75"/>
    </r:RDF>""")
    import_sidecar()
    wait_for(lambda: image_state(url, image, token=token)['adjustments']['exposure'] == 1.25, 'renamed-prefix CRS import')
    imported = image_state(url, image, token=token)
    assert imported['adjustments']['contrast'] == initial['adjustments']['contrast']
    assert imported['adjustments']['shadows'] == initial['adjustments']['shadows']
    assert imported['adjustments']['blacks'] == initial['adjustments']['blacks']
    assert imported['adjustments']['curves']['luma'] == [{'x': 0, 'y': 0}, {'x': 128, 'y': 144}, {'x': 255, 'y': 255}]
    smoke.capture('crs-import-report')
    dismiss_alerts()
    changed = smoke.stable_preview('crs-imported')
    assert ImageChops.difference(smoke.crop(baseline), smoke.crop(changed)).getbbox() is not None
    immediate = imported
    imported = image_state(url, image, token=token)
    snapshots = {'original': initial, 'imported_immediate': immediate, 'imported_after_render': imported}
    (case / 'crs-state-snapshots.json').write_text(json.dumps(snapshots, indent=2) + '\n')
    assert imported['adjustments']['exposure'] == 1.25
    assert imported['adjustments']['curves']['luma'] == [{'x': 0, 'y': 0}, {'x': 128, 'y': 144}, {'x': 255, 'y': 255}]
    for key in ('contrast', 'shadows', 'blacks'):
        assert imported['adjustments'][key] == initial['adjustments'][key]
    smoke.step('public CRS import resolves namespace aliases and curves without foreign/nested/other-subject values')

    dismiss_alerts()
    sidecar.write_text('<root crs:SharpenRadius="0" crs:PerspectiveVertical="3"/>')
    import_sidecar()
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].some(e=>e.textContent.includes('0 mapped, 0 approximated, 2 ignored'))"), 'bounded per-setting import summary')
    smoke.execute("[...document.querySelectorAll('[role=alert]')].find(e=>e.textContent.includes('0 mapped, 0 approximated, 2 ignored')).click();return true;")
    wait_for(lambda: smoke.execute("return document.body.textContent.includes('SharpenRadius — ignored: Lightroom sharpening radius/detail')"), 'individual ignored setting with reason in import dialog')
    report_text = smoke.execute("return document.body.innerText;")
    assert 'PerspectiveVertical — ignored: Lightroom perspective/Upright' in report_text
    assert image not in report_text and str(case) not in report_text
    wait_for(lambda: smoke.execute("""const dialog = document.querySelector('[role=dialog]');
        return !!dialog && Number(getComputedStyle(dialog).opacity) >= 0.99
          && Number(getComputedStyle(dialog.firstElementChild).opacity) >= 0.99;"""),
             'import dialog CSS transition settled')
    smoke.capture('crs-per-setting-report')
    assert image_state(url, image, token=token) == imported, 'Unsupported-only import changed native draft'
    smoke.execute("[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='Cancel').click();return true;")
    dismiss_alerts()
    unchanged = smoke.stable_preview('crs-unsupported-unchanged')
    assert ImageChops.difference(smoke.crop(changed), smoke.crop(unchanged)).getbbox() is None
    smoke.step('unsupported-only import shows individual reasons without private paths or changes to native edits/pixels')

    sidecar.write_text('<!DOCTYPE root [<!ENTITY x SYSTEM "file:///etc/passwd">]><root>&x;</root>')
    import_sidecar()
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].some(e=>e.textContent.includes('DOCTYPE'))"), 'unsafe CRS import refused')
    smoke.capture('crs-refusal-alert')
    after_refusal = image_state(url, image, token=token)
    snapshots['after_refusal'] = after_refusal
    (case / 'crs-state-snapshots.json').write_text(json.dumps(snapshots, indent=2) + '\n')
    assert after_refusal == imported, 'Refusal changed settled state; see crs-state-snapshots.json'
    dismiss_alerts()
    refused = smoke.stable_preview('crs-unsafe-refused')
    assert ImageChops.difference(smoke.crop(changed), smoke.crop(refused)).getbbox() is None
    expected_error = 'Failed to import XMP adjustments: Failed to read XMP file: XMP refused: DOCTYPE is unsupported'
    consumed = smoke.execute("""const errors=window.__RAPIDROOM_SMOKE_ERRORS__;
      const matches=errors.filter(e=>e===arguments[0]).length;
      if(matches===1) errors.splice(errors.indexOf(arguments[0]),1);
      return matches;""", [expected_error])
    assert consumed == 1, 'Expected exactly one console error for the verified DOCTYPE refusal'
    (case / 'crs-expected-error.json').write_text(json.dumps({'error': expected_error, 'occurrences': consumed}, indent=2) + '\n')
    smoke.step('public import refuses DOCTYPE without changing the active draft or pixels')

    result = rpc(url, 'tools/call', {'name': 'set_adjustments', 'arguments': {
        'imagePath': image, 'expectedRevision': imported['editRevision'], 'adjustments': initial['adjustments']}}, token=token)
    assert not result.get('isError'), result
    assert image_state(url, image, token=token)['adjustments'] == initial['adjustments']
    final = smoke.stable_preview('crs-original-restored')
    assert ImageChops.difference(smoke.crop(baseline), smoke.crop(final)).getbbox() is None
    sidecar.unlink()
    run_client_checks(case, smoke, wire_only=True, folder_name="crs-client-tests")
    smoke.expected_crs_refusal_error = expected_error
    (case / 'crs-interop-result.json').write_text(json.dumps({
        'real_model_requests': False, 'real_lightroom_tested': False,
        'public_alias_import_and_curve': True, 'foreign_nested_subject_fields_ignored': True,
        'doctype_refused_without_changes': True, 'original_draft_and_pixels_restored': True}, indent=2) + '\n')
    smoke.step('CRS checks restore the original draft and exact native preview pixels')
