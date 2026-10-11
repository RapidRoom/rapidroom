"""Point Color MCP validation against the real app; no model requests."""
import copy
import json
from PIL import ImageChops
from mcp_clients import image_state, rpc, run_client_checks


def run_point_color_mcp_checks(smoke, wait_for, baseline):
    endpoint = smoke.case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json"
    wait_for(endpoint.exists, 'local Point Color MCP endpoint')
    wire = run_client_checks(smoke.case, smoke, wire_only=True)
    wire['transport'] = 'authenticated local HTTP JSON-RPC'
    (smoke.case / 'client-tests/result.json').write_text(json.dumps(wire, indent=2) + '\n')
    smoke.step('Point Color authenticated MCP discovery', wire)
    connection = json.loads(endpoint.read_text())
    url, token = connection['url'], connection['token']
    path = str(smoke.case / 'input/smoke.ARW')
    tools = rpc(url, 'tools/list', {}, token=token)['tools']
    update = next(tool for tool in tools if tool['name'] == 'update_adjustments')
    point_schema = update['inputSchema']['properties']['changes']['properties']['pointColor']
    if point_schema.get('maxItems') != 8 or point_schema['items']['properties']['hueShift'].get('maximum') != 100:
        raise RuntimeError('Real MCP discovery lost Point Color bounds')
    before = image_state(url, path, token=token)
    points = before['adjustments']['pointColor']
    if len(points) != 1:
        raise RuntimeError('MCP did not retain the UI-picked swatch')

    def update_points(points, revision):
        return rpc(url, 'tools/call', {'name': 'update_adjustments', 'arguments': {
            'imagePath': path, 'expectedRevision': revision, 'changes': {'pointColor': points}}}, token=token)

    invalid = copy.deepcopy(points)
    invalid[0]['hueShift'] = 101
    rejected = update_points(invalid, before['editRevision'])
    unchanged = image_state(url, path, token=token)
    if not rejected.get('isError') or unchanged['adjustments'] != before['adjustments'] or unchanged['editRevision'] != before['editRevision']:
        raise RuntimeError('Invalid MCP Point Color edit changed the real image')
    smoke.step('MCP rejects out-of-bounds Point Color without editing or advancing revision')
    edited_points = copy.deepcopy(points)
    edited_points[0]['hueShift'] = 40
    result = update_points(edited_points, before['editRevision'])
    if result.get('isError'):
        raise RuntimeError('MCP Point Color edit failed: ' + str(result))
    wait_for(lambda: smoke.slider('Hue Shift')['value'] == 40, 'MCP updates real Point Color control')
    after = image_state(url, path, token=token)
    expected = dict(before['adjustments'], pointColor=edited_points)
    if after['adjustments'] != expected or after['editRevision'] == before['editRevision']:
        raise RuntimeError('MCP Point Color edit changed unrelated adjustments or did not advance revision')
    rendered = smoke.stable_preview('point-color-mcp-edit')
    if ImageChops.difference(smoke.crop(baseline), smoke.crop(rendered)).getbbox() is None:
        raise RuntimeError('MCP Point Color edit did not affect rendered pixels')
    smoke.execute("""const e=document.querySelector('button[data-tooltip^="Undo ("]');
      if(!e||e.disabled)throw Error('MCP Undo unavailable');e.click();return true;""")
    wait_for(lambda: smoke.slider('Hue Shift')['value'] == 0, 'one GUI Undo restores MCP Point Color edit')
    restored = image_state(url, path, token=token)
    pixels = smoke.stable_preview('point-color-mcp-undo')
    if restored['adjustments'] != before['adjustments'] or ImageChops.difference(smoke.crop(baseline),smoke.crop(pixels)).getbbox() is not None:
        raise RuntimeError('One Undo failed to restore MCP Point Color state and exact pixels')
    smoke.step('MCP Point Color updates only the selected swatch and real pixels; one GUI Undo restores both')
