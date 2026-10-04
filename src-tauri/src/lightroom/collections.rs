use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use rusqlite::types::ValueRef;
use serde::Serialize;
use tauri::AppHandle;

use super::catalog::{Catalog, RootResolution, text};
use crate::file_management::{AlbumItem, get_albums, save_albums};

const COLLECTION_SET: &str = "com.adobe.ag.library.group";
const COLLECTION: &str = "com.adobe.ag.library.collection";
const SMART_COLLECTION: &str = "com.adobe.ag.library.smart_collection";
const SMART_COLLECTION_RULES: &str = "ag.library.smart_collection";
const MISSING_IMAGE_LIST_LIMIT: usize = 500;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RootFolderPreview {
    catalog_path: String,
    local_path: String,
    resolution: RootResolution,
    found: bool,
    image_count: usize,
    missing_image_count: usize,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LightroomImportPreview {
    catalog_name: String,
    collection_count: usize,
    group_count: usize,
    matched_image_count: usize,
    missing_image_count: usize,
    /// The first missing files, sorted; `missing_image_count` has the total.
    missing_images: Vec<String>,
    /// Smart collections that are not imported, as `Set / Collection` paths.
    smart_collections: Vec<String>,
    /// Books, slideshows, prints and web galleries, which are not imported.
    skipped_other_count: usize,
    /// The root folders that hold images of imported collections.
    root_folders: Vec<RootFolderPreview>,
    replaces_previous_import: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Set,
    Collection,
    Smart,
    Other,
}

struct CollectionRow {
    id: i64,
    parent: Option<i64>,
    name: String,
    kind: Kind,
}

struct CollectionImport {
    preview: LightroomImportPreview,
    group: AlbumItem,
}

fn collection_rows(catalog: &Catalog) -> Result<Vec<CollectionRow>, String> {
    let mut smart_rules = HashSet::new();
    if catalog
        .require_tables(&["AgLibraryCollectionContent"])
        .is_ok()
    {
        let mut statement = catalog
            .connection()
            .prepare("SELECT collection FROM AgLibraryCollectionContent WHERE owningModule = ?1")
            .map_err(|error| error.to_string())?;
        for collection in statement
            .query_map([SMART_COLLECTION_RULES], |row| row.get::<_, Option<i64>>(0))
            .map_err(|error| error.to_string())?
        {
            smart_rules.extend(collection.map_err(|error| error.to_string())?);
        }
    }

    let mut statement = catalog
        .connection()
        .prepare(
            "SELECT id_local, parent, name, creationId FROM AgLibraryCollection
            WHERE COALESCE(CAST(systemOnly AS REAL), 0) = 0
            ORDER BY id_local",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let parent = match row.get_ref(1)? {
                ValueRef::Integer(parent) if parent != 0 => Some(parent),
                _ => None,
            };
            let kind = match text(row, 3)?.as_str() {
                _ if smart_rules.contains(&id) => Kind::Smart,
                COLLECTION_SET => Kind::Set,
                COLLECTION => Kind::Collection,
                SMART_COLLECTION => Kind::Smart,
                _ => Kind::Other,
            };
            Ok(CollectionRow {
                id,
                parent,
                name: text(row, 2)?,
                kind,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())
}

fn memberships(catalog: &Catalog) -> Result<Vec<(i64, i64)>, String> {
    let mut statement = catalog
        .connection()
        .prepare("SELECT collection, image FROM AgLibraryCollectionImage")
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .map_err(|error| error.to_string())?
        .filter_map(|row| match row {
            Ok((Some(collection), Some(image))) => Some(Ok((collection, image))),
            Ok(_) => None,
            Err(error) => Some(Err(error.to_string())),
        })
        .collect()
}

struct TreeBuilder<'a> {
    children: HashMap<Option<i64>, Vec<&'a CollectionRow>>,
    images: HashMap<i64, Vec<String>>,
    id_prefix: String,
    group_count: usize,
    collection_count: usize,
    smart_collections: Vec<String>,
    skipped_other_count: usize,
}

impl TreeBuilder<'_> {
    /// The collections reachable from the top level through collection sets.
    fn reachable_collections(&self) -> HashSet<i64> {
        let mut collections = HashSet::new();
        let mut pending = vec![None];
        while let Some(parent) = pending.pop() {
            for row in self.children.get(&parent).into_iter().flatten() {
                match row.kind {
                    Kind::Set => pending.push(Some(row.id)),
                    Kind::Collection => {
                        collections.insert(row.id);
                    }
                    Kind::Smart | Kind::Other => {}
                }
            }
        }
        collections
    }

    fn build(&mut self, parent: Option<i64>, trail: &str) -> Vec<AlbumItem> {
        let rows = self.children.get(&parent).cloned().unwrap_or_default();
        let mut items = Vec::new();
        for row in rows {
            let id = format!("lightroom:{}:{}", self.id_prefix, row.id);
            let path = if trail.is_empty() {
                row.name.clone()
            } else {
                format!("{trail} / {}", row.name)
            };
            match row.kind {
                Kind::Set => {
                    self.group_count += 1;
                    let children = self.build(Some(row.id), &path);
                    items.push(AlbumItem::Group {
                        id,
                        name: row.name.clone(),
                        icon: None,
                        children,
                    });
                }
                Kind::Collection => {
                    self.collection_count += 1;
                    let mut images = self.images.remove(&row.id).unwrap_or_default();
                    images.sort();
                    images.dedup();
                    items.push(AlbumItem::Album {
                        id,
                        name: row.name.clone(),
                        icon: None,
                        images,
                    });
                }
                Kind::Smart => self.smart_collections.push(path),
                Kind::Other => self.skipped_other_count += 1,
            }
        }
        items
    }
}

