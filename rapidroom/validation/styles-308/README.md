# Built-in Styles validation

Twenty original Styles start as a flat list. All twenty passed owner taste review on scene-appropriate photographs. Public comparison panels are reduced to 2400 pixels wide for repository size; source-image and native-crop review used the full exports. Public examples below use only CC0 sources; owner photographs and reference material remain private.

These are intentional applied-style changes at Amount 100. Before and after share exposure and white balance. A shared exposure correction is outside the preset. The night source is an already edited JPEG; these examples show a further grade, not a RAW-versus-camera comparison. Daylight and woodland examples are RAW files.

The compiled releases identify `fad5a6288d48aa513bec251924d0ad1f0a826041`. Native harness candidate `88171ee5bf5f4f19a5b2690ce94549c8a74b7d09` changes only `native-ui/presets.py`; all production objects are identical. Any later evidence-only commit does not relabel either binary.

Validation: 451 frontend tests; frontend build and scoped formatting; exact baseline lint/TypeScript diagnostics retained. Rust formatting, strict all-feature Clippy and 507 library tests pass (18 ignored). Both release pins were captured from successful Cargo compiler-artifact events and copied/hashed during the same build-lock hold; 105 packaged resource files verified. A fresh native run passes all 23 checks, including flat seeding, Amount/Undo, reusable editable masks, deletion/reload, stock retirement and preservation of user edits/folders/favourites. All 60 default-render comparisons are pixel-identical to the committed reference. Fresh approved-image parity covers all twenty recipes. Earlier harness failures remain recorded separately.

All fourteen public comparisons were personally reviewed at phone size, full view and native crops. The local comparison viewer was checked at 2560×1440 and 3840×2160, with 1024-pixel crops at device scales 1 and 2, keyboard before/after navigation and a 420-pixel phone view. Native UI interactions were automated in isolated Linux/Weston; native screenshots were personally inspected. This is not a claim of manual human operation of the app, nor testing on Windows/macOS.

[Validation receipt](validation.json) · [Source attribution and source hashes](sources.json) · [Public render receipts](public-render-receipts.json) · [Image hashes](image-manifest.json)

| Style            | Source type | Shared base EV | Before / after                            |
| ---------------- | ----------- | -------------: | ----------------------------------------- |
| Teal & Orange    | RAW         |          +0.65 | [comparison](panels/teal-orange.jpg)      |
| Vivid Rainbow    | RAW         |          +0.55 | [comparison](panels/vivid-atlas.jpg)      |
| Bright Beige     | RAW         |          +0.55 | [comparison](panels/ivory-light.jpg)      |
| Aqua             | RAW         |          +0.45 | [comparison](panels/aqua-vista.jpg)       |
| Flower Garden    | RAW         |           +0.4 | [comparison](panels/petal-leaf.jpg)       |
| Black & Gold     | JPEG        |             +0 | [comparison](panels/black-gold.jpg)       |
| Neon Night       | JPEG        |             +0 | [comparison](panels/neon-night.jpg)       |
| Blue & Orange    | JPEG        |             +0 | [comparison](panels/night-city.jpg)       |
| Cyberpunk        | JPEG        |             +0 | [comparison](panels/neon-outrun.jpg)      |
| Deep Green       | RAW         |          +0.65 | [comparison](panels/emerald-grove.jpg)    |
| Dark Forest      | RAW         |          +0.65 | [comparison](panels/forest-gothic.jpg)    |
| My Look          | RAW         |          +0.65 | [comparison](panels/my-look.jpg)          |
| Cyberpunk Teal   | JPEG        |             +0 | [comparison](panels/cyberpunk-teal.jpg)   |
| Cyberpunk Indigo | JPEG        |             +0 | [comparison](panels/cyberpunk-indigo.jpg) |

Native captures: [description search](native/description-search.png), [editable style](native/editable-style.png), [JPEG export](native/native-export.png).
