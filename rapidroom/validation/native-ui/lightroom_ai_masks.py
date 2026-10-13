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
    smoke.execute("""const old=window.__TAURI_INTERNALS__.invoke;
      window.__lraiCalls=[];
      window.__TAURI_INTERNALS__.invoke=async function(cmd,args,options){
        const result=await old.call(this,cmd,args,options);
        if(cmd==='import_xmp_adjustments_for_image')window.__lraiCalls.push({
          unchanged:result.unchanged,notTransferred:result.notTransferred,maskFailures:result.maskFailures,
          masks:(result.adjustments?.masks??[]).map(m=>({name:m.name,types:m.subMasks.map(s=>s.type),bitmapBytes:m.subMasks.filter(s=>s.parameters.maskDataBase64).map(s=>s.parameters.maskDataBase64.length)}))});
        return result;
      };return true;""")

    def dismiss():
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        wait_for(lambda: smoke.execute("return !document.querySelector('[role=alert]');"), 'AI import alerts dismissed')

    def import_xmp(count):
        rect = smoke.roi
        smoke.execute("""const x=arguments[0],y=arguments[1],e=document.elementFromPoint(x,y);
          e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:x,clientY:y}));return true;""", [(rect[0]+rect[2])/2,(rect[1]+rect[3])/2])
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'AI import menu')
        smoke.execute("[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;")
        return wait_for(lambda: smoke.execute("return window.__lraiCalls.length===arguments[0]?window.__lraiCalls.at(-1):null;",[count]), 'native AI import result',120)

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
    result = import_xmp(1)
    assert not result['unchanged'] and not result['maskFailures']
    assert 'aiMasks' not in result['notTransferred']
    assert len(result['masks']) == 1
    assert result['masks'][0]['types'][0:2] == ['ai-foreground','radial']
    assert result['masks'][0]['types'][2] in ('brush','flow')
    assert result['masks'][0]['bitmapBytes'][0] > 32
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
    refused = import_xmp(2)
    assert refused['unchanged'] and len(refused['maskFailures']) == 1
    assert refused['maskFailures'][0]['component'] == 'Subject coverage'
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].some(e=>e.textContent.includes('Subject coverage')&&e.textContent.includes('unavailable'));"), 'named missing ACR notice')
    smoke.capture('ai-mask-missing-companion-report')
    assert persisted.read_bytes() == saved
    acr.with_suffix('.acr.hidden').rename(acr)
    assert hashlib.sha256(acr.read_bytes()).hexdigest() == guard
    dismiss()
    result_file={'imported':result,'refused':refused,'sourceAcrUnchanged':True,'refusalPreservesSidecar':True,'previewChanged':True}
    (case / 'lightroom-ai-mask-evidence.json').write_text(json.dumps(result_file,indent=2)+'\n')
    rpc(url,'tools/call',{'name':'set_adjustments','arguments':{'path':image,'changes':initial['adjustments']}},token)
    wait_for(lambda: image_state(url,image,token=token)['adjustments'] == initial['adjustments'], 'original draft restored after AI import test')
    xmp.unlink()
    smoke.step('native ACR AI mask import persists mixed coverage and reports missing components by name',result_file)