fn catalog_key(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    blake3::hash(path.to_string_lossy().as_bytes())
        .to_hex()
        .to_string()
}

fn read_import(
    catalog: &Catalog,
    mappings: &HashMap<String, String>,
    existing: &[AlbumItem],
) -> Result<CollectionImport, String> {
    catalog.require_tables(&["AgLibraryCollection", "AgLibraryCollectionImage"])?;
    let roots = catalog.resolve_roots(mappings)?;
    let locations = catalog.image_locations()?;
    let rows = collection_rows(catalog)?;
    let key = catalog_key(catalog.path());
    let group_id = format!("lightroom-import:{key}");
    let mut builder = TreeBuilder {
        children: HashMap::new(),
        images: HashMap::new(),
        id_prefix: key[..12].to_string(),
        group_count: 0,
        collection_count: 0,
        smart_collections: Vec::new(),
        skipped_other_count: 0,
    };
    for row in &rows {
        builder.children.entry(row.parent).or_default().push(row);
    }
    let imported = builder.reachable_collections();

    let mut images: HashMap<i64, Vec<String>> = HashMap::new();
    let mut paths_by_root: HashMap<i64, HashSet<PathBuf>> = HashMap::new();
    let mut unresolved = HashSet::new();
    for (collection, image) in memberships(catalog)? {
        if !imported.contains(&collection) {
            continue;
        }
        let Some(location) = locations.get(&image) else {
            continue;
        };
        match roots.file_path(location) {
            Some(path) => {
                images
                    .entry(collection)
                    .or_default()
                    .push(path.to_string_lossy().into_owned());
                paths_by_root
                    .entry(location.root_folder)
                    .or_default()
                    .insert(path);
            }
            None => {
                let root = roots.get(location.root_folder).map_or_else(
                    || format!("(root folder {}) ", location.root_folder),
                    |root| root.catalog_path.clone(),
                );
                unresolved.insert(format!(
                    "{root}{}{}",
                    location.path_from_root, location.file_name
                ));
            }
        }
    }

    let mut root_folders = Vec::new();
    let mut missing_images = Vec::new();
    let mut matched_image_count = 0;
    for (root_id, paths) in &paths_by_root {
        let Some(root) = roots.get(*root_id) else {
            continue;
        };
        let missing: Vec<&PathBuf> = if root.found {
            paths.par_iter().filter(|path| !path.is_file()).collect()
        } else {
            paths.iter().collect()
        };
        matched_image_count += paths.len() - missing.len();
        root_folders.push(RootFolderPreview {
            catalog_path: root.catalog_path.clone(),
            local_path: root.local_path.to_string_lossy().into_owned(),
            resolution: root.resolution,
            found: root.found,
            image_count: paths.len(),
            missing_image_count: missing.len(),
        });
        missing_images.extend(
            missing
                .iter()
                .map(|path| path.to_string_lossy().into_owned()),
        );
    }
    root_folders.sort_by(|a, b| a.catalog_path.cmp(&b.catalog_path));
    let missing_image_count = missing_images.len() + unresolved.len();
    missing_images.extend(unresolved);
    missing_images.sort();
    missing_images.truncate(MISSING_IMAGE_LIST_LIMIT);

    builder.images = images;
    let children = builder.build(None, "");
    builder.smart_collections.sort();

    Ok(CollectionImport {
        preview: LightroomImportPreview {
            catalog_name: catalog.name(),
            collection_count: builder.collection_count,
            group_count: builder.group_count,
            matched_image_count,
            missing_image_count,
            missing_images,
            smart_collections: builder.smart_collections,
            skipped_other_count: builder.skipped_other_count,
            root_folders,
            replaces_previous_import: contains_item(existing, &group_id),
        },
        group: AlbumItem::Group {
            id: group_id,
            name: catalog.name(),
            icon: None,
            children,
        },
    })
}

