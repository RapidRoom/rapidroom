// Hand-written notes for the generated adjustment reference (see adjustmentReference.ts): what a
// positive value does and what an agent must know. Each was checked against the shader and
// image_processing.rs; keep them short, they end up in table cells. The test fails if a key has
// no note or a note's key no longer exists.
export const ADJUSTMENT_NOTES: Record<string, string> = {
  baselineExposure:
    'Eligibility marker for baseline/highlight-preservation compensation. Fresh/reset images use true; older sidecars without it use false. With cameraExposureCompensation=false, the original Settings baseline path is retained. Does not change the Exposure slider.',
  cameraExposureCompensation:
    'Fresh/reset RAW baseline policy: DNG BaselineExposure plus Offset takes precedence, including explicit zero; otherwise supported Sony A7C II/A7R V use +0.35 EV. Does not add the generic Settings scene lift. Missing/false on saved recipes keeps legacy behavior; baselineExposure still controls eligibility.',
  exposure:
    '+ brightens linear light before tone mapping: ×2^(value/0.8), so 1.0 is about +1.25 EV. The first slider for overall brightness.',
  toneMapper:
    '`basic` (default; hard-clips above white) or `agx` (filmic, rolls highlights off). A Settings override can force AgX for raws.',
  brightness:
    '+ lifts the midtones after tone mapping; black and white stay put. Use after exposure, for the feel rather than the level.',
  contrast: '+ S-curve around middle grey. Keep moderate; contrast between regions comes from masks.',
  highlights:
    '− recovers bright-area detail; + brightens it. Uses a log-space edge-aware base and mid-scale detail reinjection.',
  shadows: '+ lifts and opens the shadows with some local detail; − deepens them.',
  whites:
    'Selectively lifts or compresses bright tones above an edge-aware base, with local and mid-scale detail reinjection. It no longer applies a uniform gain to the whole image.',
  whiteBalance:
    'Optional absolute Kelvin baseline {temperature: 2000–50000, tint: −150–150}; null uses the camera as-shot baseline. The relative temperature/tint offsets apply on top. Set both offsets to0 when setting an absolute target, as the GUI does.',
  blacks: '+ lifts the deepest tones (faded look); − deepens them.',
  'curves.<channel>':
    'Points `{x, y}` from 0 to 255, sorted by x, at most 16, after tone mapping. `luma` is applied to R, G and B alike. The renderer reads only `curves`.',
  'pointCurves.<channel>': 'Saved copy of the point curves for point mode. Write the same points as `curves`.',
  'parametricCurve.<channel>.darks':
    'Zone between split1 and split2. The renderer ignores `parametricCurve`: the GUI turns it into `curves`. Over MCP, write `curves` instead.',
  'parametricCurve.<channel>.shadows': 'Zone from black to split1. GUI only, see darks.',
  'parametricCurve.<channel>.highlights': 'Zone from split3 to white. GUI only, see darks.',
  'parametricCurve.<channel>.lights': 'Zone between split2 and split3. GUI only, see darks.',
  'parametricCurve.<channel>.whiteLevel': 'Lowers the white end point (0–255 units). GUI only, see darks.',
  'parametricCurve.<channel>.blackLevel': 'Raises the black end point (0–255 units). GUI only, see darks.',
  'parametricCurve.<channel>.split1': 'Zone boundary in % of the input; at least 10. GUI only, see darks.',
  'parametricCurve.<channel>.split2': 'Zone boundary in % of the input. GUI only, see darks.',
  'parametricCurve.<channel>.split3': 'Zone boundary in % of the input; at most 90. GUI only, see darks.',
  curveMode: '`point` or `parametric`: which curve editor the GUI shows. Leave at `point` and write `curves`.',
  temperature: '+ warmer (yellow/orange), − cooler (blue). Strong: ±30 is already a lot.',
  tint: '+ magenta, − green.',
  saturation: '+ more colour everywhere; −100 is black and white.',
  vibrance: '+ boosts muted colours most and protects skin; − mostly calms colours that are already strong.',
  hue: 'Rotates every hue by this many degrees: + goes red → yellow → green → blue → magenta.',
  'colorGrading.balance':
    '+ lowers the highlight zone, so more of the image gets the highlight tint; − raises the shadow zone.',
  'colorGrading.blending': 'Higher blends the shadow, midtone and highlight zones more softly.',
  'colorGrading.global.hue': 'Hue of the tint in degrees, 0–360 (0 red, 120 green, 240 blue). Color wheel in the GUI.',
  'colorGrading.global.saturation':
    'Strength of the tint over the whole image, 0–100. Only positive values do anything.',
  'colorGrading.global.luminance': '+ brightens, − darkens the whole image (−100 to 100).',
  'colorGrading.highlights.hue': 'Hue of the highlight tint, 0–360 degrees.',
  'colorGrading.highlights.saturation': 'Strength of the highlight tint, 0–100. Warm highlights: hue 30–50, 10–30.',
  'colorGrading.highlights.luminance': '+ brightens, − darkens the highlights (−100 to 100).',
  'colorGrading.midtones.hue': 'Hue of the midtone tint, 0–360 degrees.',
  'colorGrading.midtones.saturation': 'Strength of the midtone tint, 0–100.',
  'colorGrading.midtones.luminance': '+ brightens, − darkens the midtones (−100 to 100).',
  'colorGrading.shadows.hue': 'Hue of the shadow tint, 0–360 degrees. Cool shadows: about 200–230.',
  'colorGrading.shadows.saturation':
    'Strength of the shadow tint, 0–100. Weaker than the other zones at the same value.',
  'colorGrading.shadows.luminance': '+ lifts, − deepens the shadows (−100 to 100).',
  'hsl.<band>.hue':
    '+ shifts the band towards the next colour (reds → orange, yellows → green, blues → purple, magentas → red), up to ±60°.',
  'hsl.<band>.saturation': '+ more colour in that band only; −100 removes it. Neutral greys are not affected.',
  'hsl.<band>.luminance': '+ brightens, − darkens that colour (for example blues −20 for a deeper sky).',
  'colorCalibration.shadowsTint': '+ magenta, − green, in the shadows only.',
  'colorCalibration.<primary>Hue':
    'Shifts the camera primary: red + towards orange, green + towards cyan, blue + towards magenta. Hidden in the GUI by default.',
  'colorCalibration.<primary>Saturation':
    '+ saturates colours dominated by that primary. Hidden in the GUI by default.',
  clarity: '+ local contrast in fine detail (crisper); − softens. Keep off skin.',
  structure: 'Like clarity at a larger scale (shapes, not texture).',
  dehaze: '+ removes haze and adds saturation; − adds a bluish haze. Mild per unit.',
  centré:
    'Centre emphasis: + adds contrast, about 0.2 EV and colour in a central ellipse and quietens the edges; − the reverse. Not in masks.',
  centréX: 'Normalized horizontal centre position for Centré (0–1); default 0.5.',
  centréY: 'Normalized vertical centre position for Centré (0–1); default 0.5.',
  sharpness: '+ sharpens with halo limiting; − softens. Sharpen the subject, not the background.',
  sharpnessThreshold: 'Higher sharpens only stronger edges (keeps skies and noise clean).',
  lumaNoiseReduction:
    '+ smooths luminance noise. In a mask, the value is added to the global one, so a negative mask value only reduces global NR there.',
  colorNoiseReduction: '+ removes colour blotches. Masks work as for luma noise reduction.',
  aiDenoiseEnabled:
    'Turns on upstream non-destructive AI denoise: the model runs once per photo (cached on disk) and is blended into the base image. Global only. Separate from the Library denoise queue.',
  aiDenoiseMethod:
    '`ai` runs the NIND model; `bm3d` runs the model-free BM3D denoiser (upstream, the default in AI-free mode). Needs `aiDenoiseEnabled`.',
  aiDenoiseAmount: 'Blend of the AI-denoised base, 0–100; 0 leaves the image unchanged. Needs `aiDenoiseEnabled`.',
  aiDenoiseDetail:
    '+ keeps more of the original luminance texture while colour noise stays removed (0–100). Needs `aiDenoiseEnabled`.',
  negativeConversionEnabled:
    'Treats the photo as a film negative and inverts it non-destructively before all other adjustments.',
  negativeExposure: 'Brightness of the converted positive. Only used with `negativeConversionEnabled`.',
  negativeContrast: 'Contrast of the converted positive; 1 is neutral. Only used with `negativeConversionEnabled`.',
  negativeRedWeight: 'Red channel balance for removing the film base cast; 1 is neutral. Negative conversion only.',
  negativeGreenWeight: 'Green channel balance for removing the film base cast; 1 is neutral. Negative conversion only.',
  negativeBlueWeight: 'Blue channel balance for removing the film base cast; 1 is neutral. Negative conversion only.',
  chromaticAberrationRedCyan:
    'Scales the red channel against green by up to 1 %. Try both signs and check high-contrast edges; which sign fixes a lens varies.',
  chromaticAberrationBlueYellow: 'Scales the blue channel against green by up to 1 %. Try both signs, as above.',
  vignetteAmount: '− darkens the corners (the usual edge burn); + lightens them towards white.',
  vignetteFeather: 'Width of the transition: higher is softer.',
  vignetteMidpoint: 'Where the transition sits: higher pushes it towards the corners.',
  vignetteRoundness: 'Changes the shape between rectangular and round.',
  vignetteCenterX: 'Normalized horizontal vignette centre (0–1); default 0.5.',
  vignetteCenterY: 'Normalized vertical vignette centre (0–1); default 0.5.',
  grainAmount: '+ more film grain, mostly in the midtones. Applied last, after the LUT.',
  grainRoughness: 'Higher mixes in a coarser, less even grain.',
  grainSize: 'Higher is coarser grain.',
  glowAmount: '+ warm bloom from bright areas.',
  halationAmount: '+ red-orange glow around highlights, like film halation.',
  flareAmount: '+ synthetic lens flare and starbursts from bright highlights.',
  lutIntensity: 'How much of the LUT is mixed in. Only active while `lutPath` is set.',
  lutName: 'Display name of the LUT. Set by the GUI.',
  lutPath:
    'Absolute path of a `.cube` file; `null` for none. The six built-in film looks are in the app resources (`film_luts`).',
  lutSize: 'Grid size of the loaded LUT. Set by the GUI.',
  lutData: 'Loaded LUT data. Set by the GUI; leave alone.',
  lutIsSceneReferred:
    '`true` applies the LUT before tone mapping (built-in film looks use this), `false` after the curves.',
  crop: '`{"unit": "px", "x", "y", "width", "height"}` in full-resolution pixels after rotation and flips; `null` for none.',
  aspectRatio:
    '`0` follows the oriented image (Original); `null` is Free; positive numbers are fixed crop width / height (for example 0.8 for 4:5).',
  rotation: 'Fine straightening in degrees; + turns the image clockwise.',
  flipHorizontal: 'Mirror left–right.',
  flipVertical: 'Mirror top–bottom.',
  orientationSteps:
    '0–3 quarter turns clockwise. The GUI also swaps the aspect ratio and recentres the crop; prefer asking the user.',
  transformDistortion: '+ adds barrel (fixes pincushion), − adds pincushion.',
  transformVertical: '+ widens the top and narrows the bottom: fixes buildings that lean back.',
  transformHorizontal: '+ enlarges the right side and shrinks the left.',
  transformRotate: 'Rotation inside the transform, in degrees; + clockwise.',
  transformAspect: '+ stretches horizontally, − narrows.',
  transformScale: 'Zoom in percent (100 = none). Use to hide empty corners after a transform.',
  transformXOffset: '+ moves the content right, in % of the width.',
  transformYOffset: '+ moves the content down, in % of the height.',
  lensCorrectionMode: 'GUI only: `auto` makes the GUI detect the lens and fill the lens keys. The renderer ignores it.',
  lensMaker: 'Lens maker from the lens database, filled by the GUI.',
  lensModel: 'Lens model from the lens database, filled by the GUI.',
  lensDistortionAmount: 'Strength of the profile distortion correction (100 = profile). Needs a detected lens.',
  lensVignetteAmount: 'Strength of the profile vignette correction (100 = profile). Needs a detected lens.',
  lensTcaAmount: 'Strength of the profile chromatic aberration correction (100 = profile). Needs a detected lens.',
  lensDistortionEnabled: 'Turns the profile distortion correction on or off.',
  lensTcaEnabled: 'Turns the profile chromatic aberration correction on or off.',
  lensVignetteEnabled: 'Turns the profile vignette correction on or off.',
  'guidedPerspective.enabled': 'Turns guided perspective on. Compute lines with `calculate_guided_perspective` first.',
  'guidedPerspective.lines':
    'Up to two vertical and two horizontal guides: `{id, type: "vertical" | "horizontal", p1: {x, y}, p2: {x, y}}`, 0–1.',
  'guidedPerspective.autoCrop': 'Crop away the empty corners after the correction.',
  masks: 'Mask containers; see [Masks](#masks). Replacing the array replaces every mask.',
  aiPatches: 'Generative edits made in the GUI. Leave alone.',
  lensBlurAmount: 'Strength of the depth-of-field blur. Needs `lensBlurEnabled` and a depth map.',
  lensBlurDiffusion: 'Softens the blur discs.',
  lensBlurShape: 'Bokeh shape: `circle`, `hexagon`, `octagon` or `ring`.',
  lensBlurDepthMap: 'Depth map the GUI generates when lens blur is turned on. Without it the blur does nothing.',
  relightEnabled:
    'Enables relight using a source-space normal map; the GUI generates it on explicit activation. Off by default.',
  relightNormalMap: 'Generated source-space normal map. Set by the GUI; leave its encoded data alone.',
  relightLights:
    'Array of point, spot or directional lights. Replacing the array replaces all lights; preserve existing entries and stable IDs.',
  relightAmbient: 'Adds ambient illumination to the relit surface.',
  relightSoftness: 'Softens local relight contrast.',
  relightShine: 'Increases the specular highlight contribution.',
  relightShadows: 'Enables shadows cast by relight lights.',
  relightShadowSoftness: 'Softens relight shadow edges.',
  fogEnabled: 'Enables atmospheric fog using a generated depth map. Off by default.',
  fogDepthMap: 'Generated source-space depth map. Set by the GUI; leave its encoded data alone.',
  fogAmount: 'Increases the fog blend strength; 0 leaves the image unchanged.',
  fogStart: 'Moves the start of fog farther into the depth range.',
  fogDensity: 'Increases optical fog density.',
  fogHeight: 'Controls the height-dependent fog distribution.',
  fogVariation: 'Increases spatial variation in the fog.',
  fogGlow: 'Increases light scattered into the fog.',
  fogTemperature: '+ warms the fog colour; − cools it.',
  fogTint: '+ magenta, − green, in the fog colour.',
  'relightLights.<light>.id':
    'Stable light ID. Retain it when editing an existing light; use a new UUID for a new light.',
  'relightLights.<light>.type': '`point`, `spot` or `directional`; controls which geometry and aiming fields apply.',
  'relightLights.<light>.x': 'Horizontal placement in normalized source-image coordinates, 0–1.',
  'relightLights.<light>.y': 'Vertical placement in normalized source-image coordinates, 0–1.',
  'relightLights.<light>.depth': 'Position of a point or spot light along the depth axis.',
  'relightLights.<light>.intensity': 'Light strength; 0 contributes no illumination.',
  'relightLights.<light>.radius': 'Falloff extent for point and spot lights.',
  'relightLights.<light>.angle': 'Aim direction in degrees in the image plane.',
  'relightLights.<light>.elevation': 'Aim elevation in degrees; applies to spot and directional lights.',
  'relightLights.<light>.cone': 'Spotlight cone width; ignored for other light types.',
  'relightLights.<light>.feather': 'Softens the spotlight cone edge.',
  'relightLights.<light>.temperature': '+ warms the light; − cools it.',
  'relightLights.<light>.tint': '+ magenta, − green, in the light colour.',
  'relightLights.<light>.color': 'Light colour as a six-digit `#RRGGBB` string, before temperature/tint shifts.',
  lensBlurEnabled: 'Turns lens blur on. Turn it on in the GUI, which also makes the depth map.',
  lensBlurMaxDepth: 'Depth (0–100) up to which the image stays sharp; nearer gets blurred.',
  lensBlurMaxFade: 'Transition width at the near end of the sharp range.',
  lensBlurMinDepth: 'Depth (0–100) from which the image stays sharp; farther gets blurred.',
  lensBlurMinFade: 'Transition width at the far end of the sharp range.',
  lensDistortionParams: 'Lens profile coefficients, filled by the GUI from the lens database. Leave alone.',
  activeTools:
    'Per-tool boolean map; omitted keys are enabled. Set colorMixer or colorGrading to false to bypass that tool without losing its values. Applies to global and mask edits; legacy sectionVisibility sidecars are migrated on load.',
  showClipping: 'Shows the clipping overlay in the editor. Display only; doesn’t change the photo.',
};

