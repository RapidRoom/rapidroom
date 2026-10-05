"""Native stepped dock layout and compact-header checks on owned CC0 photos."""
import json
import time


def run_layout_checks(case, smoke):
    from smoke import save, wait_for

    def click(selector):
        smoke.execute("const e=document.querySelector(arguments[0]);if(!e)throw Error(arguments[0]);e.click();return true;", [selector])

    click('[data-tab-id="layout-tab-terminal"]')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-terminal-open]');"), 'terminal open control')
    click('[data-terminal-open]')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-terminal-panel] textarea');"), 'real terminal')
    smoke.terminal_type("printf 'RR_LAYOUT_READY\\n'\r")
    wait_for(lambda: 'RR_LAYOUT_READY' in smoke.terminal_text(), 'layout PTY output')

    def geometry(view, region, size, index):
        data = smoke.execute(r"""
          const rect=e=>{const r=e.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
          const visible=e=>!!e && rect(e).width>0 && rect(e).height>0 && getComputedStyle(e).visibility!=='hidden';
          const clip=e=>{const r=rect(e);for(let p=e.parentElement;p;p=p.parentElement){const s=getComputedStyle(p),q=rect(p);
            if(/hidden|clip|scroll|auto/.test(s.overflowX)){r.left=Math.max(r.left,q.left);r.right=Math.min(r.right,q.right)}
            if(/hidden|clip|scroll|auto/.test(s.overflowY)){r.top=Math.max(r.top,q.top);r.bottom=Math.min(r.bottom,q.bottom)}}
            r.width=Math.max(0,r.right-r.left);r.height=Math.max(0,r.bottom-r.top);return r};
          const intersects=(a,b)=>Math.min(a.right,b.right)-Math.max(a.left,b.left)>1 && Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)>1;
          const terminal=document.querySelector('[data-terminal-panel]'),screen=terminal.querySelector('[data-terminal-screen]');
          const toolbar=[...document.querySelectorAll('div.shrink-0.h-12.flex.items-center.justify-between.px-3')].find(visible);
          if(!toolbar||!screen)throw Error('Layout bounds missing');
          const dock=terminal.closest('[data-layout-region]'),dockRect=rect(dock),bar=rect(toolbar),violations=[];
          if(intersects(bar,dockRect))violations.push('toolbar overlaps dock');
          const thumbs=[...document.querySelectorAll('[data-bench-id=thumbnail]')].filter(visible);
          const painted=thumbs.map(clip).filter(r=>r.width>0 && r.height>0);
          for(const r of painted){if(intersects(r,bar))violations.push('thumbnail overlaps toolbar');if(intersects(r,dockRect))violations.push('thumbnail overlaps dock')}
          let viewport=null;
          if(arguments[0]==='library'){
            const thumb=thumbs[0],list=thumb?.closest('[role=list]') || thumb?.closest('.custom-scrollbar');
            if(!list)throw Error('Real library viewport missing');viewport=rect(list);
            if(viewport.bottom>bar.top+1)violations.push('library viewport extends below toolbar');
            if(intersects(viewport,dockRect))violations.push('library viewport overlaps dock');
          }else{
            const editor=document.querySelector('[data-bench-id=editor-first-frame]')?.parentElement;
            const viewer=editor?.querySelector('[data-editor-viewport]') || editor?.querySelector('.touch-none');
            if(!viewer)throw Error('Real editor viewport missing');viewport=rect(viewer);
            if(viewport.width<=0||viewport.height<=0)violations.push('editor viewport collapsed');
            if(intersects(viewport,bar)||intersects(viewport,dockRect))violations.push('editor viewport overlaps chrome');
          }
          const header=document.querySelector('[data-terminal-header]');
          const headerHeight=rect(screen).top-(arguments[1]==='bottom'?dockRect.top+4:rect(terminal).top);
          return {view:arguments[0],region:arguments[1],requested_size:arguments[2],viewport,toolbar:bar,dock:dockRect,
            terminal:rect(terminal),header_height:headerHeight,header:header?rect(header):null,painted_thumbnails:painted,violations,
            controls:header?[...header.querySelectorAll('button')].filter(e=>!e.closest('[role=tablist]')).map(e=>({label:e.getAttribute('aria-label')||e.getAttribute('data-tooltip'),bounds:rect(e)})):[],
            agent_labels:header?[...header.querySelectorAll('[data-start-agent] span')].map(e=>({text:e.textContent,visible:rect(e).width>0})):[]};
        """, [view, region, size])
        name=f'dock-{view}-{region}-{index}-{size}'
        data['capture']=str(smoke.capture(name).relative_to(case))
        save(case / (name+'.json'),data)
        if not 28 <= data['header_height'] <= 33:
            data['violations'].append('terminal header is not one 28–32px row')
        actual=data['dock']['height'] if region=='bottom' else data['dock']['width']
        if abs(actual-size)>11:
            data['violations'].append('resize did not reach requested step within keyboard quantization')
        if data['header']:
            for control in data['controls']:
                b=control['bounds'];h=data['header']
                if not control['label'] or b['width']<=0 or b['left']<h['left']-1 or b['right']>h['right']+1 or b['top']<h['top']-1 or b['bottom']>h['bottom']+1:
                    data['violations'].append('header control clipped or unlabelled')
            expected_labels=data['terminal']['width']>=480
            if any(label['visible']!=expected_labels for label in data['agent_labels']):
                data['violations'].append('agent labels do not adapt to dock width')
            save(case / (name+'.json'),data)
        if data['violations']:
            raise RuntimeError(name+': '+', '.join(sorted(set(data['violations']))))
        smoke.step(name,data)
        return data

    def resize(region, target):
        selector='[data-bottom-dock] [role=separator]' if region=='bottom' else '[data-side-panel="'+('left' if region=='leftTop' else 'right')+'"] [data-side-resizer]'
        current=smoke.execute("const e=document.querySelector('[data-terminal-panel]').closest('[data-layout-region]');const r=e.getBoundingClientRect();return arguments[0]==='bottom'?r.height:e.closest('[data-side-panel]').getBoundingClientRect().width;", [region])
        key=('ArrowUp' if target>current else 'ArrowDown') if region=='bottom' else ('ArrowRight' if (target>current)==(region=='leftTop') else 'ArrowLeft')
        count=round(abs(target-current)/20)
        for _ in range(count):
            smoke.execute("const e=document.querySelector(arguments[0]);if(!e)throw Error('Resize control missing');e.focus();e.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,key:arguments[1]}));return true;",[selector,key])
            time.sleep(0.02)
        time.sleep(0.7)

    for view in ('library','editor'):
        if view=='editor':
            click('[data-bench-id=thumbnail]')
            smoke.execute("document.querySelector('[data-bench-id=thumbnail]').dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0}));return true;")
            wait_for(lambda: smoke.execute("return !!document.querySelector('[data-bench-id=editor-first-frame]');"),'editor native frame')
        for region in ('bottom','leftTop','rightTop'):
            if region!='bottom': smoke.terminal_drag(region)
            for index,size in enumerate((120,260,420,620,260) if region=='bottom' else (240,400,560,320)):
                resize(region,size)
                measurements=geometry(view,region,size,index)
            if not 28 <= measurements['header_height'] <= 33:
                raise RuntimeError(f"{view}/{region}: expected one 28–32px terminal header, observed {measurements['header_height']}")
            if region!='bottom': smoke.terminal_drag('bottom')
    # Exercise real header controls with owned CLI probes, not model requests.
    tabs=lambda: smoke.execute("return document.querySelectorAll('[data-terminal-header] [role=tab]').length;")
    count=tabs()
    click('[data-terminal-header] [aria-label="New terminal tab"]')
    wait_for(lambda: tabs()==count+1,'new tab from compact header')
    close_tab="const e=[...document.querySelectorAll('[data-terminal-header] button[aria-label]')].filter(e=>e.getAttribute('aria-label').startsWith('Close ')).at(-1);e.focus();e.click();return true;"
    smoke.execute(close_tab)
    wait_for(lambda: tabs()==count,'close tab from compact header')
    records=case/'tools/agents.txt'
    launch_records=[]
    for mode in ('external','built-in'):
        click('[data-terminal-header] [aria-label="Terminal preferences"]')
        smoke.execute("const e=document.querySelector('[data-terminal-panel] select');e.value=arguments[0];e.dispatchEvent(new Event('change',{bubbles:true}));return true;",[mode])
        settings=case/'data/io.github.CyberTimon.RapidRAW/settings.json'
        wait_for(lambda: json.loads(settings.read_text()).get('terminalSettings',{}).get('startIn')==mode,'gear launch mode persists')
        click('[data-terminal-header] [aria-label="Terminal preferences"]')
        for agent in ('claude','codex'):
            before=records.read_text().count(agent+'|') if records.exists() else 0
            before_tabs=tabs()
            click('[data-terminal-header] [data-start-agent="'+agent+'"]')
            wait_for(lambda: records.exists() and records.read_text().count(agent+'|')>before,mode+' header launch '+agent)
            expected=before_tabs+(mode=='built-in')
            wait_for(lambda: tabs()==expected,'header launch tab behavior')
            launch_records.append({'mode':mode,'agent':agent,'tabs_before':before_tabs,'tabs_after':tabs()})
    for _ in range(2):
        smoke.execute(close_tab)
        time.sleep(0.3)
    if tabs()!=count: raise RuntimeError('Header launch cleanup lost original tab')
    smoke.step('compact header tab, gear launch mode and both assistant controls',{'launches':launch_records,'preserved_tabs':count,'real_assistants_started':False})
    records.rename(case/'dock-header-agents.txt')
    # Restore normal minimum smoke layout through public UI controls.
    resize('bottom',260)
    click('[aria-label="Collapse bottom panel"]')
    wait_for(lambda: smoke.execute("return !document.querySelector('[data-terminal-panel]');"),'compact header collapses')
    click('[data-tab-id="layout-tab-terminal"]')
    wait_for(lambda: 'RR_LAYOUT_READY' in smoke.terminal_text(),'collapse/reopen preserves real terminal')
    smoke.step('compact header collapse/reopen preserves PTY',{'marker_occurrences':smoke.terminal_text().count('RR_LAYOUT_READY')})
    smoke.execute(close_tab)
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-terminal-open]');"),'header scenario tabs cleaned')
    click('[aria-label="Collapse bottom panel"]')
    smoke.execute("const e=[...document.querySelectorAll('button')].find(e=>/back to library/i.test(e.getAttribute('data-tooltip')||''));if(!e)throw Error('Back to Library missing');e.click();return true;")
    wait_for(lambda: smoke.execute("return ![...document.querySelectorAll('[data-bench-id=editor-first-frame]')].some(e=>e.parentElement.getBoundingClientRect().height>0);"),'library restored')
