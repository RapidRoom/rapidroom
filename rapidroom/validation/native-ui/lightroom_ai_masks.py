"""Native import of an authored ACR/JXL fixture; no Adobe assets or model calls."""
import hashlib
import json
import time
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

    observer = smoke.execute("""window.__aiMaskAudit={calls:[]};const api=window.__TAURI_INTERNALS__,callbacks=api.callbacks;
  function record(cmd,payload,callback,error){
   if(!['apply_adjustments','load_metadata','load_image','save_metadata_and_update_thumbnail','import_xmp_adjustments_for_image','import_matching_xmp_sidecars_in_folder'].includes(cmd))return;
   const row={startedAt:performance.now(),command:cmd,args:JSON.parse(JSON.stringify(payload)),done:false};window.__aiMaskAudit.calls.push(row);
   for(const [id,isError] of [[callback,false],[error,true]]){const old=callbacks.get(Number(id));if(!old)throw Error('Missing native callback '+cmd);callbacks.set(Number(id),value=>{row.done=true;row.completedAt=performance.now();if(isError)row.error=String(value);else if(cmd==='apply_adjustments'){row.previewBytes=value?.byteLength??value?.length;}else row.value=value;return old(value);});}
  }
  const oldFetch=window.fetch;const observer=function(url,options){if(String(url).startsWith('ipc://localhost/')){const cmd=decodeURIComponent(String(url).split('ipc://localhost/')[1]);record(cmd,typeof options.body==='string'?JSON.parse(options.body):options.body,options.headers.get('Tauri-Callback'),options.headers.get('Tauri-Error'));}return oldFetch.apply(this,arguments);};window.fetch=observer;
  if(window.fetch!==observer)throw Error('Native fetch observer unavailable');return {nativeFetchObserved:true,invokeUntouched:true};""")
    (case / 'ai-mask-native-observer.json').write_text(json.dumps(observer, indent=2)+'\n')

    def sidecar_guard(path):
        info = path.stat()
        return {'size': info.st_size, 'mtime_ns': info.st_mtime_ns,
                'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

    def settled_import():
        started = time.monotonic()
        calls = wait_for(lambda: time.monotonic()-started >= 2 and smoke.execute(
            "return window.__aiMaskAudit.calls.some(c=>c.command==='import_xmp_adjustments_for_image'&&c.done&&!c.error)&&window.__aiMaskAudit.calls.every(c=>c.done)&&window.__aiMaskAudit.calls;"),
            'AI import and delayed native callbacks settled', 30)
        return {'calls': calls, 'observation_seconds': time.monotonic()-started}

    def dismiss():
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        wait_for(lambda: smoke.execute("return !document.querySelector('[role=alert]');"), 'AI import alerts dismissed')

    def import_xmp():
        smoke.execute("window.__aiMaskAudit.calls=[];return true;")
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
    successful_import = settled_import()
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
    saved = sidecar_guard(persisted)
    saved_state = image_state(url, image, token=token)
    xmp.write_text(packet(False))
    acr.rename(acr.with_suffix('.acr.hidden'))
    import_xmp()
    notice = wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].map(e=>e.textContent).find(text=>text.includes('Subject coverage')&&text.includes('unavailable'));"), 'named missing ACR notice')
    refusal_observation = settled_import()
    assert sidecar_guard(persisted) == saved, 'Refused AI import changed the sidecar after callbacks settled'
    assert image_state(url, image, token=token) == saved_state, 'Refused AI import changed native image state'
    assert not any(c['command'] in ('save_metadata_and_update_thumbnail', 'apply_adjustments')
                   for c in refusal_observation['calls']), 'Refused AI import triggered an unsolicited save or preview'
    refused = {'notice': notice, 'before': saved, 'after': sidecar_guard(persisted),
               'observation': refusal_observation,
               'evidence': 'visible native refusal, completed import and delayed no-write observation'}
    smoke.capture('ai-mask-missing-companion-report')
    assert sidecar_guard(persisted) == saved
    acr.with_suffix('.acr.hidden').rename(acr)
    assert hashlib.sha256(acr.read_bytes()).hexdigest() == guard
    dismiss()
    result_file={'imported':result,'successfulImportObservation':successful_import,'refused':refused,'sourceAcrUnchanged':True,'refusalPreservesSidecar':True,'previewChanged':True}
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
