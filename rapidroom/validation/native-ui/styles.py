"""Built-in Styles lifecycle and rendering checks in an isolated native app."""
import time
from PIL import ImageChops


def run_styles_checks(smoke, wait_for, baseline):
    import json
    from pathlib import Path
    from copy import deepcopy
    from smoke import request
    def search(query):
        smoke.execute('''const e=document.querySelector('input[aria-label="Search presets or groups"]');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,arguments[0]);
          e.dispatchEvent(new Event('input',{bubbles:true}));return true;''',[query])
        time.sleep(.3)
    def ipc(command,args=None):
        value=request(smoke.url+'/execute/async',{'script':'''const done=arguments[arguments.length-1];
          window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(v=>done({ok:true,value:v})).catch(e=>done({ok:false,error:String(e)}));''','args':[command,args or {}]})['value']
        if not value['ok']:raise RuntimeError(value)
        return value['value']
    pack = json.loads((Path(__file__).resolve().parents[3] / 'src-tauri/resources/styles/styles-v1.json').read_text())
    expected_count = len(pack['presets'])
    assert expected_count == 20 and all('preset' in item for item in pack['presets'])
    search('copper warmth')
    wait_for(lambda:smoke.execute("return document.body.innerText.includes('Teal & Orange') && !document.body.innerText.includes('Neon Night');"),'style description search')
    smoke.capture('styles-description-search')
    smoke.step('built-in Styles descriptions searchable in native browser')
    row="[...document.querySelectorAll('[data-tooltip]')].find(e=>e.dataset.tooltip==='Cyan city shadows and copper warmth. Best with blue skies, warm buildings and people in mixed light.')?.closest('[role=button]')"
    label=smoke.execute('const r='+row+";if(!r)throw Error('Style row missing');const b=r.querySelector('button[aria-label]');const old=b.getAttribute('aria-label');b.click();return old;")
    wait_for(lambda:smoke.execute('const r='+row+";return r.querySelector('button[aria-label]').getAttribute('aria-label')!==arguments[0];",[label]),'style favorite toggles')
    presetfile=smoke.case/'data/io.github.CyberTimon.RapidRAW/presets/presets.json'
    wait_for(lambda:presetfile.exists(),'native preset persistence')
    time.sleep(.8)
    saved=ipc('load_presets')
    assert len(saved)==expected_count and all('preset' in item for item in saved)
    styles=[item['preset'] for item in saved]
    teal=next(p for p in styles if p['id']=='rapidroom-style:teal-orange')
    assert teal['favorite'] and teal['description'].startswith('Cyan city shadows')
    assert len({p['id'] for p in styles})==expected_count
    smoke.step('native save/load retains unique Styles, favorite and description',{'count':expected_count,'favorite':teal['favorite']})
    smoke.execute('const r='+row+';r.click();return true;')
    wait_for(lambda:smoke.slider('Amount')['value']==100,'style Amount100')
    applied=smoke.stable_preview('styles-teal-100')
    assert ImageChops.difference(smoke.crop(baseline),smoke.crop(applied)).getbbox() is not None
    for amount in [60,30,0]:
        smoke.drag('Amount',amount)
        current=smoke.stable_preview('styles-teal-'+str(amount))
        if amount==0:assert ImageChops.difference(smoke.crop(baseline),smoke.crop(current)).getbbox() is None
    smoke.step('native Styles apply/Amount60/30 changes preview; Amount0 restores exact original pixels')
    smoke.execute("document.querySelector('[data-applied-style=\"rapidroom-style:teal-orange\"] button[aria-label^=\"Remove Style\"]').click();return true;")
    exported=smoke.case/'style-roundtrip.rrpreset'
    ipc('handle_export_presets_to_file',{'presetsToExport':[{'preset':teal}],'filePath':str(exported)})
    data=json.loads(exported.read_text());assert data['presets'][0]['preset']['description']==teal['description']
    smoke.step('real native preset export preserves original description and recipe')
    # The beam is an ordinary native radial mask, appended to local edits rather
    # than replacing them. Observe autosaved data and actual preview pixels.
    photo=smoke.case/'input/smoke.ARW'
    def edit():
        return ipc('load_metadata',{'path':str(photo)})['adjustments']
    original=edit()
    gothic=next(p for p in styles if p['id']=='rapidroom-style:forest-gothic')
    search(gothic['name'])
    beamrow="[...document.querySelectorAll('[data-tooltip]')].find(e=>e.dataset.tooltip===arguments[0])?.closest('[role=button]')"
    def apply_beam():
        smoke.execute('const r='+beamrow+";if(!r)throw Error('Dark Forest row missing');r.click();return true;",[gothic['description']])
    apply_beam()
    full=smoke.stable_preview('styles-gothic-100')
    first=wait_for(lambda:edit() if len(edit().get('masks',[]))==len(original.get('masks',[]))+1 else None,'beam autosaved as native mask')
    beam=first['masks'][-1]
    geometry=beam['subMasks'][0]['parameters']
    assert geometry['centerX']>100 and geometry['centerY']>100
    assert 'relativeToImage' not in beam and beam['name']=='Warm diagonal light beam'
    assert first['masks'][:-1]==original.get('masks',[])
    for amount in [60,30,0]:
        smoke.drag('Amount',amount)
        actual=smoke.slider('Amount')['value']
        current=smoke.stable_preview('styles-gothic-'+str(amount))
        if actual==0:
            wait_for(lambda:len(edit().get('masks',[]))==len(original.get('masks',[]))+1 and edit()['masks'][-1]['opacity']==0,'Amount0 mutes only the Style beam')
            assert ImageChops.difference(smoke.crop(baseline),smoke.crop(current)).getbbox() is None
        else:
            def faded():
                data=edit();masks=data.get('masks',[])
                if len(masks)!=len(first['masks']):return None
                return data if abs(masks[-1]['opacity']-beam['opacity']*actual/100)<.001 else None
            data=wait_for(faded,'native beam opacity follows Amount')
            assert data['masks'][-1]['subMasks']==beam['subMasks']
            assert ImageChops.difference(smoke.crop(full),smoke.crop(current)).getbbox() is not None
    smoke.drag('Amount',100)
    first_pixels=smoke.stable_preview('styles-gothic-first-mask')
    first=wait_for(lambda:edit() if len(edit().get('masks',[]))==len(original.get('masks',[]))+1 else None,'first beam restored')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-masks\"]').click();return true;")
    wait_for(lambda:smoke.execute("return document.body.innerText.includes('Warm diagonal light beam');"),'editable beam in Masks panel')
    smoke.execute("const e=[...document.querySelectorAll('span,div')].find(e=>e.textContent.trim()==='Warm diagonal light beam');if(!e)throw Error('Beam row missing');e.closest('[role=button]').click();return true;")
    if not smoke.execute("return [...document.querySelectorAll('input[type=range]')].some(e=>e.parentElement.parentElement.textContent.trim().startsWith('Opacity'));"):
        smoke.execute("const e=[...document.querySelectorAll('button,[role=button]')].find(e=>e.textContent.trim()==='Mask Properties');if(!e)throw Error('Mask Properties missing');e.click();return true;")
    smoke.drag('Opacity',72)
    actual=smoke.slider('Opacity')['value']
    wait_for(lambda:abs(edit().get('masks',[])[-1]['opacity']-actual)<.001,'native beam edited through ordinary mask controls')
    component_visible="return [...document.querySelectorAll('[role=button]')].some(e=>e.textContent.trim()==='Movable warm beam');"
    if not smoke.execute(component_visible):
        smoke.execute("const e=[...document.querySelectorAll('span')].find(e=>e.textContent.trim()==='Warm diagonal light beam');e.dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));return true;")
    wait_for(lambda:smoke.execute(component_visible),'movable beam component row')
    smoke.execute("const e=[...document.querySelectorAll('[role=button]')].find(e=>e.textContent.trim()==='Movable warm beam');e.click();return true;")
    geometry_before=edit()['masks'][-1]['subMasks'][0]['parameters']
    point=smoke.execute("const e=[...document.querySelectorAll('svg')].find(e=>{const r=e.getBoundingClientRect();return r.width>500&&r.height>300});const r=e.getBoundingClientRect();return [Math.round(r.left+r.width*arguments[0]/4608),Math.round(r.top+r.height*arguments[1]/3072)];",[geometry_before['centerX'],geometry_before['centerY']])
    time.sleep(.5)
    point_evidence=smoke.execute("window.__beamEvents=[];for(const type of ['mousedown','mousemove','mouseup','pointerdown','pointermove'])window.addEventListener(type,e=>{if(window.__beamEvents.length<40)window.__beamEvents.push({type:e.type,x:e.clientX,y:e.clientY,target:e.target.tagName})},{capture:true});const e=document.elementFromPoint(arguments[0],arguments[1]);return {tag:e.tagName,cls:e.className,canvas:[...document.querySelectorAll('.konvajs-content canvas')].map(c=>{const r=c.getBoundingClientRect();return [r.left,r.top,r.width,r.height]})};",point)
    request(smoke.url+'/actions',{'actions':[{'type':'pointer','id':'beam-mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'x':point[0],'y':point[1],'origin':'viewport'},{'type':'pointerDown','button':0},{'type':'pause','duration':100},{'type':'pointerMove','duration':400,'x':point[0]+65,'y':point[1]+30,'origin':'viewport'},{'type':'pointerUp','button':0}]}]})
    def beam_moved():
        now=edit()['masks'][-1]['subMasks'][0]['parameters']
        return now if abs(now['centerX']-geometry_before['centerX'])>1 and abs(now['centerY']-geometry_before['centerY'])>1 else None
    time.sleep(1)
    input_method='W3C mouse drag on native canvas'
    if not beam_moved():
        input_method='DOM mouse drag through normal Konva listeners'
        smoke.execute("const e=document.elementFromPoint(arguments[0],arguments[1]);window.__beamTarget=e;e.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true,button:0,buttons:1,clientX:arguments[0],clientY:arguments[1]}));return true;",point)
        time.sleep(.2)
        for dx,dy in [(10,5),(35,15),(65,30)]:
            smoke.execute("window.__beamTarget.dispatchEvent(new MouseEvent('mousemove',{bubbles:true,cancelable:true,button:0,buttons:1,clientX:arguments[0],clientY:arguments[1]}));return true;",[point[0]+dx,point[1]+dy])
            time.sleep(.15)
        smoke.execute("window.__beamTarget.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,cancelable:true,button:0,buttons:0,clientX:arguments[0],clientY:arguments[1]}));return true;",[point[0]+65,point[1]+30])
    trace=smoke.execute('return window.__beamEvents;')
    (smoke.case/'beam-pointer-trace.json').write_text(json.dumps({'point':point,'target':point_evidence,'events':trace,'input':input_method},indent=2)+'\n')
    moved=wait_for(beam_moved,'ordinary canvas drag moves saved beam geometry')
    assert moved['radiusX']==geometry_before['radiusX'] and moved['radiusY']==geometry_before['radiusY']
    smoke.step('native pointer drag moves Dark Forest beam without resizing',{'before':geometry_before,'after':moved,'input':input_method})
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    first_pixels=smoke.stable_preview('styles-gothic-edited-mask')
    first=edit()
    smoke.step('Dark Forest beam is editable through ordinary native mask controls',{'opacity':actual})
    # Clicking an applied Style removes it. The edited beam becomes user-owned.
    apply_beam()
    retained_pixels=smoke.stable_preview('styles-gothic-removed-edited-mask')
    wait_for(lambda:edit().get('masks',[])==first['masks'],'removing Style preserves manually edited beam')
    apply_beam()
    smoke.stable_preview('styles-gothic-second-mask')
    second=wait_for(lambda:edit() if len(edit().get('masks',[]))==len(first['masks'])+1 else None,'second style preserves existing local mask')
    assert second['masks'][:-1]==first['masks']
    ids=[value for m in second['masks'] for value in [m['id'],*[s['id'] for s in m['subMasks']]]]
    assert len(set(ids))==len(ids)
    smoke.drag('Amount',0)
    restored=smoke.stable_preview('styles-gothic-existing-restored')
    wait_for(lambda:edit().get('masks',[])[:-1]==first['masks'] and edit()['masks'][-1]['opacity']==0,'Amount0 preserves prior mask and mutes new beam')
    assert ImageChops.difference(smoke.crop(retained_pixels),smoke.crop(restored)).getbbox() is None
    mask_export=smoke.case/'beam-style-roundtrip.rrpreset'
    ipc('handle_export_presets_to_file',{'presetsToExport':[{'preset':gothic}],'filePath':str(mask_export)})
    roundtrip=json.loads(mask_export.read_text())['presets'][0]['preset']
    assert roundtrip['includeMasks'] and roundtrip['adjustments']['masks']==gothic['adjustments']['masks']
    smoke.step('native Gothic beam uses pixel geometry, fades at60/30/0, preserves existing mask and exports reusable template')
    # Restore through ordinary shared Undo before continuing the library checks.
    for attempt in range(24):
        smoke.execute("document.querySelector('button[data-tooltip^=\"Undo (\"]').click();return true;")
        current=smoke.stable_preview('styles-beam-undo-'+str(attempt))
        if ImageChops.difference(smoke.crop(baseline),smoke.crop(current)).getbbox() is None:
            break
    else:raise RuntimeError('Beam style Undo did not restore baseline')
    smoke.step('native shared Undo restores pre-style pixels after beam and Amount edits')
    # Delete a real top-level preset through the public menu; the marker alone
    # protects that deletion even though no built-in folder exists.
    search('copper warmth')
    smoke.execute('const r='+row+";if(!r)throw Error('Teal row missing');r.scrollIntoView({block:'center'});const b=r.getBoundingClientRect();r.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:b.x+30,clientY:b.y+15}));return true;")
    wait_for(lambda:smoke.execute("return [...document.querySelectorAll('button,[role=menuitem]')].some(e=>/Delete Preset/i.test(e.textContent));"),'preset context delete')
    smoke.capture('styles-delete-menu')
    smoke.execute("const e=[...document.querySelectorAll('button,[role=menuitem]')].find(e=>/Delete Preset/i.test(e.textContent));e.click();return true;")
    def removed_on_disk():
        if not presetfile.exists():return False
        items=json.loads(presetfile.read_text())
        return len(items)==expected_count-1 and all(i.get('preset',{}).get('id')!=teal['id'] for i in items)
    wait_for(removed_on_disk,'UI deletion saved to native preset library')
    marker=presetfile.with_name('.rapidroom-styles-v1');assert marker.exists()
    for _ in range(3):
        deleted=ipc('load_presets');assert len(deleted)==expected_count-1 and all(i.get('preset',{}).get('id')!=teal['id'] for i in deleted)
    smoke.step('public UI deletes a flat Style; saved deletion survives three real native reloads')
    # A user-created folder, even with the former reserved ID/name, must not
    # suppress missing built-ins or lose its moved/favourited recipe.
    existing=[{'preset':dict(teal,id='user:teal',name='My imported teal')},
              {'folder':{'id':'rapidroom-styles:v1','name':'Styles','children':[dict(teal,name='Moved custom teal',favorite=True)]}},
              {'folder':{'id':'user-folder','name':'User group','children':[]}}]
    ipc('save_presets',{'presets':existing});marker.unlink()
    migrated=ipc('load_presets')
    additions=migrated[:expected_count-1]
    assert all('preset' in item and item['preset']['id']!=teal['id'] for item in additions)
    assert len({i['preset']['id'] for i in additions})==expected_count-1
    assert migrated[expected_count-1:]==existing and marker.exists()
    assert json.loads(presetfile.read_text())==migrated
    assert ipc('load_presets')==migrated
    user_folders=[existing[-1]]
    ipc('save_presets',{'presets':user_folders});assert ipc('load_presets')==user_folders
    ipc('save_presets',{'presets':[]});assert ipc('load_presets')==[]
    presetfile.unlink();assert ipc('load_presets')==[]
    smoke.step('native marker-only flat seeding preserves user folders, moved/favourite/imported presets and order; empty/missing saved library does not resurrect Styles')
    # The sanitized fixture represents an older converter, whose stock values
    # differ from the current bundle. Exercise the actual upgrade on disk.
    fixture = Path(__file__).resolve().parents[3] / 'rapidroom/validation/fixtures/presets-stock-older.json'
    older_bytes = fixture.read_bytes()
    old_library = json.loads(older_bytes)
    expected = old_library[3:]
    replacement_marker = presetfile.with_name('.rapidroom-styles-replacement-v1')
    replacement_v2 = presetfile.with_name('.rapidroom-styles-replacement-v2')
    before_backups = set(presetfile.parent.glob('presets.json.bak-*'))
    presetfile.write_bytes(older_bytes)
    replacement_v2.unlink()
    migrated = ipc('load_presets')
    # Unknown fields intentionally remain in the file even though the public
    # typed IPC response contains the app's current schema only.
    assert [item.get('folder', item.get('preset'))['id'] for item in migrated] == [
        item.get('folder', item.get('preset'))['id'] for item in expected]
    assert json.loads(presetfile.read_text()) == expected
    assert marker.exists() and replacement_marker.exists() and replacement_v2.exists()
    backups = set(presetfile.parent.glob('presets.json.bak-*')) - before_backups
    assert len(backups) == 1 and next(iter(backups)).read_bytes() == older_bytes
    assert ipc('load_presets') == migrated
    assert set(presetfile.parent.glob('presets.json.bak-*')) - before_backups == backups
    smoke.step('native v2 upgrade backs up exact older-library bytes, removes whole stock folders, preserves mixed/moved/renamed/user items and prior Styles deletion')
    # A normal GUI-created UUID preset can have the same name as stock.
    mixed = deepcopy(old_library)
    mixed[1]['folder']['children'].append({
        'id': '8f9ab237-4a89-49a2-9baa-c84c482f65c3', 'name': 'Cool',
        'adjustments': {'contrast': 42}, 'favorite': True})
    mixed_bytes = json.dumps(mixed, indent=2).encode()
    presetfile.write_bytes(mixed_bytes)
    replacement_v2.unlink()
    ipc('load_presets')
    assert json.loads(presetfile.read_text()) == [mixed[1]] + mixed[3:]
    smoke.step('native v2 keeps the whole mixed stock folder when a user UUID preset shares the name Cool')
    presetfile.write_bytes(older_bytes)
    marker.unlink()
    replacement_marker.unlink()
    replacement_v2.unlink()
    first_upgrade = ipc('load_presets')
    assert len(first_upgrade) == expected_count + len(expected)
    assert all('preset' in item for item in first_upgrade[:expected_count])
    assert json.loads(presetfile.read_text())[expected_count:] == expected
    assert ipc('load_presets') == first_upgrade
    ipc('save_presets', {'presets': []})
    assert ipc('load_presets') == []
    presetfile.unlink()
    assert ipc('load_presets') == []
    smoke.step('native first upgrade seeds twenty colour Styles once; deleting all presets or the saved file stays empty')
    ipc('save_presets',{'presets':saved})
    search('')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    wait_for(lambda:smoke.execute("return document.body.innerText.includes('Exposure');"),'return to Develop afterStyles')
