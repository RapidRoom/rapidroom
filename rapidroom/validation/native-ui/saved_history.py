"""Saved History through real native controls and on-disk companion files."""
import json
from smoke import save


def run_saved_history(smoke, wait_for):
    history_path = smoke.case / 'input/smoke.ARW.rrhistory'
    sidecar = smoke.case / 'input/smoke.ARW.rrdata'

    def document():
        try:
            value = json.loads(history_path.read_text())
            root = json.loads(sidecar.read_text())['adjustments']
            current = next(step for step in value['steps'] if step['id'] == value['currentStepId'])
            return value if current['adjustments'] == root else None
        except (OSError, ValueError, StopIteration):
            return None

    def click_text(text):
        smoke.execute("""const e=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()===arguments[0]);
          if(!e)throw Error('Missing '+arguments[0]);e.click();return true;""", [text])

    def tab(name):
        smoke.execute("document.querySelector('[data-tab-id=layout-tab-'+arguments[0]+']').click();return true;", [name])

    def name_snapshot(value):
        smoke.execute("""const input=document.querySelector('[role=dialog] input');
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,arguments[0]);
          input.dispatchEvent(new Event('input',{bubbles:true}));return true;""", [value])
        click_text('Save')

    smoke.drag('Exposure', .4)
    smoke.stable_preview('history-exposure')
    wait_for(lambda: document() and document()['steps'][-1]['adjustments']['exposure'] == .4, 'saved exposure History')
    tab('history')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[aria-label=History] [aria-current=step]');"), 'History panel')
    click_text('+ Snapshot')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[role=dialog] input');"), 'snapshot name dialog')
    name_snapshot('Checkpoint')
    wait_for(lambda: document() and len(document()['snapshots']) == 1, 'snapshot saved before rename')
    smoke.execute("document.querySelector('[aria-label=History] summary').click();return true;")
    click_text('Rename snapshot')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[role=dialog] input');"), 'snapshot rename dialog')
    name_snapshot('Before contrast')
    wait_for(lambda: document() and document()['snapshots'][0]['name'] == 'Before contrast', 'snapshot renamed')
    wait_for(lambda: document() and len(document()['snapshots']) == 1, 'snapshot saved')
    original_snapshot = document()['snapshots'][0]
    snapshot_capture = smoke.capture('history-snapshot')
    tab('adjustments')
    smoke.drag('Contrast', 12)
    smoke.stable_preview('history-contrast')
    wait_for(lambda: document() and document()['steps'][-1]['adjustments']['contrast'] == 12, 'contrast step saved')
    tab('history')
    current = document()
    contrast_id = current['currentStepId']
    smoke.execute("""const buttons=[...document.querySelectorAll('[aria-label=History] button[data-history-row]')];
      const e=buttons.find(e=>e.title.includes('Exposure 0 → +0.4'));if(!e)throw Error('Numeric transition absent');e.click();return true;""")
    wait_for(lambda: document() and document()['currentStepId'] != contrast_id, 'jump saved current ID')
    focused = smoke.execute("""const e=document.querySelector('[aria-label=History] [aria-current=step]');e.focus();
      e.dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true,cancelable:true}));
      return document.activeElement===document.querySelector('[aria-label=History] [data-history-row]');""")
    if not focused:
        raise RuntimeError('History Home key did not focus the first visual row')
    branch_capture = smoke.capture('history-earlier-step')
    tab('adjustments')
    smoke.drag('Contrast', -10)
    smoke.stable_preview('history-branched')
    wait_for(lambda: document() and document()['steps'][-1]['adjustments']['contrast'] == -10, 'branched History saved')
    if contrast_id in [step['id'] for step in document()['steps']]:
        raise RuntimeError('Abandoned redo step remained in History')
    if document()['snapshots'][0] != original_snapshot:
        raise RuntimeError('Branch changed immutable snapshot')
    tab('history')
    smoke.execute("""const e=[...document.querySelectorAll('[aria-label=History] button[data-history-row]')]
      .find(e=>e.textContent.includes('Before contrast'));if(!e)throw Error('Snapshot missing');e.click();return true;""")
    wait_for(lambda: document() and document()['steps'][-1]['label'] == 'Snapshot: Before contrast', 'snapshot restore creates step')
    if document()['steps'][-1]['adjustments'] != original_snapshot['adjustments']:
        raise RuntimeError('Snapshot restore changed saved state')
    restored_capture = smoke.capture('history-restored')
    saved = document()
    before_reload_errors = smoke.execute("return window.__RAPIDROOM_SMOKE_ERRORS__;")
    if before_reload_errors:
        raise RuntimeError('Frontend errors before restart: ' + str(before_reload_errors))
    # A full WebView reload discards all React/Zustand session history. The backend
    # and compositor remain live, while the Editor must reload its companion file.
    smoke.execute("location.reload();return true;")
    wait_for(lambda: smoke.execute("return [...document.querySelectorAll('button')].some(e=>e.textContent.trim()==='Continue Session' && !e.disabled);"), 'reloaded frontend Continue Session', 90)
    click_text('Continue Session')
    wait_for(lambda: smoke.execute("return [...document.images].some(e=>e.alt==='smoke.ARW' && e.complete && e.naturalWidth>100);"), 'reloaded library thumbnail', 90)
    smoke.execute("""const e=[...document.images].find(e=>e.alt==='smoke.ARW');
      e.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0}));return true;""")
    wait_for(lambda: smoke.execute("return !!document.querySelector('[data-editor-info-line]');"), 'reopened Editor', 90)
    tab('history')
    wait_for(lambda: smoke.execute("return document.querySelector('[aria-label=History] [aria-current=step]')?.title.includes('Snapshot: Before contrast');"), 'saved current step restored from disk', 90)
    if smoke.execute("return document.querySelectorAll('[aria-label=History] [data-history-row]').length;") != len(saved['steps']) + 1:
        raise RuntimeError('History stack or snapshot lost after WebView restart')
    restart_capture = smoke.capture('history-after-restart')
    smoke.execute("document.querySelector('[aria-label=History] summary').click();return true;")
    click_text('Delete snapshot')
    wait_for(lambda: smoke.execute("return !!document.querySelector('[role=alertdialog]');"), 'snapshot delete confirmation')
    smoke.execute("""const e=[...document.querySelectorAll('[role=alertdialog] button')].find(e=>e.textContent.trim()==='Delete snapshot');
      e.click();return true;""")
    wait_for(lambda: document() and not document()['snapshots'], 'snapshot deletion saved')
    if [step['id'] for step in document()['steps']] != [step['id'] for step in saved['steps']]:
        raise RuntimeError('Deleting a snapshot changed edit history')
    # Restore the untouched develop state for the ordinary full smoke that follows.
    smoke.execute("""const e=[...document.querySelectorAll('[aria-label=History] button[data-history-row]')]
      .find(e=>e.title.endsWith('Initial state'));if(!e)throw Error('Initial state missing');e.click();return true;""")
    tab('adjustments')
    wait_for(lambda: abs(smoke.slider('Exposure')['value']) < .001 and abs(smoke.slider('Contrast')['value']) < .001, 'initial state restored')
    smoke.stable_preview('history-initial')
    evidence = {'steps': [{key: step[key] for key in ('id','label','actor','timestamp')} for step in saved['steps']],
                'snapshot_id': original_snapshot['id'], 'abandoned_step_id': contrast_id,
                'captures': [str(path.relative_to(smoke.case)) for path in (snapshot_capture, branch_capture, restored_capture, restart_capture)],
                'root_matches_current': True, 'immutable_snapshot': True, 'webview_restart': True, 'snapshot_rename_delete': True}
    save(smoke.case / 'saved-history.json', evidence)
    smoke.step('saved History panel, branch, keyboard and immutable snapshot', evidence)