fn item_id(item: &AlbumItem) -> &str {
    match item {
        AlbumItem::Album { id, .. } | AlbumItem::Group { id, .. } => id,
    }
}

fn contains_item(items: &[AlbumItem], target: &str) -> bool {
    items.iter().any(|item| {
        item_id(item) == target
            || matches!(item, AlbumItem::Group { children, .. } if contains_item(children, target))
    })
}

/// Puts a catalog's import group where an earlier import of the same catalog
/// is, keeping that group's name and icon, and drops any further copies. A
/// catalog imported for the first time is added at the top level.
fn merge_import(tree: &mut Vec<AlbumItem>, group: AlbumItem) {
    fn replace(items: &mut Vec<AlbumItem>, target: &str, group: &mut Option<AlbumItem>) {
        let mut index = 0;
        while index < items.len() {
            if item_id(&items[index]) == target {
                match group.take() {
                    Some(mut new_group) => {
                        if let (
                            AlbumItem::Group { name, icon, .. },
                            AlbumItem::Group {
                                name: new_name,
                                icon: new_icon,
                                ..
                            },
                        ) = (&items[index], &mut new_group)
                        {
                            *new_name = name.clone();
                            *new_icon = icon.clone();
                        }
                        items[index] = new_group;
                        index += 1;
                    }
                    None => {
                        items.remove(index);
                    }
                }
                continue;
            }
            if let AlbumItem::Group { children, .. } = &mut items[index] {
                replace(children, target, group);
            }
            index += 1;
        }
    }

    let target = item_id(&group).to_string();
    let mut group = Some(group);
    replace(tree, &target, &mut group);
    if let Some(group) = group {
        tree.push(group);
    }
}

#[tauri::command]
pub async fn preview_lightroom_collections(
    path: String,
    mappings: HashMap<String, String>,
    app_handle: AppHandle,
) -> Result<LightroomImportPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = Catalog::open(Path::new(&path))?;
        let albums = get_albums(app_handle)?;
        Ok(read_import(&catalog, &mappings, &albums)?.preview)
    })
    .await
    .map_err(|error| format!("Task failed: {error}"))?
}

