"""Synthetic photo-versus-preset policy on disposable CC0 input; no Adobe app."""
import hashlib
import json
from PIL import ImageChops
from mcp_clients import image_state, rpc


def run_photo_policy_checks(smoke, wait_for):
    case = smoke.case
    endpoint = json.loads((case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json').read_text())
    url, token = endpoint['url'], endpoint['token']
    raw = case / 'input/smoke.ARW'
    image = str(raw)
    sidecar = case / 'input/smoke.xmp'
    assert not sidecar.exists()
    raw_sha = hashlib.sha256(raw.read_bytes()).hexdigest()
    initial = image_state(url, image, token=token)
    settings_path = case / 'data/io.github.CyberTimon.RapidRAW/settings.json'
    settings = json.loads(settings_path.read_text())
    old_sync = settings.get('enableXmpSync')
    settings.update(enableXmpSync=True, createXmpIfMissing=False)
    settings_path.write_text(json.dumps(settings))

    def dismiss():
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        wait_for(lambda: smoke.execute("return !document.querySelector('[role=alert]');"), 'policy alerts dismissed')

    def import_menu():
        rect = smoke.roi
        smoke.execute("""const x=arguments[0],y=arguments[1],e=document.elementFromPoint(x,y);
          e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:x,clientY:y}));return true;""",
                      [(rect[0]+rect[2])/2, (rect[1]+rect[3])/2])
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'photo policy public import menu')

    def click_import():
        smoke.execute("""[...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;""")

    def packet(attributes, curve=False):
        return '<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:a="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/"><r:Description r:about="photo" '+attributes+'>'+ ('<a:ToneCurvePV2012><r:Seq><r:li>0, 0</r:li><r:li>128, 144</r:li><r:li>255, 255</r:li></r:Seq></a:ToneCurvePV2012>' if curve else '')+'</r:Description></r:RDF>'

    def restore():
        # Ordinary autosave may sync XMP; remove only the owned synthetic input first.
        sidecar.unlink(missing_ok=True)
        current = image_state(url, image, token=token)
        result = rpc(url, 'tools/call', {'name':'set_adjustments','arguments':{'imagePath':image,'expectedRevision':current['editRevision'],'adjustments':initial['adjustments']}}, token=token)
        assert not result.get('isError'), result
        wait_for(lambda: image_state(url, image, token=token)['adjustments'] == initial['adjustments'], 'policy original restored')
        dismiss()

    dismiss()
    baseline = smoke.stable_preview('photo-policy-original')
    for name, attrs in [('already-applied', 'a:AlreadyApplied="True" a:Exposure2012="4" a:HasCrop="True" xmp:Rating="5"'), ('metadata-only', 'xmp:Rating="3"')]:
        xml = packet(attrs, curve=True if name == 'already-applied' else False)
        sidecar.write_text(xml)
        import_menu()
        # A real controlled range edit and import occur in the same JS task, before
        # the 300ms autosave timer. The menu is already open to avoid helper waits.
        smoke.execute("""const e=[...document.querySelectorAll('input[type=range]')].find(e=>e.parentElement.parentElement.textContent.trim().startsWith('Exposure'));
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,0.75);
          e.dispatchEvent(new Event('input',{bubbles:true}));
          [...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;""")
        wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].some(e=>e.textContent.includes('Existing RapidRoom edits are preserved'));"), name+' outcome')
        after = image_state(url, image, token=token)
        assert after['adjustments']['exposure'] == 0.75, after
        expected = dict(initial['adjustments'], exposure=0.75)
        assert after['adjustments'] == expected, 'Metadata-only import changed development values'
        assert sidecar.read_text() == xml, 'Import rewrote its source despite ordinary XMP sync being enabled'
        stored = json.loads((case / 'input/smoke.ARW.rrdata').read_text())
        assert stored['adjustments']['exposure'] == 0.75
        assert stored['rating'] == (5 if name == 'already-applied' else 3)
        dismiss()
        changed = smoke.stable_preview('photo-policy-'+name)
        assert ImageChops.difference(smoke.crop(baseline), smoke.crop(changed)).getbbox() is not None
        sidecar.unlink()
        smoke.execute("document.querySelector('button[data-tooltip^=\"Undo (\"]').click();return true;")
        wait_for(lambda: image_state(url, image, token=token)['adjustments'] == initial['adjustments'], name+' Undo keeps preceding draft history')
        undone = smoke.stable_preview('photo-policy-'+name+'-undo')
        assert ImageChops.difference(smoke.crop(baseline), smoke.crop(undone)).getbbox() is None
        smoke.step('public '+name+' import preserves pending draft, incoming bytes and Undo', {'sync_enabled':True,'exposure':0.75,'rating':stored['rating']})

    xml = packet('a:AlreadyApplied="True" a:Exposure2012="4" xmp:Rating="4"', curve=True)
    sidecar.write_text(xml)
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-folderTree\"]').click();return true;")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('span')].some(e=>e.textContent.trim()==='input');"), 'policy source folder row')
    smoke.execute("""const e=[...document.querySelectorAll('span')].find(e=>e.textContent.trim()==='input');
      const row=e.closest('div.cursor-pointer');const r=row.getBoundingClientRect();
      row.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:r.left+20,clientY:r.top+10}));return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=menuitem]')].some(e=>e.textContent.trim()==='Import XMP Adjustments');"), 'public folder import menu')
    smoke.execute("""const e=[...document.querySelectorAll('input[type=range]')].find(e=>e.parentElement.parentElement.textContent.trim().startsWith('Exposure'));
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,0.75);e.dispatchEvent(new Event('input',{bubbles:true}));
      [...document.querySelectorAll('[role=menuitem]')].find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=alert]')].some(e=>e.textContent.includes('XMP import report:'));"), 'folder policy result')
    assert image_state(url,image,token=token)['adjustments'] == dict(initial['adjustments'], exposure=0.75)
    stored = json.loads((case / 'input/smoke.ARW.rrdata').read_text())
    assert stored['adjustments']['exposure'] == 0.75 and stored['rating'] == 4
    assert sidecar.read_text() == xml
    dismiss()
    smoke.stable_preview('photo-policy-folder-pending')
    sidecar.unlink()
    smoke.execute("document.querySelector('button[data-tooltip^=\"Undo (\"]').click();return true;")
    wait_for(lambda: image_state(url,image,token=token)['adjustments'] == initial['adjustments'], 'folder metadata import preserves preceding Undo')
    undone = smoke.stable_preview('photo-policy-folder-undo')
    assert ImageChops.difference(smoke.crop(baseline),smoke.crop(undone)).getbbox() is None
    smoke.step('public folder AlreadyApplied import preserves active pending draft, input bytes and history')

    for flag in ['a:AlreadyApplied="False"', '']:
        xml = packet(flag+' a:Exposure2012="1.25"', curve=True)
        sidecar.write_text(xml)
        import_menu()
        click_import()
        wait_for(lambda: image_state(url, image, token=token)['adjustments']['exposure'] == 1.25, 'ordinary photo settings apply')
        state = image_state(url, image, token=token)
        assert state['adjustments']['curves']['luma'] == [{'x':0,'y':0},{'x':128,'y':144},{'x':255,'y':255}]
        dismiss()
        smoke.stable_preview('photo-policy-ordinary-'+('false' if flag else 'absent'))
        assert sidecar.read_text() == xml, 'Ordinary import rewrote its incoming packet'
        restore()
    smoke.step('False and absent photo flags retain ordinary exposure and curve import')

    # Exercise the real preset importer, then reload the public browser to apply it.
    preset_file = case / 'AlreadyApplied-reusable.xmp'
    preset_file.write_text(packet('a:AlreadyApplied="True" a:Name="AlreadyApplied reusable" a:Exposure2012="1.25"'))
    smoke.execute("""window.__photoPolicyPreset={pending:true};window.__TAURI_INTERNALS__.invoke('handle_import_presets_from_file',{filePath:arguments[0]}).then(value=>window.__photoPolicyPreset={value},error=>window.__photoPolicyPreset={error:String(error)});return true;""",[str(preset_file)])
    wait_for(lambda: smoke.execute('return !window.__photoPolicyPreset.pending;'), 'native reusable preset importer')
    imported = smoke.execute('return window.__photoPolicyPreset;')
    assert not imported.get('error'), imported
    smoke.execute('setTimeout(()=>location.reload(),0);return true;')
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.innerText.trim()==='Continue Session'&&!e.disabled);"), 'policy preset frontend reload')
    smoke.execute("[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Continue Session').click();return true;")
    result = rpc(url,'tools/call',{'name':'select_image','arguments':{'imagePath':image}},token=token)
    assert not result.get('isError'), result
    wait_for(lambda: smoke.execute("return document.body.innerText.includes('Exposure');"), 'preset photo ready')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-presets\"]').click();return true;")
    wait_for(lambda: smoke.execute('return !!document.querySelector(\'input[aria-label="Search presets or groups"]\');'), 'policy preset browser')
    smoke.execute("""const e=document.querySelector('input[aria-label="Search presets or groups"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'AlreadyApplied reusable');e.dispatchEvent(new Event('input',{bubbles:true}));return true;""")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('[role=button]')].some(e=>e.textContent.trim()==='AlreadyApplied reusable');"), 'reusable preset row')
    smoke.execute("[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='AlreadyApplied reusable').click();return true;")
    wait_for(lambda: image_state(url,image,token=token)['adjustments']['exposure']==1.25, 'AlreadyApplied reusable preset edits the photo')
    smoke.step('real preset importer and public browser retain AlreadyApplied reusable development settings')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    restore()
    final = smoke.stable_preview('photo-policy-original-restored')
    assert ImageChops.difference(smoke.crop(baseline),smoke.crop(final)).getbbox() is None
    assert hashlib.sha256(raw.read_bytes()).hexdigest() == raw_sha
    settings = json.loads(settings_path.read_text())
    if old_sync is None: settings.pop('enableXmpSync',None)
    else: settings['enableXmpSync']=old_sync
    settings_path.write_text(json.dumps(settings))
    (case / 'photo-policy-result.json').write_text(json.dumps({'real_adobe_application':False,'real_model_requests':False,'raw_unchanged':True,'source_xmp_read_only':True,'pending_metadata_history_preserved':True,'public_folder_pending_history_preserved':True,'ordinary_photo_flags_import':True,'reusable_preset_import':True,'original_pixels_restored':True},indent=2)+'\n')
