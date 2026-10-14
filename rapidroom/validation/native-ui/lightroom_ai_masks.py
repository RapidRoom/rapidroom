"""Native import of an authored ACR/JXL fixture; no Adobe assets or model calls."""
import hashlib
import json
from PIL import ImageChops
from mcp_clients import image_state, rpc


def run_ai_mask_checks(smoke, wait_for):
    case = smoke.case
    image = str(case / 'input/smoke.ARW')
    xmp = case / 'input/smoke.xmp'
    acr = case / 'input/smoke.acr'
    fixture = json.loads((case / 'lightroom-ai-masks-test.json').read_text())
    endpoint = json.loads((case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json').read_text())
    url, token = endpoint['url'], endpoint['token']
    initial = image_state(url, image, token=token)
    guard = hashlib.sha256(acr.read_bytes()).hexdigest()
    assert not xmp.exists()

    def dismiss():
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        wait_for(lambda: smoke.execute("return !document.querySelector('[role=alert]');"), 'AI import alerts dismissed')

    def import_xmp():
        rect = smoke.roi
        smoke.execute("""const x=arguments[0],y=arguments[1],e=document.elementFromPoint(x,y);
          e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:x,clientY:y}));return true;""", [(rect[0]+rect[2])/2,(rect[1]+rect[3])/2])
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'AI import menu')
        smoke.execute("[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;")


    def packet(mixed):
        companions = '''<rdf:li crs:What="Mask/CircularGradient" crs:MaskName="Subtract ellipse" crs:Top="0.4" crs:Left="0.4" crs:Bottom="0.6" crs:Right="0.6" crs:Flipped="true" crs:MaskBlendMode="1" crs:MaskInverted="true"/>
          <rdf:li crs:What="Mask/Aggregate" crs:MaskName="Inline brush"><crs:Masks><rdf:Seq>
            <rdf:li crs:What="Mask/Paint" crs:Radius="0.08" crs:Flow="1" crs:CenterWeight="1" crs:Density="1"><crs:Dabs><rdf:Seq><rdf:li>d 0.2 0.2</rdf:li></rdf:Seq></crs:Dabs></rdf:li>
          </rdf:Seq></crs:Masks></rdf:li>''' if mixed else ''
        return f'''<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
          <rdf:RDF><rdf:Description rdf:about=""><crs:MaskGroupBasedCorrections><rdf:Seq>
            <rdf:li crs:CorrectionName="Synthetic subject" crs:LocalExposure2012="1"><crs:CorrectionMasks><rdf:Seq>
              <rdf:li crs:What="Mask/Image" crs:MaskName="Subject coverage" crs:MaskSubType="1" crs:MaskDigest="{fixture['digest']}" crs:Origin="1,1" crs:FullMaskSize="5,4" crs:WholeImageArea="1,1,3,4"/>{companions}
            </rdf:Seq></crs:CorrectionMasks></rdf:li>
          </rdf:Seq></crs:MaskGroupBasedCorrections></rdf:Description></rdf:RDF></x:xmpmeta>'''

    dismiss()
    before = smoke.stable_preview('ai-mask-before')
    xmp.write_text(packet(True))
    source_xmp = xmp.read_bytes()
    import_xmp()
    imported = wait_for(lambda: image_state(url, image, token=token)['adjustments'].get('masks'),
                        'persisted native AI import masks', 120)
    assert len(imported) == 1
    types = [sub['type'] for sub in imported[0]['subMasks']]
    assert types[0:2] == ['ai-foreground', 'radial']
    assert types[2] in ('brush', 'flow')
    bitmap_bytes = [len(sub['parameters']['maskDataBase64']) for sub in imported[0]['subMasks']
                    if sub['parameters'].get('maskDataBase64')]
    assert bitmap_bytes[0] > 32
    result = {'name': imported[0]['name'], 'types': types, 'bitmapBytes': bitmap_bytes,
              'evidence': 'authenticated native image state after context-menu import'}
    smoke.capture('ai-mask-imported-report')
    dismiss()
    after = smoke.stable_preview('ai-mask-after')
    assert ImageChops.difference(smoke.crop(before),smoke.crop(after)).getbbox() is not None
    assert hashlib.sha256(acr.read_bytes()).hexdigest() == guard and xmp.read_bytes() == source_xmp
    persisted = case / 'input/smoke.ARW.rrdata'
    if not persisted.exists():
        persisted = next((case / 'input').glob('*.rrdata'))
    saved = persisted.read_bytes()
    xmp.write_text(packet(False))
    acr.rename(acr.with_suffix('.acr.hidden'))
    import_xmp()
    notice = wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].map(e=>e.textContent).find(text=>text.includes('Subject coverage')&&text.includes('unavailable'));"), 'named missing ACR notice')
    refused = {'notice': notice, 'evidence': 'visible native import refusal and unchanged sidecar'}
    smoke.capture('ai-mask-missing-companion-report')
    assert persisted.read_bytes() == saved
    acr.with_suffix('.acr.hidden').rename(acr)
    assert hashlib.sha256(acr.read_bytes()).hexdigest() == guard
    dismiss()
    result_file={'imported':result,'refused':refused,'sourceAcrUnchanged':True,'refusalPreservesSidecar':True,'previewChanged':True}
    (case / 'lightroom-ai-mask-evidence.json').write_text(json.dumps(result_file,indent=2)+'\n')
    current = image_state(url,image,token=token)
    restored = rpc(url,'tools/call',{'name':'set_adjustments','arguments':{
        'imagePath':image,'expectedRevision':current['editRevision'],
        'adjustments':initial['adjustments']}},token=token)
    assert not restored.get('isError'), restored
    wait_for(lambda: image_state(url,image,token=token)['adjustments'] == initial['adjustments'], 'original draft restored after AI import test')
    restored_preview = smoke.stable_preview('ai-mask-restored')
    assert ImageChops.difference(smoke.crop(before),smoke.crop(restored_preview)).getbbox() is None
    xmp.unlink()
    smoke.step('native ACR AI mask import persists mixed coverage and reports missing components by name',result_file)
