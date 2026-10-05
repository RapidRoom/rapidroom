"""Native consent/runtime checks using app-generated assistant registration."""
import json
import os
import socket
import stat
import urllib.error
import urllib.request

from PIL import ImageChops

from mcp_clients import image_state, rpc


def run_packaging_checks(case, smoke):
    from smoke import wait_for

    endpoint_path = case / 'config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json'
    settings_path = case / 'data/io.github.CyberTimon.RapidRAW/settings.json'
    image = str(case / 'input/smoke.ARW')
    port = int(os.environ['RAPIDRAW_MCP_PORT'])

    def status():
        return smoke.execute("return window.__TAURI_INTERNALS__.invoke('mcp_control_status');")

    def port_closed():
        with socket.socket() as connection:
            connection.settimeout(1)
            return connection.connect_ex(('127.0.0.1', port)) != 0

    def off():
        return status() == {'available': True, 'enabled': False, 'port': 0} and not endpoint_path.exists() and port_closed()

    def state(endpoint):
        return image_state(endpoint['url'], image, token=endpoint['token'])

    def history(endpoint):
        value = rpc(endpoint['url'], 'tools/call', {'name': 'history_list', 'arguments': {'imagePath': image}}, token=endpoint['token'])
        return json.loads(next(item['text'] for item in value['content'] if item['type'] == 'text'))

    def toggle(enabled):
        smoke.key('Comma', ',', True)
        wait_for(lambda: smoke.execute("return !!document.querySelector('[data-mcp-control] input');"), 'AI control setting')
        label = smoke.execute("return document.querySelector('[data-mcp-control]').innerText;")
        if 'Let AI assistants control RapidRoom' not in label or 'Control is local only, with a fresh key each time it starts.' not in label:
            raise RuntimeError('AI control setting label/explanation differs')
        smoke.execute("""const input=document.querySelector('[data-mcp-control] input');
          input.closest('label').scrollIntoView({block:'center'});
          if(input.checked!==arguments[0])input.click();return true;""", [enabled])
        wait_for(lambda: status()['enabled'] == enabled, 'AI control toggle completes')
        wait_for(lambda: smoke.execute("return document.querySelector('[data-mcp-control] input').checked===arguments[0] && !document.querySelector('[data-mcp-control] input').disabled;", [enabled]), 'AI control switch reflects runtime')
        wait_for(lambda: json.loads(settings_path.read_text()).get('mcpEnabled') == enabled, 'AI control preference persists')
        settings = json.loads(settings_path.read_text())
        if settings['adjustmentDensity'] != 'compact' or settings['rootFolders'] != [str(case / 'input')]:
            raise RuntimeError('AI control toggle discarded other preferences')
        smoke.capture('control-' + str(len(smoke.result['steps'])) + ('-on' if enabled else '-off'))
        smoke.key('Escape', 'Escape')
        wait_for(lambda: smoke.execute("return !document.querySelector('[data-mcp-control]');"), 'AI settings close')
        if not enabled:
            wait_for(off, 'disabled listener/file removed')

    def consent(confirm):
        wait_for(lambda: smoke.execute("return !!document.querySelector('[aria-labelledby=confirm-modal-title]');"), 'assistant enable prompt')
        text = smoke.execute("return document.querySelector('[aria-labelledby=confirm-modal-title]').innerText;")
        if 'Let AI assistants control RapidRoom' not in text or 'Control is local only, with a fresh key each time it starts.' not in text:
            raise RuntimeError('Consent prompt lacks local-only/fresh-key explanation')
        smoke.capture('consent-' + str(len(smoke.result['steps'])) + ('-confirm' if confirm else '-cancel'))
        smoke.execute("""const dialog=document.querySelector('[aria-labelledby=confirm-modal-title]');
          [...dialog.querySelectorAll('button')].find(e=>e.innerText.trim()===arguments[0]).click();return true;""",
          ['Enable and start' if confirm else 'Cancel'])
        wait_for(lambda: smoke.execute("return !document.querySelector('[aria-labelledby=confirm-modal-title]');"), 'consent closes')

    wait_for(off, 'MCP absent on default startup')
    if json.loads(settings_path.read_text()).get('mcpEnabled', False):
        raise RuntimeError('AI control default preference is enabled')
    smoke.step('default-off: no listener or endpoint file', {'available': True, 'enabled': False, 'port': 0,
               'endpoint_exists': False, 'pinned_port_closed': True, 'setting_omitted_at_startup': True})

    smoke.execute("document.querySelector('[data-start-agent=claude]').click();return true;")
    consent(False)
    wait_for(off, 'cancel leaves control off')
    if list((case / 'terminal-client-tests').glob('*/launch-result.json')) or smoke.execute("return !!document.querySelector('[data-terminal-panel]');"):
        raise RuntimeError('Cancelled prompt launched a client or opened a terminal')
    smoke.step('cancelled launch leaves control off and starts no client', {'endpoint_exists': False})

    previous_endpoint = None
    before_restart = None
    for mode, clients in (('built-in', (('claude', 0.5), ('codex', 1.0))),
                          ('external', (('claude', 1.5), ('codex', 2.0)))):
        if mode == 'external':
            smoke.execute("document.querySelector('[aria-label=\"Collapse bottom panel\"]').click();return true;")
            wait_for(lambda: smoke.execute("return !document.querySelector('[data-terminal-panel]');"), 'built-in panel closes')
            before_restart = (state(previous_endpoint), history(previous_endpoint))
            # Also leave an owned partial connection open to catch orphaned handlers.
            connection = socket.create_connection(('127.0.0.1', port), timeout=3)
            connection.sendall(b'POST /mcp HTTP/1.1\r\n')
            toggle(False)
            connection.settimeout(3)
            try:
                closed = connection.recv(1) == b''
            except ConnectionResetError:
                closed = True
            finally:
                connection.close()
            if not closed:
                raise RuntimeError('Disabled server retained an accepted connection')
            smoke.step('disable persists off and closes endpoint, listener and accepted connection', {'endpoint_exists': False, 'pinned_port_closed': True})
            smoke.execute("""const e=document.querySelector('[data-agent-launcher] select');
              e.value='external';e.dispatchEvent(new Event('change',{bubbles:true}));return true;""")
            wait_for(lambda: smoke.execute("return document.querySelector('[data-agent-launcher] select').value==='external';"), 'external launch mode')
        for index, (name, exposure) in enumerate(clients):
            label = mode + '-' + name
            folder = case / 'terminal-client-tests' / label
            folder.mkdir(parents=True, mode=0o700)
            (case / 'tools' / (name + '-request.json')).write_text(json.dumps({'folder': str(folder), 'exposure': exposure}) + '\n')
            current = previous_endpoint if index else None
            before = state(current) if current else None
            before_pixels = smoke.crop(smoke.stable_preview(label + '-before'))
            smoke.execute("document.querySelector('[data-start-agent=\"'+arguments[0]+'\"]').click();return true;", [name])
            if index == 0:
                consent(True)
                wait_for(lambda: endpoint_path.exists() and status()['enabled'], 'enabled endpoint published')
                current = json.loads(endpoint_path.read_text())
                if stat.S_IMODE(endpoint_path.stat().st_mode) != 0o600:
                    raise RuntimeError('Published endpoint is not private')
                if previous_endpoint and current['token'] == previous_endpoint['token']:
                    raise RuntimeError('Re-enabled server reused the previous key')
                if previous_endpoint:
                    request = urllib.request.Request(current['url'], b'{}', headers={'Authorization': 'Bearer ' + previous_endpoint['token'], 'Content-Type': 'application/json'})
                    try:
                        urllib.request.urlopen(request, timeout=3)
                        raise RuntimeError('Previous key survived restart')
                    except urllib.error.HTTPError as error:
                        if error.code != 401:
                            raise RuntimeError('Previous key was not refused with HTTP 401') from error
                    # The client may already have edited; enforce unchanged history
                    # across a separately tested settings-only toggle below instead.
                smoke.step(mode + ' launch offers enable and publishes a private fresh-key endpoint', {'port': port, 'mode_0600': True,
                           'fresh_key_after_restart': previous_endpoint is not None, 'old_key_refused': previous_endpoint is not None})
            else:
                if smoke.execute("return !!document.querySelector('[aria-labelledby=confirm-modal-title]');"):
                    raise RuntimeError('Enabled control prompted again')
            path = folder / 'launch-result.json'
            wait_for(path.exists, 'real registered ' + label, 330)
            result = json.loads(path.read_text())
            if not result.get('passed') or not result.get('native_per_launch_registration_used'):
                raise RuntimeError(label + ' failed; inspect owned launch-result.json')
            if (mode == 'built-in') != result['stdin_is_pty']:
                raise RuntimeError('Client PTY mode differs from chosen launch mode')
            after = state(current)
            after_pixels = smoke.crop(smoke.stable_preview(label + '-after'))
            if after['adjustments']['exposure'] != exposure or (before and before['editRevision'] == after['editRevision']):
                raise RuntimeError('Real registered client did not make the requested edit')
            if smoke.slider('Exposure')['value'] != exposure or ImageChops.difference(before_pixels, after_pixels).getbbox() is None:
                raise RuntimeError('Real client edit did not reach native slider/preview')
            if mode == 'external' and smoke.execute("return !!document.querySelector('[data-terminal-panel]');"):
                raise RuntimeError('External launch opened built-in panel')
            smoke.step('real registered ' + label + ' edits native slider and preview', {**result, 'exposure': exposure, 'mode': mode, 'slider_preview_changed': True})
            previous_endpoint = current

    before_restart = (state(previous_endpoint), history(previous_endpoint))
    toggle(False)
    toggle(True)
    endpoint = json.loads(endpoint_path.read_text())
    if endpoint['token'] == previous_endpoint['token'] or (state(endpoint), history(endpoint)) != before_restart:
        raise RuntimeError('Settings-only restart reused key or changed edit/revision/history')
    smoke.step('settings enable uses fresh key and preserves edit, revision and history', {'fresh_key': True, 'edit_revision_history_unchanged': True})
