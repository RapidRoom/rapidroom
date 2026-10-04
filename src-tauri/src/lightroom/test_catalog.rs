//! Synthetic Lightroom Classic catalogs for tests. The tables keep Lightroom's
//! names and untyped columns, but only the columns the importers read.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, ToSql, params};

pub const SET: &str = "com.adobe.ag.library.group";
pub const COLLECTION: &str = "com.adobe.ag.library.collection";
pub const SMART: &str = "com.adobe.ag.library.smart_collection";

const SCHEMA: &str = "
    CREATE TABLE AgLibraryRootFolder (id_local INTEGER PRIMARY KEY, absolutePath UNIQUE NOT NULL DEFAULT '', name NOT NULL DEFAULT '', relativePathFromCatalog);
    CREATE TABLE AgLibraryFolder (id_local INTEGER PRIMARY KEY, pathFromRoot NOT NULL DEFAULT '', rootFolder INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE AgLibraryFile (id_local INTEGER PRIMARY KEY, baseName NOT NULL DEFAULT '', extension NOT NULL DEFAULT '', folder INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE Adobe_images (id_local INTEGER PRIMARY KEY, copyName, masterImage INTEGER, rootFile INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE AgLibraryCollection (id_local INTEGER PRIMARY KEY, creationId NOT NULL DEFAULT '', genealogy NOT NULL DEFAULT '', imageCount, name NOT NULL DEFAULT '', parent INTEGER, systemOnly NOT NULL DEFAULT '');
    CREATE TABLE AgLibraryCollectionImage (id_local INTEGER PRIMARY KEY, collection INTEGER NOT NULL DEFAULT 0, image INTEGER NOT NULL DEFAULT 0, pick NOT NULL DEFAULT 0, positionInCollection);
    CREATE TABLE AgLibraryCollectionContent (id_local INTEGER PRIMARY KEY, collection INTEGER NOT NULL DEFAULT 0, content, owningModule);
";

pub struct TestCatalog {
    pub connection: Connection,
}

impl TestCatalog {
    pub fn create(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let connection = Connection::open(path).unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        Self { connection }
    }

    pub fn root(&self, id: i64, absolute_path: &str, relative_to_catalog: Option<&str>) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryRootFolder (id_local, absolutePath, relativePathFromCatalog) VALUES (?1, ?2, ?3)",
                params![id, absolute_path, relative_to_catalog],
            )
            .unwrap();
    }

    pub fn folder(&self, id: i64, root: i64, path_from_root: &str) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryFolder (id_local, rootFolder, pathFromRoot) VALUES (?1, ?2, ?3)",
                params![id, root, path_from_root],
            )
            .unwrap();
    }

    /// Adds a file and its master image, both with id `id`.
    pub fn image(&self, id: i64, folder: i64, base_name: &str, extension: &str) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryFile (id_local, folder, baseName, extension) VALUES (?1, ?2, ?3, ?4)",
                params![id, folder, base_name, extension],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO Adobe_images (id_local, rootFile) VALUES (?1, ?1)",
                params![id],
            )
            .unwrap();
    }

    pub fn virtual_copy(&self, id: i64, master: i64) {
        self.connection
            .execute(
                "INSERT INTO Adobe_images (id_local, rootFile, masterImage, copyName) VALUES (?1, ?2, ?2, 'Copy 1')",
                params![id, master],
            )
            .unwrap();
    }

    pub fn collection(&self, id: i64, parent: Option<i64>, name: &dyn ToSql, creation_id: &str) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryCollection (id_local, parent, name, creationId, systemOnly) VALUES (?1, ?2, ?3, ?4, 0)",
                params![id, parent, name, creation_id],
            )
            .unwrap();
    }

    pub fn system_collection(&self, id: i64, name: &str) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryCollection (id_local, name, creationId, systemOnly) VALUES (?1, ?2, ?3, 1)",
                params![id, name, COLLECTION],
            )
            .unwrap();
    }

    pub fn content(&self, collection: i64, owning_module: &str, content: &str) {
        self.connection
            .execute(
                "INSERT INTO AgLibraryCollectionContent (collection, owningModule, content) VALUES (?1, ?2, ?3)",
                params![collection, owning_module, content],
            )
            .unwrap();
    }

    pub fn add(&self, collection: i64, images: &[i64]) {
        for image in images {
            self.connection
                .execute(
                    "INSERT INTO AgLibraryCollectionImage (collection, image) VALUES (?1, ?2)",
                    params![collection, image],
                )
                .unwrap();
        }
    }
}

/// Writes an empty file, creating its folders.
pub fn touch(path: &Path) -> PathBuf {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"raw").unwrap();
    path.to_path_buf()
}

/// Every file below `folder` with its contents and modification time.
pub fn folder_state(folder: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    walkdir::WalkDir::new(folder)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let metadata = entry.metadata().unwrap();
            (
                entry.path().to_path_buf(),
                (
                    fs::read(entry.path()).unwrap(),
                    metadata.modified().unwrap(),
                ),
            )
        })
        .collect()
}
