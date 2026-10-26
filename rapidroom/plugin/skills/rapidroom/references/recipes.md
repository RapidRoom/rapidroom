# Recipes

## Export for Instagram

Agree the destination and output shape with the user before grading. These are useful export targets:

| Shape            | Crop ratio (`aspectRatio`) | Export size | Notes                                     |
| ---------------- | -------------------------- | ----------- | ----------------------------------------- |
| Portrait 4:5     | 0.8                        | 1080 × 1350 | Takes the most room in the feed           |
| Feed 3:4         | 0.75                       | 1080 × 1440 | When the user requests this feed shape    |
| Story 9:16       | 0.5625                     | 1080 × 1920 | Full-height vertical story                |
| Square 1:1       | 1                          | 1080 × 1080 |                                           |
| Landscape 1.91:1 | 1.91                       | 1080 × 566  | Small in the feed; only for wide pictures |

1. **Crop first.** Use `get_editor_context` for image dimensions and the documented original-pixel crop schema. Agree the frame before applying it through `update_adjustments` with the latest revision. The user can place the frame in the Crop panel if preferred. Keep an existing crop that already matches their request. Preview to check the actual framing and rendered aspect; export resizing does not create the crop.
2. **Check at phone size**: `get_preview` with `maxDimension` about 800, and look at it small. Fine texture and thin lines get lost; a little more `clarity` or `sharpness` can help. Output sharpening for export is [coming](tools.md#coming) (#121).
3. **Export** a JPEG into a separate folder, so the full-size edit stays untouched:

   ```json
   {
     "imagePaths": ["<the open image>"],
     "outputFormat": "jpg",
     "exportSettings": {
       "destinationType": "originalFolder",
       "subfolder": "instagram",
       "jpegQuality": 90,
       "resize": { "mode": "width", "value": 1080, "dontEnlarge": true },
       "keepMetadata": true,
       "stripGps": true,
       "filenameTemplate": "{original_filename}_ig"
     }
   }
   ```

   Use `outputDirectory` with `destinationType: "customFolder"` when the user chooses another folder. A 3:4 crop resized to width 1080 yields 1080×1440; check the export result’s actual dimensions. `dontEnlarge` caps smaller sources at native size.

   `stripGps` keeps the location private while keeping camera and lens data. Ask before keeping GPS.

4. Tell the user where the file is. Posting stays manual: never upload anything.

Export presets you can name and reuse ("Instagram 4:5") are [coming](tools.md#coming) (#121).

## Explore looks

The user wants to see a few directions before choosing one: "show me three looks".

**With virtual copies** ([coming](tools.md#coming), #120): one virtual copy per look, each edited and named by its idea, compared side by side in RapidRoom; the user keeps the one they like and refines it. Not possible yet. Don't say otherwise.

**Until then, with proposed edits**:

1. Make the base edit first (geometry, white balance, exposure) and save its full state from `get_active_image_state(nonDefaultOnly: false)`.
2. Pick two or three different directions that suit this photo, from [looks.md](looks.md) or the user's words. Different in key, colour or mood, not five shades of the same thing.
3. Call `render_compare` once with named variants and temporary `changes` for those directions, at about 800 px. Keep a common approved crop if the current build requires matching geometry. Use separate proposed `get_preview` calls only if comparison is unavailable.
4. Show the comparison with a one-line description each: "**Gold and black**: deep shadows, warm light on the face; the blue jacket is muted." Inspect shadow and midtone subject detail before accepting a strong curve.
5. When the user chooses an identifier/label, apply that patch with `update_adjustments` and the latest `expectedRevision`, then refine it together. Finish with one small native noise/detail crop. Do not apply rejected variants merely to render them.
6. If the user wants to keep more than one look, suggest they create virtual copies in the library by hand for now, then open each copy and apply its look.

Learn from the choice: what they picked and what they said about the others is evidence for the next photo ([taste.md](taste.md)).

## Grade a series consistently

1. Edit one representative photo with the user until they accept it.
2. Take the keys that generalise (white balance offsets aside: tone, colour, grading, effects; never crop, masks or geometry) from its state.
3. Open the next photo, apply them with `update_adjustments`, then correct what this photo needs (exposure and white balance differ per frame). Check each one; don't batch blindly.
4. Presets and syncing across a shoot over MCP are [coming](tools.md#coming) (#120). The user can also copy and paste settings in the GUI.
