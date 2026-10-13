#!/usr/bin/env python3
"""Opt-in real-Hyprland titlebar check; only an owned app window is operated.

Unlike smoke.py's private Weston, this uses the current Wayland compositor.
Data/config/cache and the session bus remain isolated. Captures stay local.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

from smoke import request, wait_for, sha, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--desktop', action='store_true', required=True)
    args = parser.parse_args()
    if not os.environ.get('HYPRLAND_INSTANCE_SIGNATURE') or not os.environ.get('WAYLAND_DISPLAY'):
        parser.error('This check requires the actual Hyprland Wayland session')
    case = args.out.resolve()
    if 'samples' not in case.parts:
        parser.error('Keep private native captures under samples/')
    case.mkdir(parents=True, exist_ok=False)
    result = {'passed': False, 'steps': [], 'engine_sha256': sha(args.engine)}
    def step(name, **evidence):
        result['steps'].append({'name': name, **evidence}); save(case/'result.json', result)
        print('PASS '+name, flush=True)
    env = os.environ.copy()
    for key, folder in [('XDG_DATA_HOME','data'), ('XDG_CONFIG_HOME','config'), ('XDG_CACHE_HOME','cache'), ('XDG_STATE_HOME','state')]:
        (case/folder).mkdir(); env[key] = str(case/folder)
    env.pop('DISPLAY', None); env.pop('DBUS_SESSION_BUS_ADDRESS', None)
    env.update(GDK_BACKEND='wayland', GIO_USE_VFS='local', NO_AT_BRIDGE='1')
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
    env['RAPIDROOM_NATIVE_UI_TEST_PORT']=str(port)
    settings = case/'data/io.github.CyberTimon.RapidRAW'
    settings.mkdir()
    (case/'input').mkdir()
    from PIL import Image, ImageDraw
    fixture=Image.new('RGB',(960,640));draw=ImageDraw.Draw(fixture)
    for y in range(640):draw.line((0,y,959,y),fill=(y%256,(y*2)%256,(y*3)%256))
    fixture.save(case/'input/controls.jpg',quality=90)
    save(settings/'settings.json', {'rootFolders':[str(case/'input')], 'lastRootPath':str(case/'input'), 'language':'en','useWgpuRenderer':False, 'editorPreviewResolution':1280,'decorations':False,'windowControls':'auto','mcpEnabled':False})
    with open('/tmp/rapidroom-build.lock','a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        with (case/'app.log').open('w') as log:
            process=subprocess.Popen(['nice','-n','10','dbus-run-session','--',str(args.engine.resolve())],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            base=f'http://127.0.0.1:{port}'
            def ready():
                try: return request(base+'/status')['value']['ready']
                except (OSError,RuntimeError): return False
            wait_for(ready,'native driver',90)
            session=request(base+'/session', {'capabilities':{'alwaysMatch':{}}})['value']['sessionId']
            url=base+'/session/'+session
            def js(script,args=None): return request(url+'/execute/sync',{'script':script,'args':args or []})['value']
            def state():
                data=request(url+'/execute/async', {'script':"const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke('get_window_control_state').then(done).catch(e=>done({error:String(e)}));",'args':[]})['value']
                if data.get('error'): raise RuntimeError(data['error'])
                return data
            def client():
                clients=json.loads(subprocess.check_output(['hyprctl','-j','clients'],text=True))
                matches=[]
                for item in clients:
                    try:
                        executable=Path(f"/proc/{item['pid']}/exe").resolve()
                        if executable==args.engine.resolve():matches.append(item)
                    except (KeyError,OSError): pass
                if len(matches)!=1: raise RuntimeError('Owned Hyprland window not uniquely identified')
                return matches[0]
            def capture(name):
                c=client(); x,y=c['at'];w,h=c['size']
                path=case/(name+'.png')
                subprocess.run(['grim','-g',f'{x},{y} {w}x{h}',str(path)],check=True)
                return {'capture':path.name,'geometry':[w,h],'sha256':sha(path)}
            wait_for(lambda: js("return !!document.querySelector('#root')?.children.length;"),'native frontend',90)
            js("const e=[...document.querySelectorAll('button')].find(e=>e.innerText.trim()==='Continue Session');if(!e)throw Error('Continue Session missing');e.click();return true;")
            wait_for(lambda: js("return [...document.images].some(e=>e.src.startsWith('asset:')&&e.complete&&e.naturalWidth>100);"),'native fixture thumbnail',90)
            auto=state()
            if auto['maximized'] or auto['fullscreen']:
                js("document.querySelector('button[aria-label=\"Restore window\"]').click();return true;")
                wait_for(lambda: not state()['maximized'] and not state()['fullscreen'],'initial window restored')
                auto=state()
            if auto['canMinimize'] or not auto['canMaximize']:
                raise RuntimeError('Unexpected Hyprland Auto policy: '+str(auto))
            if js("return !!document.querySelector('button[aria-label=\"Minimize window\"]');"):
                raise RuntimeError('Auto still shows unsupported Minimize')
            step('Auto hides Minimize and retains Maximize and Close on the actual Hyprland socket', state=auto, **capture('controls-auto'))
            # Force Always show through the real Settings dropdown, then check
            # the actual no-op rather than presuming from environment alone.
            js("document.activeElement?.blur();document.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,code:'Comma',key:',',ctrlKey:true}));return true;")
            wait_for(lambda: js("return !!document.querySelector('[data-window-controls-setting]');"),'window controls setting')
            def choose(label):
                js("document.querySelector('[data-window-controls-setting] button').click();return true;")
                wait_for(lambda: js("return [...document.querySelectorAll('[role=option]')].some(e=>e.textContent.trim()===arguments[0]);",[label]),'override '+label)
                js("[...document.querySelectorAll('[role=option]')].find(e=>e.textContent.trim()===arguments[0]).click();return true;",[label])
            choose('Always show')
            wait_for(lambda: json.loads((settings/'settings.json').read_text()).get('windowControls')=='always','override persisted')
            step('Settings Always show persists', **capture('controls-always-setting'))
            js("document.activeElement?.blur();document.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,code:'Escape',key:'Escape'}));return true;")
            wait_for(lambda: js("return !!document.querySelector('button[aria-label=\"Minimize window\"]');"),'Always show exposes Minimize')
            js("document.querySelector('button[aria-label=\"Minimize window\"]').click();return true;")
            time.sleep(.6)
            minimized=state(); c=client()
            if minimized['minimized'] or c.get('hidden'):
                raise RuntimeError('Hyprland honoured Minimize; the Auto hide policy must be revised')
            step('Minimize confirmed a no-op on the actual Hyprland compositor', state=minimized, compositor_hidden=bool(c.get('hidden')))
            # Observe the granted state, including Hyprland fullscreen-1.
            js("document.querySelector('button[aria-label=\"Maximize window\"]').click();return true;")
            granted=wait_for(lambda: (lambda s:s if s['maximized'] or s['fullscreen'] else False)(state()),'compositor grants maximize',15)
            wait_for(lambda: js("return !!document.querySelector('button[aria-label=\"Restore window\"]');"),'icon follows granted maximize')
            step('Maximize is honoured and the icon follows actual state', state=granted, compositor_fullscreen=client().get('fullscreen'), **capture('controls-maximized'))
            js("document.querySelector('button[aria-label=\"Restore window\"]').click();return true;")
            wait_for(lambda: not state()['maximized'] and not state()['fullscreen'],'restore granted',15)
            wait_for(lambda: js("return !!document.querySelector('button[aria-label=\"Maximize window\"]');"),'restore icon returns')
            step('Restore reverses the granted state', state=state(), **capture('controls-restored'))
            js("document.activeElement?.blur();document.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,code:'Comma',key:',',ctrlKey:true}));return true;")
            wait_for(lambda: js("return !!document.querySelector('[data-window-controls-setting]');"),'reopen settings')
            choose('Hide')
            wait_for(lambda: json.loads((settings/'settings.json').read_text()).get('windowControls')=='hide','Hide persisted')
            js("document.activeElement?.blur();document.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,code:'Escape',key:'Escape'}));return true;")
            wait_for(lambda: js("return !document.querySelector('button[aria-label=\"Minimize window\"]')&&!document.querySelector('button[aria-label=\"Maximize window\"]')&&!!document.querySelector('button[aria-label=\"Close window\"]');"),'Hide keeps Close')
            step('Hide override leaves Close accessible', **capture('controls-hidden'))
            errors=js('return window.__RAPIDROOM_SMOKE_ERRORS__;')
            if errors: raise RuntimeError('Native frontend errors: '+str(errors))
            js("document.querySelector('button[aria-label=\"Close window\"]').click();return true;")
            process.wait(timeout=15)
            if process.returncode: raise RuntimeError('Owned app did not exit cleanly')
            result['passed']=True;save(case/'result.json',result)
        except Exception as error:
            result['error']=str(error);save(case/'result.json',result);raise
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    if sha(args.engine)!=result['engine_sha256']:raise RuntimeError('Pinned engine changed')


if __name__=='__main__': main()
