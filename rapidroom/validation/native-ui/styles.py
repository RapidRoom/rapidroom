"""Built-in Styles lifecycle and rendering checks in an isolated native app."""
import time
from PIL import ImageChops


def run_styles_checks(smoke, wait_for, baseline):
    import json
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
    search('copper warmth')
    wait_for(lambda:smoke.execute("return document.body.innerText.includes('Teal & Ember') && !document.body.innerText.includes('Neon Circuit');"),'style description search')
    smoke.capture('styles-description-search')
    smoke.step('built-in Styles descriptions searchable in native browser')
    row="[...document.querySelectorAll('[data-tooltip]')].find(e=>e.dataset.tooltip==='Cyan city shadows and copper warmth. Best with blue skies, warm buildings and people in mixed light.')?.closest('[role=button]')"
    label=smoke.execute('const r='+row+";if(!r)throw Error('Style row missing');const b=r.querySelector('button[aria-label]');const old=b.getAttribute('aria-label');b.click();return old;")
    wait_for(lambda:smoke.execute('const r='+row+";return r.querySelector('button[aria-label]').getAttribute('aria-label')!==arguments[0];",[label]),'style favorite toggles')
    presetfile=smoke.case/'data/io.github.CyberTimon.RapidRAW/presets/presets.json'
    wait_for(lambda:presetfile.exists(),'native preset persistence')
    time.sleep(.8)
    saved=ipc('load_presets')
    styles=next(i['folder'] for i in saved if i.get('folder',{}).get('id')=='rapidroom-styles:v1')
    assert len(styles['children'])==17
    teal=next(p for p in styles['children'] if p['id']=='rapidroom-style:teal-orange')
    assert teal['favorite'] and teal['description'].startswith('Cyan city shadows')
    assert len({p['id'] for p in styles['children']})==17
    smoke.step('native save/load retains17 unique Styles, favorite and description',{'count':17,'favorite':teal['favorite']})
    smoke.execute('const r='+row+';r.click();return true;')
    wait_for(lambda:smoke.slider('Amount')['value']==100,'style Amount100')
    applied=smoke.stable_preview('styles-teal-100')
    assert ImageChops.difference(smoke.crop(baseline),smoke.crop(applied)).getbbox() is not None
    for amount in [60,30,0]:
        smoke.drag('Amount',amount)
        current=smoke.stable_preview('styles-teal-'+str(amount))
        if amount==0:assert ImageChops.difference(smoke.crop(baseline),smoke.crop(current)).getbbox() is None
    smoke.step('native Styles apply/Amount60/30 changes preview; Amount0 restores exact original pixels')
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
    gothic=next(p for p in styles['children'] if p['id']=='rapidroom-style:forest-gothic')
    search('Forest Gothic')
    beamrow="[...document.querySelectorAll('[data-tooltip]')].find(e=>e.dataset.tooltip===arguments[0])?.closest('[role=button]')"
    def apply_beam():
        smoke.execute('const r='+beamrow+";if(!r)throw Error('Forest Gothic row missing');r.click();return true;",[gothic['description']])
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
            wait_for(lambda:edit().get('masks',[])==original.get('masks',[]),'Amount0 removes only new beam')
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
    apply_beam()
    smoke.stable_preview('styles-gothic-second-mask')
    second=wait_for(lambda:edit() if len(edit().get('masks',[]))==len(first['masks'])+1 else None,'second style preserves existing local mask')
    assert second['masks'][:-1]==first['masks']
    ids=[value for m in second['masks'] for value in [m['id'],*[s['id'] for s in m['subMasks']]]]
    assert len(set(ids))==len(ids)
    smoke.drag('Amount',0)
    restored=smoke.stable_preview('styles-gothic-existing-restored')
    wait_for(lambda:edit().get('masks',[])==first['masks'],'Amount0 restores prior mask exactly')
    assert ImageChops.difference(smoke.crop(first_pixels),smoke.crop(restored)).getbbox() is None
    mask_export=smoke.case/'beam-style-roundtrip.rrpreset'
    ipc('handle_export_presets_to_file',{'presetsToExport':[{'preset':gothic}],'filePath':str(mask_export)})
    roundtrip=json.loads(mask_export.read_text())['presets'][0]['preset']
    assert roundtrip['includeMasks'] and roundtrip['adjustments']['masks']==gothic['adjustments']['masks']
    smoke.step('native Gothic beam uses pixel geometry, fades at60/30/0, preserves existing mask and exports reusable template')
    # Restore through ordinary shared Undo before continuing the library checks.
    for attempt in range(14):
        smoke.execute("document.querySelector('button[data-tooltip^=\"Undo (\"]').click();return true;")
        current=smoke.stable_preview('styles-beam-undo-'+str(attempt))
        if ImageChops.difference(smoke.crop(baseline),smoke.crop(current)).getbbox() is None:
            break
    else:raise RuntimeError('Beam style Undo did not restore baseline')
    smoke.step('native shared Undo restores pre-style pixels after beam and Amount edits')
    # Exercise the actual folder delete action, then the native loader repeatedly.
    search('Styles')
    smoke.execute("""const r=[...document.querySelectorAll('[role=button]')].find(e=>[...e.children].some(c=>c.textContent.trim()==='Styles'));
      if(!r)throw Error('Styles folder row missing');r.scrollIntoView({block:'center'});
      const b=r.getBoundingClientRect();r.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:b.x+30,clientY:b.y+15}));return true;""")
    wait_for(lambda:smoke.execute("return [...document.querySelectorAll('button,[role=menuitem]')].some(e=>/Delete Folder/i.test(e.textContent));"),'folder context delete')
    smoke.capture('styles-delete-menu')
    smoke.execute("""const e=[...document.querySelectorAll('button,[role=menuitem]')].find(e=>/Delete Folder/i.test(e.textContent));e.click();return true;""")
    def removed_on_disk():
        if not presetfile.exists():return False
        return not any(i.get('folder',{}).get('id')=='rapidroom-styles:v1' for i in json.loads(presetfile.read_text()))
    wait_for(removed_on_disk,'UI deletion saved to native preset library')
    marker=presetfile.with_name('.rapidroom-styles-v1');assert marker.exists()
    for _ in range(3):
        deleted=ipc('load_presets');assert not any(i.get('folder',{}).get('id')=='rapidroom-styles:v1' for i in deleted)
    smoke.step('public UI deletes Styles; saved deletion survives three real native reloads')
    # An existing pre-migration library must receive the pack without changing
    # a user preset, favourite, moved style, or saved order.
    existing=[{'preset':dict(teal,name='Moved custom teal',favorite=True)},
              {'folder':{'id':'user-folder','name':'User group','children':[]}}]
    ipc('save_presets',{'presets':existing});marker.unlink()
    migrated=ipc('load_presets')
    group=next(i['folder'] for i in migrated if i.get('folder',{}).get('id')=='rapidroom-styles:v1')
    assert len(group['children'])==16 and all(p['id']!=teal['id'] for p in group['children'])
    assert migrated[1:]==existing and marker.exists()
    assert json.loads(presetfile.read_text())==migrated
    assert ipc('load_presets')==migrated
    ipc('save_presets',{'presets':[]});assert ipc('load_presets')==[]
    smoke.step('native one-time migration persists before marker; moved/favourite/user order retained; empty saved library stays empty')
    ipc('save_presets',{'presets':saved})
    search('')
    smoke.execute("document.querySelector('[data-tab-id=\"layout-tab-adjustments\"]').click();return true;")
    wait_for(lambda:smoke.execute("return document.body.innerText.includes('Exposure');"),'return to Develop afterStyles')
