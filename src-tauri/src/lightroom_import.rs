use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use tauri::AppHandle;

use crate::file_management::{AlbumItem, get_albums, save_albums};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightroomImportPreview {
    catalog_name: String,
    collection_count: usize,
    group_count: usize,
    matched_image_count: usize,
    missing_image_count: usize,
    smart_collection_count: usize,
    missing_roots: Vec<String>,
}

#[derive(Clone)]
struct CollectionRow {
    id: i64,
    parent: Option<i64>,
    name: String,
    is_group: bool,
}

struct ParsedCatalog {
    preview: LightroomImportPreview,
    tree: Vec<AlbumItem>,
}

fn open_catalog(path: &str) -> Result<Connection, String> {
    if Path::new(path).extension().and_then(|value| value.to_str()) != Some("lrcat") {
        return Err("Please select a Lightroom .lrcat catalog".into());
    }
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Could not open Lightroom catalog: {error}"))
}

fn parse_catalog(
    path: &str,
    replacements: &HashMap<String, String>,
) -> Result<ParsedCatalog, String> {
    let connection = open_catalog(path)?;
    let catalog_name = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Lightroom Catalog")
        .to_string();

    let mut roots = HashMap::new();
    let mut missing_roots = Vec::new();
    let mut statement = connection
        .prepare("SELECT id_local, absolutePath FROM AgLibraryRootFolder")
        .map_err(|error| format!("Unsupported Lightroom catalog schema: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (id, original) = row.map_err(|error| error.to_string())?;
        let resolved = replacements
            .get(&original)
            .cloned()
            .unwrap_or_else(|| original.clone());
        if !Path::new(&resolved).exists() && !missing_roots.contains(&original) {
            missing_roots.push(original.clone());
        }
        roots.insert(id, resolved);
    }

    let smart_ids: HashSet<i64> = connection
        .prepare("SELECT collection FROM AgLibraryCollectionContent WHERE owningModule = 'com.adobe.ag.library.filter' AND content IS NOT NULL")
        .map_err(|error| error.to_string())?
        .query_map([], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect();

    let mut collections = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id_local, parent, name, creationId FROM AgLibraryCollection WHERE CAST(systemOnly AS REAL) = 0",
    ).map_err(|error| error.to_string())?;
    for row in statement
        .query_map([], |row| {
            let creation: String = row.get(3)?;
            Ok(CollectionRow {
                id: row.get(0)?,
                parent: row.get(1)?,
                name: row.get(2)?,
                is_group: creation == "com.adobe.ag.library.group",
            })
        })
        .map_err(|error| error.to_string())?
    {
        let row = row.map_err(|error| error.to_string())?;
        if row.is_group || !smart_ids.contains(&row.id) {
            collections.push(row);
        }
    }

    let mut images: HashMap<i64, Vec<String>> = HashMap::new();
    let mut matched = HashSet::new();
    let mut missing = HashSet::new();
    let sql = "SELECT ci.collection, rf.absolutePath, fo.pathFromRoot, f.baseName, f.extension
        FROM AgLibraryCollectionImage ci
        JOIN Adobe_images i ON i.id_local = ci.image
        JOIN AgLibraryFile f ON f.id_local = i.rootFile
        JOIN AgLibraryFolder fo ON fo.id_local = f.folder
        JOIN AgLibraryRootFolder rf ON rf.id_local = fo.rootFolder";
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    for row in statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
    {
        let (collection, original_root, folder, base, extension) =
            row.map_err(|error| error.to_string())?;
        if smart_ids.contains(&collection) {
            continue;
        }
        let root = replacements
            .get(&original_root)
            .cloned()
            .unwrap_or(original_root);
        let filename = if extension.is_empty() {
            base
        } else {
            format!("{base}.{extension}")
        };
        let image_path = PathBuf::from(root)
            .join(folder)
            .join(filename)
            .to_string_lossy()
            .into_owned();
        if Path::new(&image_path).exists() {
            matched.insert(image_path.clone());
        } else {
            missing.insert(image_path.clone());
        }
        images.entry(collection).or_default().push(image_path);
    }

    fn build(
        parent: Option<i64>,
        rows: &[CollectionRow],
        images: &HashMap<i64, Vec<String>>,
        prefix: &str,
    ) -> Vec<AlbumItem> {
        rows.iter()
            .filter(|row| row.parent == parent)
            .map(|row| {
                let id = format!("lightroom:{prefix}:{}", row.id);
                if row.is_group {
                    AlbumItem::Group {
                        id,
                        name: row.name.clone(),
                        icon: None,
                        children: build(Some(row.id), rows, images, prefix),
                    }
                } else {
                    let mut paths = images.get(&row.id).cloned().unwrap_or_default();
                    paths.sort();
                    paths.dedup();
                    AlbumItem::Album {
                        id,
                        name: row.name.clone(),
                        icon: None,
                        images: paths,
                    }
                }
            })
            .collect()
    }
    let prefix = blake3::hash(path.as_bytes()).to_hex()[..12].to_string();
    let tree = build(None, &collections, &images, &prefix);
    let group_count = collections.iter().filter(|row| row.is_group).count();
    let collection_count = collections.len() - group_count;
    Ok(ParsedCatalog {
        preview: LightroomImportPreview {
            catalog_name,
            collection_count,
            group_count,
            matched_image_count: matched.len(),
            missing_image_count: missing.len(),
            smart_collection_count: smart_ids.len(),
            missing_roots,
        },
        tree,
    })
}

#[tauri::command]
pub fn inspect_lightroom_catalog(
    path: String,
    replacements: HashMap<String, String>,
) -> Result<LightroomImportPreview, String> {
    Ok(parse_catalog(&path, &replacements)?.preview)
}

#[tauri::command]
pub fn import_lightroom_collections(
    path: String,
    replacements: HashMap<String, String>,
    app_handle: AppHandle,
) -> Result<LightroomImportPreview, String> {
    let parsed = parse_catalog(&path, &replacements)?;
    if !parsed.preview.missing_roots.is_empty() {
        return Err("Relink all missing Lightroom root folders before importing".into());
    }
    let group_id = format!(
        "lightroom-import:{}",
        blake3::hash(path.as_bytes()).to_hex()
    );
    let mut albums = get_albums(app_handle.clone())?;
    albums.retain(|item| match item {
        AlbumItem::Group { id, .. } | AlbumItem::Album { id, .. } => id != &group_id,
    });
    albums.push(AlbumItem::Group {
        id: group_id,
        name: parsed.preview.catalog_name.clone(),
        icon: None,
        children: parsed.tree,
    });
    save_albums(albums, app_handle)?;
    Ok(parsed.preview)
}
