"""Exercise Lightroom XMP photo import and Undo through the native editor."""
import hashlib
import json
from PIL import ImageChops
from mcp_clients import image_state


def run_lightroom_import_checks(smoke, wait_for, baseline):
    case = smoke.case
    endpoint = json.loads(
        (case / "config/io.github.CyberTimon.RapidRAW/mcp-endpoint.json").read_text()
    )
    url, token = endpoint["url"], endpoint["token"]
    raw = case / "input/smoke.ARW"
    image = str(raw)
    sidecar = case / "input/smoke.xmp"
    initial = image_state(url, image, token=token)
    (case / "lightroom-initial-adjustments.json").write_text(
        json.dumps(initial["adjustments"], indent=2) + "\n"
    )
    raw_hash = hashlib.sha256(raw.read_bytes()).hexdigest()

    def packet(attributes, include_curve=False):
        curve = (
            "<a:ToneCurvePV2012><r:Seq><r:li>0, 0</r:li>"
            "<r:li>128, 144</r:li><r:li>255, 255</r:li></r:Seq></a:ToneCurvePV2012>"
            if include_curve
            else ""
        )
        return (
            '<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" '
            'xmlns:a="http://ns.adobe.com/camera-raw-settings/1.0/" '
            'xmlns:xmp="http://ns.adobe.com/xap/1.0/">'
            '<r:Description r:about="photo" '
            + attributes
            + ">"
            + curve
            + "</r:Description></r:RDF>"
        )

    def import_menu():
        x, y, width, height = smoke.roi
        smoke.execute(
            """const x=arguments[0],y=arguments[1],e=document.elementFromPoint(x,y);
              e.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,
                button:2,clientX:x,clientY:y}));return true;""",
            [x + width / 2, y + height / 2],
        )
        wait_for(
            lambda: smoke.execute(
                "return [...document.querySelectorAll('[role=menuitem]')].some(e=>"
                "e.textContent.trim()==='Import XMP Adjustments');"
            ),
            "public photo XMP import menu",
        )
        smoke.execute(
            """[...document.querySelectorAll('[role=menuitem]')]
              .find(e=>e.textContent.trim()==='Import XMP Adjustments').click();return true;"""
        )

    def apply_and_check(name, xml, validate, expect_unsupported=False):
        assert not sidecar.exists(), "Previous XMP sidecar was not removed"
        sidecar.write_text(xml)
        import_menu()
        wait_for(lambda: validate(image_state(url, image, token=token)["adjustments"]), name + " imported state")
        adjustments = image_state(url, image, token=token)["adjustments"]
        assert sidecar.read_text() == xml, name + " import rewrote its source packet"
        if expect_unsupported:
            assert smoke.execute(
                "return [...document.querySelectorAll('[role=alert]')].some(e=>"
                "e.innerText.includes('Lightroom develop settings with no RapidRoom control'));"
            ), "CurveRefineSaturation was not reported as unsupported"
        smoke.execute("document.querySelectorAll('.Toastify__close-button').forEach(e=>e.click());return true;")
        imported_preview = smoke.stable_preview(name + "-imported")
        assert ImageChops.difference(smoke.crop(baseline), smoke.crop(imported_preview)).getbbox() is not None, (
            name + " did not change the rendered preview"
        )
        smoke.execute(
            """const e=document.querySelector('button[data-tooltip^="Undo ("]');
              if(!e||e.disabled)throw Error('Undo unavailable after XMP import');e.click();return true;"""
        )
        sidecar.unlink()
        def restored_state():
            current = image_state(url, image, token=token)["adjustments"]
            if current == initial["adjustments"]:
                return current
            return False

        try:
            wait_for(restored_state, name + " Undo restores original adjustment state")
        except RuntimeError:
            actual = image_state(url, image, token=token)["adjustments"]
            (case / "lightroom-undo-diff.json").write_text(
                json.dumps(
                    {
                        key: {"before": initial["adjustments"].get(key), "after": actual.get(key)}
                        for key in initial["adjustments"]
                        if initial["adjustments"].get(key) != actual.get(key)
                    },
                    indent=2,
                )
                + "\n"
            )
            raise
        undone = smoke.stable_preview(name + "-undo")
        assert ImageChops.difference(smoke.crop(baseline), smoke.crop(undone)).getbbox() is None
        smoke.step(name + " public photo import", adjustments)

    raw_defaults = packet(
        'a:HasSettings="True" a:ProcessVersion="15.4" a:Exposure2012="0"'
    )
    apply_and_check(
        "HasSettings photo import applies omitted RAW defaults and preserves explicit zero",
        raw_defaults,
        lambda a: a.get("colorNoiseReduction") == 25
        and abs(a.get("sharpness", 0) - 40 / 150 * 100) < 1e-6
        and a.get("exposure") == 0,
    )

    parametric = packet(
        'a:HasSettings="True" a:ProcessVersion="15.4" '
        'a:ParametricShadows="-50" a:ParametricHighlights="20" '
        'a:ParametricShadowSplit="20" a:CurveRefineSaturation="50"',
        include_curve=True,
    )

    def valid_parametric(a):
        state = a.get("parametricCurve", {}).get("luma", {})
        return (
            a.get("curveMode") == "parametric"
            and state.get("shadows") == -50
            and state.get("highlights") == 20
            and state.get("split1") == 20
            and state.get("split2") == 50
            and state.get("split3") == 75
            and a.get("pointCurves", {}).get("luma")
            == [{"x": 0, "y": 0}, {"x": 128, "y": 144}, {"x": 255, "y": 255}]
            and a.get("curves", {}).get("luma") != a.get("pointCurves", {}).get("luma")
            and a.get("curves", {}).get("red") == a.get("pointCurves", {}).get("red")
        )

    apply_and_check(
        "Parametric photo import composes curves and preserves channel state",
        parametric,
        valid_parametric,
        expect_unsupported=True,
    )
    assert hashlib.sha256(raw.read_bytes()).hexdigest() == raw_hash
    (case / "lightroom-import-result.json").write_text(
        json.dumps(
            {
                "raw_unchanged": True,
                "photo_defaults_and_explicit_zero": True,
                "parametric_before_point_curve": True,
                "point_and_channel_curves_preserved": True,
                "gui_undo_restored_preview": True,
                "real_adobe_application": False,
            },
            indent=2,
        )
        + "\n"
    )