export const MASK_TYPE_NOTES: Record<string, string> = {
  'ai-depth':
    'Band of the AI depth map: `minDepth`, `maxDepth`, `minFade`, `maxFade` in %. The depth map is generated by the GUI.',
  'ai-foreground': 'AI foreground selection; `maskDataBase64` is generated by the GUI. Coming over MCP (#120).',
  'ai-sky': 'AI sky selection; generated by the GUI. Coming over MCP (#120).',
  'ai-subject': 'AI subject selection; generated by the GUI. Coming over MCP (#120).',
  'ai-text': 'AI selection from a text `prompt` (upstream); `maskDataBase64` is generated by the GUI.',
  'ai-face': 'AI face and body-part selection; `parts` names the parts (upstream). Generated by the GUI.',
  all: 'The whole image. Useful with `subtractive` sub-masks.',
  brush:
    '`lines`: `{tool: "brush" | "eraser", brushSize, feather 0–1, points: [{x, y}]}`. Painting is easier in the GUI.',
  flow: 'Like `brush`, with `flow` (build-up per stroke).',
  color: 'Selects colours like the pixel at `targetX`, `targetY`; picked in the GUI.',
  linear:
    'Gradient: the start → end line is the 50 % contour, `range` its half-width in pixels. Full effect left of the direction: above a line drawn left to right.',
  luminance: 'Selects tones like the pixel at `targetX`, `targetY`; picked in the GUI.',
  'quick-eraser': 'AI removal area for generative edits. GUI only.',
  radial:
    'Ellipse around `centerX`, `centerY` with `radiusX`, `radiusY` (pixels) and `rotation` (degrees); full effect inside. `feather` 0–1.',
  clone: 'AI patch area (Clone). GUI only.',
  heal: 'AI patch area (Heal). GUI only.',
  liquify: 'AI patch area (Liquify). GUI only.',
  retouch: 'AI patch area (Retouch). GUI only.',
};
