use crate::file_management::{PresetFolder, PresetItem};
use std::collections::BTreeMap;

fn starter_library() -> Result<Vec<PresetItem>, String> {
    let mut groups: BTreeMap<&str, Vec<crate::file_management::Preset>> = BTreeMap::new();
    {
        let xmp = include_str!("../resources/starter-presets/BandW/Green Filter.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/Green Filter.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/BandW/High Contrast.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/High Contrast.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/BandW/Neutral.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/Neutral.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/BandW/Red Filter.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/Red Filter.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/BandW/Sepia.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/Sepia.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/BandW/Soft.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:BandW/Soft.xmp".to_string();
        groups.entry("BandW").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Color/Cool.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Color/Cool.xmp".to_string();
        groups.entry("Color").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Color/Matte.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Color/Matte.xmp".to_string();
        groups.entry("Color").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Color/Muted.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Color/Muted.xmp".to_string();
        groups.entry("Color").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Color/Punch.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Color/Punch.xmp".to_string();
        groups.entry("Color").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Color/Warm.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Color/Warm.xmp".to_string();
        groups.entry("Color").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Darken.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Darken.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Flat.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Flat.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Lighten.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Lighten.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Linear.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Linear.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Matte.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Matte.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Medium Contrast.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Medium Contrast.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Curve/Strong Contrast.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Curve/Strong Contrast.xmp".to_string();
        groups.entry("Curve").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Grain/Heavy.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Grain/Heavy.xmp".to_string();
        groups.entry("Grain").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Grain/Light.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Grain/Light.xmp".to_string();
        groups.entry("Grain").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Grain/Medium.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Grain/Medium.xmp".to_string();
        groups.entry("Grain").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Vignetting/Light.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Vignetting/Light.xmp".to_string();
        groups.entry("Vignetting").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Vignetting/Strong.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Vignetting/Strong.xmp".to_string();
        groups.entry("Vignetting").or_default().push(preset);
    }
    {
        let xmp = include_str!("../resources/starter-presets/Vignetting/Subtle.xmp");
        let mut preset = crate::preset_converter::convert_xmp_to_preset(xmp)?;
        preset.id = "rapidroom-starter:Vignetting/Subtle.xmp".to_string();
        groups.entry("Vignetting").or_default().push(preset);
    }
    Ok(groups
        .into_iter()
        .map(|(name, children)| {
            PresetItem::Folder(PresetFolder {
                id: format!("rapidroom-starter:{name}"),
                name: if name == "BandW" {
                    "B&W".to_string()
                } else {
                    name.to_string()
                },
                children,
            })
        })
        .collect())
}

/// Add the built-in pack for existing libraries without replacing saved edits,
/// favourites, order or renamed/moved presets. Stable IDs prevent duplicates.
pub fn include_styles(library: &mut Vec<PresetItem>) -> Result<(), String> {
    if library.iter().any(
        |item| matches!(item, PresetItem::Folder(folder) if folder.id == "rapidroom-styles:v1"),
    ) {
        return Ok(());
    }
    let ids: std::collections::HashSet<String> = library
        .iter()
        .flat_map(|item| match item {
            PresetItem::Folder(folder) => folder
                .children
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            PresetItem::Preset(preset) => vec![preset.id.clone()],
        })
        .collect();
    let pack: crate::file_management::PresetFile =
        serde_json::from_str(include_str!("../resources/styles/styles-v1.json"))
            .map_err(|error| error.to_string())?;
    for item in pack.presets {
        if let PresetItem::Folder(mut folder) = item {
            folder.children.retain(|preset| !ids.contains(&preset.id));
            if !folder.children.is_empty() {
                library.insert(0, PresetItem::Folder(folder));
            }
        }
    }
    Ok(())
}

pub fn library() -> Result<Vec<PresetItem>, String> {
    let mut library = starter_library()?;
    include_styles(&mut library)?;
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_presets_are_unique_profile_free_and_usable() {
        let library = library().unwrap();
        let presets: Vec<_> = library
            .iter()
            .flat_map(|item| match item {
                PresetItem::Folder(folder) => folder.children.iter().collect::<Vec<_>>(),
                PresetItem::Preset(preset) => vec![preset],
            })
            .collect();
        assert_eq!(presets.len(), 35);
        let ids: std::collections::HashSet<_> = presets.iter().map(|p| &p.id).collect();
        assert_eq!(ids.len(), 35);
        for preset in presets {
            assert!(preset.unavailable_reason.is_none(), "{}", preset.name);
            assert!(
                !preset.adjustments.as_object().unwrap().is_empty(),
                "{}",
                preset.name
            );
        }
    }
}
