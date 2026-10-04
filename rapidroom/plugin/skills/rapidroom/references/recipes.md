# Recipes

## Export for Instagram

Instagram shows photos at 1080 px wide. Pick the shape with the user:

| Shape            | Crop ratio (`aspectRatio`) | Export size | Notes                                     |
| ---------------- | -------------------------- | ----------- | ----------------------------------------- |
| Portrait 4:5     | 0.8                        | 1080 × 1350 | Takes the most room in the feed           |
| Square 1:1       | 1                          | 1080 × 1080 |                                           |
| Landscape 1.91:1 | 1.91                       | 1080 × 566  | Small in the feed; only for wide pictures |

1. **Crop to the shape.** The crop is in full-resolution pixels, and the MCP tools don't report the full image size yet (`get_editor_context`, #115). So ask the user to choose the ratio in the Crop panel and place the frame, and suggest where ("keep the horizon in the lower third, the subject's gaze into the space on the right"). If the photo already has a crop with the right `aspectRatio`, keep it. Preview to check the composition at that shape.
2. **Check at phone size**: `get_preview` with `maxDimension` 1350 (or smaller), and look at it small. Fine texture and thin lines get lost; a little more `clarity` or `sharpness` can help. Output sharpening for export is [coming](tools.md#coming) (#121).
3. **Export** a JPEG into a separate folder, so the full-size edit stays untouched:

   ```json
   {
     "imagePaths": ["<the open image>"],
     "outputDirectory": "<the user's choice, for example ~/Pictures/instagram>",
     "outputFormat": "jpg",
     "exportSettings": {
       "jpegQuality": 90,
       "resize": { "mode": "width", "value": 1080, "dontEnlarge": true },
       "keepMetadata": true,
       "stripGps": true,
       "filenameTemplate": "{original_filename}_ig"
     }
   }
   ```

   `stripGps` keeps the location private while keeping camera and lens data. Ask before keeping GPS.

4. Tell the user where the file is. Posting stays manual: never upload anything.

Export presets you can name and reuse ("Instagram 4:5") are [coming](tools.md#coming) (#121).

## Explore looks

The user wants to see a few directions before choosing one: "show me three looks".

**With virtual copies** ([coming](tools.md#coming), #120): one virtual copy per look, each edited and named by its idea, compared side by side in RapidRoom; the user keeps the one they like and refines it. Not possible yet. Don't say otherwise.

**Until then, with proposed edits**:

1. Make the base edit first (geometry, white balance, exposure) and save its state from `get_active_image_state`.
2. Pick two to four genuinely different directions that suit this photo, from [looks.md](looks.md) or the user's words. Different in key, colour or mood, not five shades of the same thing.
3. For each, build the full adjustments (the base plus the look's changes) and render it with `get_preview`, passing `adjustments`. Nothing changes in the editor; the user keeps seeing the base.
4. Show the previews with a one-line description each: "**Gold and black**: deep shadows, warm light on the face; the blue jacket is muted."
5. When the user chooses, apply that one with `update_adjustments` and refine it together. If they want to see a look in the editor before deciding, apply it, and restore the base with `set_adjustments` if they don't keep it.
6. If the user wants to keep more than one look, suggest they create virtual copies in the library by hand for now, then open each copy and apply its look.

Learn from the choice: what they picked and what they said about the others is evidence for the next photo ([taste.md](taste.md)).

## Grade a series consistently

1. Edit one representative photo with the user until they accept it.
2. Take the keys that generalise (white balance offsets aside: tone, colour, grading, effects; never crop, masks or geometry) from its state.
3. Open the next photo, apply them with `update_adjustments`, then correct what this photo needs (exposure and white balance differ per frame). Check each one; don't batch blindly.
4. Presets and syncing across a shoot over MCP are [coming](tools.md#coming) (#120). The user can also copy and paste settings in the GUI.