#[tauri::command]
pub async fn import_lightroom_collections(
    path: String,
    mappings: HashMap<String, String>,
    app_handle: AppHandle,
) -> Result<LightroomImportPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = Catalog::open(Path::new(&path))?;
        let mut albums = get_albums(app_handle.clone())?;
        let import = read_import(&catalog, &mappings, &albums)?;
        merge_import(&mut albums, import.group);
        save_albums(albums, app_handle)?;
        Ok(import.preview)
    })
    .await
    .map_err(|error| format!("Task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::sort_album_tree;
    use crate::lightroom::test_catalog::{
        COLLECTION, SET, SMART, TestCatalog, folder_state, touch,
    };
    use rusqlite::Connection;
    use serde_json::json;

    struct Library {
        dir: tempfile::TempDir,
        catalog: PathBuf,
        mappings: HashMap<String, String>,
        windows_root: PathBuf,
        mac_root: PathBuf,
        native_root: PathBuf,
        native_catalog_path: String,
        unicode: String,
        missing: String,
        mac: String,
        gone: String,
        native: String,
    }

    fn text_of(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// A catalog made on Windows and macOS, opened on this machine: two roots
    /// mapped to local folders, one root that is gone and one local root.
    fn library() -> Library {
        let dir = tempfile::tempdir().unwrap();
        let windows_root = dir.path().join("from-windows");
        let mac_root = dir.path().join("from-mac");
        let native_root = dir.path().join("native");
        let unicode = touch(
            &windows_root
                .join("2019")
                .join("Ünïcödé 日本")
                .join("IMG_0001.CR2"),
        );
        let missing = windows_root.join("2019").join("IMG_0002.CR2");
        let mac = touch(&mac_root.join("Reise").join("Café #1.NEF"));
        let native = touch(&native_root.join("2020").join("DSC_0005.ARW"));
        let gone = native_path_of("/Volumes/Gone/")
            .join("Old")
            .join("Scan.TIF");
        let native_catalog_path = format!("{}/", text_of(&native_root).replace('\\', "/"));

        let catalog = dir.path().join("Lightroom").join("Lightroom Catalog.lrcat");
        let lr = TestCatalog::create(&catalog);
        lr.root(1, "C:/Users/Benny/Pictures/", None);
        lr.root(2, "/Volumes/Fotos/", None);
        lr.root(3, "/Volumes/Gone/", None);
        lr.root(4, &native_catalog_path, None);
        lr.root(5, "/Volumes/Unused/", None);
        lr.folder(10, 1, "2019/Ünïcödé 日本/");
        lr.folder(11, 1, "2019/");
        lr.folder(12, 2, "Reise/");
        lr.folder(13, 3, "Old/");
        lr.folder(14, 4, "2020/");
        lr.folder(15, 5, "");
        lr.folder(16, 1, "../outside/");
        lr.image(1, 10, "IMG_0001", "CR2");
        lr.image(2, 11, "IMG_0002", "CR2");
        lr.image(3, 12, "Café #1", "NEF");
        lr.image(4, 13, "Scan", "TIF");
        lr.image(5, 14, "DSC_0005", "ARW");
        lr.virtual_copy(6, 1);
        lr.image(7, 15, "Unused", "DNG");
        lr.image(8, 16, "evil", "CR2");

        lr.collection(1, None, &"2019", SET);
        lr.collection(2, Some(1), &"Reisen", SET);
        lr.collection(3, Some(2), &"Japan", COLLECTION);
        lr.add(3, &[1, 2, 6]);
        lr.collection(4, Some(1), &"Five stars", SMART);
        lr.add(4, &[5]);
        lr.collection(5, None, &"2019", SET);
        lr.collection(6, Some(5), &"Japan", COLLECTION);
        lr.add(6, &[3]);
        lr.collection(7, None, &"Favourites", COLLECTION);
        lr.add(7, &[1, 3, 4, 8]);
        lr.collection(8, None, &2020, COLLECTION);
        lr.add(8, &[5]);
        lr.collection(9, None, &"Photo book", "com.adobe.ag.book");
        lr.add(9, &[7]);
        lr.system_collection(10, "Quick Collection");
        lr.add(10, &[7]);
        lr.collection(11, Some(5), &"Rated", COLLECTION);
        lr.content(
            11,
            "ag.library.smart_collection",
            "s = { combine = \"intersect\", }",
        );
        lr.collection(12, None, &"Filtered", COLLECTION);
        lr.content(12, "com.adobe.ag.library.filter", "s = { }");
        lr.add(12, &[5]);
        lr.collection(13, Some(99), &"Orphan", COLLECTION);
        lr.add(13, &[7]);
        drop(lr);

        let mappings = HashMap::from([
            (
                "C:/Users/Benny/Pictures/".to_string(),
                text_of(&windows_root),
            ),
            ("/Volumes/Fotos/".to_string(), text_of(&mac_root)),
        ]);
        Library {
            dir,
            catalog,
            mappings,
            windows_root,
            mac_root,
            native_root,
            native_catalog_path,
            unicode: text_of(&unicode),
            missing: text_of(&missing),
            mac: text_of(&mac),
            gone: text_of(&gone),
            native: text_of(&native),
        }
    }

    fn native_path_of(catalog_path: &str) -> PathBuf {
        #[cfg(windows)]
        let catalog_path = catalog_path.replace('/', "\\");
        Path::new(&catalog_path).components().collect()
    }

    fn sorted(mut paths: Vec<String>) -> Vec<String> {
        paths.sort();
        paths
    }

    fn count(items: &[AlbumItem], target: &str) -> usize {
        items
            .iter()
            .map(|item| {
                usize::from(item_id(item) == target)
                    + match item {
                        AlbumItem::Group { children, .. } => count(children, target),
                        AlbumItem::Album { .. } => 0,
                    }
            })
            .sum()
    }

    #[test]
    fn previews_counts_skipped_collections_and_root_mappings() {
        let lib = library();
        let catalog = Catalog::open(&lib.catalog).unwrap();
        let preview = read_import(&catalog, &lib.mappings, &[]).unwrap().preview;

        assert_eq!(preview.catalog_name, "Lightroom Catalog");
        assert_eq!(preview.group_count, 3);
        assert_eq!(preview.collection_count, 5);
        assert_eq!(
            preview.smart_collections,
            ["2019 / Five stars", "2019 / Rated"]
        );
        assert_eq!(preview.skipped_other_count, 1);
        assert_eq!(preview.matched_image_count, 3);
        assert_eq!(preview.missing_image_count, 3);
        assert_eq!(
            preview.missing_images,
            sorted(vec![
                lib.missing.clone(),
                lib.gone.clone(),
                "C:/Users/Benny/Pictures/../outside/evil.CR2".to_string(),
            ])
        );
        assert!(!preview.replaces_previous_import);

        let root = |catalog_path: &str| {
            let root = preview
                .root_folders
                .iter()
                .find(|root| root.catalog_path == catalog_path)
                .unwrap();
            (
                root.local_path.clone(),
                root.resolution,
                root.found,
                root.image_count,
                root.missing_image_count,
            )
        };
        assert_eq!(preview.root_folders.len(), 4);
        assert_eq!(
            root("C:/Users/Benny/Pictures/"),
            (
                text_of(&lib.windows_root),
                RootResolution::Mapped,
                true,
                2,
                1
            )
        );
        assert_eq!(
            root("/Volumes/Fotos/"),
            (text_of(&lib.mac_root), RootResolution::Mapped, true, 1, 0)
        );
        assert_eq!(
            root("/Volumes/Gone/"),
            (
                text_of(&native_path_of("/Volumes/Gone/")),
                RootResolution::Original,
                false,
                1,
                1
            )
        );
        assert_eq!(
            root(&lib.native_catalog_path),
            (
                text_of(&lib.native_root),
                RootResolution::Original,
                true,
                1,
                0
            )
        );
    }

    #[test]
    fn builds_nested_and_duplicate_named_sets_with_local_paths() {
        let lib = library();
        let catalog = Catalog::open(&lib.catalog).unwrap();
        let group = read_import(&catalog, &lib.mappings, &[]).unwrap().group;
        let key = catalog_key(&lib.catalog);
        let id = |local: i64| format!("lightroom:{}:{local}", &key[..12]);

        assert_eq!(
            serde_json::to_value(&group).unwrap(),
            json!({
                "type": "group",
                "id": format!("lightroom-import:{key}"),
                "name": "Lightroom Catalog",
                "icon": null,
                "children": [
                    {
                        "type": "group", "id": id(1), "name": "2019", "icon": null,
                        "children": [{
                            "type": "group", "id": id(2), "name": "Reisen", "icon": null,
                            "children": [{
                                "type": "album", "id": id(3), "name": "Japan", "icon": null,
                                "images": sorted(vec![lib.unicode.clone(), lib.missing.clone()]),
                            }],
                        }],
                    },
                    {
                        "type": "group", "id": id(5), "name": "2019", "icon": null,
                        "children": [{
                            "type": "album", "id": id(6), "name": "Japan", "icon": null,
                            "images": [lib.mac],
                        }],
                    },
                    {
                        "type": "album", "id": id(7), "name": "Favourites", "icon": null,
                        "images": sorted(vec![lib.unicode.clone(), lib.mac.clone(), lib.gone.clone()]),
                    },
                    {
                        "type": "album", "id": id(8), "name": "2020", "icon": null,
                        "images": [lib.native],
                    },
                    {
                        "type": "album", "id": id(12), "name": "Filtered", "icon": null,
                        "images": [lib.native],
                    },
                ],
            })
        );
    }

    #[test]
    fn repeated_imports_replace_the_earlier_import_where_the_user_put_it() {
        let lib = library();
        let user_items = vec![
            AlbumItem::Group {
                id: "trips".into(),
                name: "Trips".into(),
                icon: Some("plane".into()),
                children: vec![AlbumItem::Album {
                    id: "best".into(),
                    name: "Best".into(),
                    icon: None,
                    images: vec![lib.mac.clone()],
                }],
            },
            AlbumItem::Album {
                id: "alpha".into(),
                name: "Alpha".into(),
                icon: None,
                images: Vec::new(),
            },
            AlbumItem::Album {
                id: "zulu".into(),
                name: "zulu".into(),
                icon: Some("star".into()),
                images: vec![lib.native.clone()],
            },
        ];
        let group_id = format!("lightroom-import:{}", catalog_key(&lib.catalog));

        let mut tree = user_items.clone();
        let catalog = Catalog::open(&lib.catalog).unwrap();
        let import = read_import(&catalog, &lib.mappings, &tree).unwrap();
        merge_import(&mut tree, import.group);
        sort_album_tree(&mut tree);
        drop(catalog);
        assert_eq!(count(&tree, &group_id), 1);
        let mut without_import = tree.clone();
        without_import.retain(|item| item_id(item) != group_id);
        assert_eq!(
            serde_json::to_value(&without_import).unwrap(),
            serde_json::to_value(&user_items).unwrap()
        );

        // The user moves the import into "Trips" and renames it...
        let index = tree
            .iter()
            .position(|item| item_id(item) == group_id)
            .unwrap();
        let AlbumItem::Group { children, .. } = tree.remove(index) else {
            panic!("the import is a group");
        };
        let AlbumItem::Group {
            children: trips, ..
        } = &mut tree[0]
        else {
            panic!("Trips is a group");
        };
        trips.push(AlbumItem::Group {
            id: group_id.clone(),
            name: "Old Lightroom".into(),
            icon: Some("heart".into()),
            children,
        });
        // ...a second copy is left over from an older build...
        tree.push(AlbumItem::Group {
            id: group_id.clone(),
            name: "Lightroom Catalog".into(),
            icon: None,
            children: Vec::new(),
        });
        // ...and adds a collection in Lightroom before importing again.
        let lr = TestCatalog {
            connection: Connection::open(&lib.catalog).unwrap(),
        };
        lr.collection(14, None, &"New", COLLECTION);
        lr.add(14, &[3]);
        drop(lr);

        let catalog = Catalog::open(&lib.catalog).unwrap();
        let import = read_import(&catalog, &lib.mappings, &tree).unwrap();
        assert!(import.preview.replaces_previous_import);
        assert_eq!(import.preview.collection_count, 6);
        merge_import(&mut tree, import.group);
        sort_album_tree(&mut tree);

        assert_eq!(count(&tree, &group_id), 1);
        let names: Vec<&str> = tree
            .iter()
            .map(|item| match item {
                AlbumItem::Album { name, .. } | AlbumItem::Group { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(names, ["Trips", "Alpha", "zulu"]);
        let AlbumItem::Group {
            children: trips, ..
        } = &tree[0]
        else {
            panic!("Trips is a group");
        };
        let AlbumItem::Group {
            name,
            icon,
            children,
            ..
        } = &trips[0]
        else {
            panic!("the import is still in Trips, before the Best album");
        };
        assert_eq!(name, "Old Lightroom");
        assert_eq!(icon.as_deref(), Some("heart"));
        assert!(children.iter().any(|item| matches!(
            item,
            AlbumItem::Album { name, images, .. } if name == "New" && images == std::slice::from_ref(&lib.mac)
        )));
        assert_eq!(
            serde_json::to_value(&trips[1]).unwrap(),
            serde_json::to_value(match &user_items[0] {
                AlbumItem::Group { children, .. } => &children[0],
                AlbumItem::Album { .. } => unreachable!(),
            })
            .unwrap()
        );
    }

    #[test]
    fn preview_and_import_leave_the_catalog_and_originals_untouched() {
        let lib = library();
        let before = folder_state(lib.dir.path());

        let catalog = Catalog::open(&lib.catalog).unwrap();
        let mut tree = Vec::new();
        read_import(&catalog, &lib.mappings, &tree).unwrap();
        let import = read_import(&catalog, &lib.mappings, &tree).unwrap();
        merge_import(&mut tree, import.group);
        drop(catalog);

        assert_eq!(folder_state(lib.dir.path()), before);
        assert_eq!(count(&tree, item_id(&tree[0])), 1);
    }
}
