//! Read-only access to Lightroom Classic catalogs (`.lrcat`), shared by every
//! Lightroom importer.
//!
//! A catalog is never written to. A cleanly closed catalog is opened in place as
//! an immutable SQLite file, so SQLite takes no locks and creates no `-wal` or
//! `-shm` files next to it. A catalog with a leftover `-wal` or `-journal` is
//! copied to a temporary folder first, and SQLite recovers the copy.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};
use serde::Serialize;

const REQUIRED_TABLES: [&str; 4] = [
    "AgLibraryRootFolder",
    "AgLibraryFolder",
    "AgLibraryFile",
    "Adobe_images",
];

pub struct Catalog {
    connection: Connection,
    path: PathBuf,
    _snapshot: Option<tempfile::TempDir>,
}

pub struct RootFolder {
    pub id: i64,
    /// The absolute path as stored by the machine that created the catalog.
    pub catalog_path: String,
    /// The root's path relative to the catalog file, when Lightroom recorded one.
    pub relative_to_catalog: Option<String>,
}

pub struct ImageLocation {
    pub root_folder: i64,
    /// The folder below the root, `/`-separated as Lightroom stores it.
    pub path_from_root: String,
    pub file_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RootResolution {
    /// The absolute path stored in the catalog.
    Original,
    /// A folder the user chose for this root.
    Mapped,
    /// The path Lightroom recorded relative to the catalog file.
    BesideCatalog,
}

pub struct ResolvedRoot {
    pub catalog_path: String,
    pub local_path: PathBuf,
    pub resolution: RootResolution,
    pub found: bool,
}

pub struct ResolvedRoots(HashMap<i64, ResolvedRoot>);

impl Catalog {
    pub fn open(path: &Path) -> Result<Self, String> {
        let is_catalog = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lrcat"));
        if !is_catalog {
            return Err("Please select a Lightroom Classic catalog (.lrcat).".into());
        }
        if !path.is_file() {
            return Err(format!("Lightroom catalog not found: {}", path.display()));
        }
        if with_suffix(path, ".lock").exists() {
            return Err(
                "Lightroom Classic has this catalog open. Close Lightroom Classic and try again."
                    .into(),
            );
        }

        let needs_recovery = ["-wal", "-journal"].iter().any(|suffix| {
            fs::metadata(with_suffix(path, suffix)).is_ok_and(|metadata| metadata.len() > 0)
        });
        let open_error =
            |error: rusqlite::Error| format!("Could not open Lightroom catalog: {error}");
        let (connection, snapshot) = if needs_recovery {
            let (directory, copy) = snapshot(path)
                .map_err(|error| format!("Could not copy the Lightroom catalog: {error}"))?;
            let connection = Connection::open_with_flags(
                &copy,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(open_error)?;
            connection
                .pragma_update(None, "query_only", true)
                .map_err(open_error)?;
            (connection, Some(directory))
        } else {
            let connection = Connection::open_with_flags(
                immutable_uri(path),
                OpenFlags::SQLITE_OPEN_READ_ONLY
                    | OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(open_error)?;
            (connection, None)
        };

        let catalog = Self {
            connection,
            path: path.to_path_buf(),
            _snapshot: snapshot,
        };
        catalog.require_tables(&REQUIRED_TABLES)?;
        Ok(catalog)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Lightroom Catalog".into())
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn require_tables(&self, tables: &[&str]) -> Result<(), String> {
        let not_a_catalog =
            |error: rusqlite::Error| format!("This is not a Lightroom Classic catalog ({error}).");
        let mut statement = self
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .map_err(not_a_catalog)?;
        let existing = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(not_a_catalog)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(not_a_catalog)?;
        match tables.iter().find(|table| !existing.contains(**table)) {
            Some(table) => Err(format!(
                "This is not a Lightroom Classic catalog (it has no {table} table)."
            )),
            None => Ok(()),
        }
    }

    fn has_column(&self, table: &str, column: &str) -> bool {
        self.connection
            .prepare(&format!("SELECT {column} FROM {table} LIMIT 0"))
            .is_ok()
    }

    pub fn root_folders(&self) -> Result<Vec<RootFolder>, String> {
        let relative = if self.has_column("AgLibraryRootFolder", "relativePathFromCatalog") {
            "relativePathFromCatalog"
        } else {
            "NULL"
        };
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT id_local, absolutePath, {relative} FROM AgLibraryRootFolder ORDER BY id_local"
            ))
            .map_err(|error| error.to_string())?;
        statement
            .query_map([], |row| {
                Ok(RootFolder {
                    id: row.get(0)?,
                    catalog_path: text(row, 1)?,
                    relative_to_catalog: Some(text(row, 2)?).filter(|path| !path.is_empty()),
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())
    }

    /// The master file of every image, keyed by `Adobe_images.id_local`.
    /// Virtual copies share their master's file.
    pub fn image_locations(&self) -> Result<HashMap<i64, ImageLocation>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT i.id_local, fo.rootFolder, fo.pathFromRoot, f.baseName, f.extension
                FROM Adobe_images i
                JOIN AgLibraryFile f ON f.id_local = i.rootFile
                JOIN AgLibraryFolder fo ON fo.id_local = f.folder",
            )
            .map_err(|error| error.to_string())?;
        statement
            .query_map([], |row| {
                let base_name = text(row, 3)?;
                let extension = text(row, 4)?;
                let file_name = if extension.is_empty() {
                    base_name
                } else {
                    format!("{base_name}.{extension}")
                };
                Ok((
                    row.get(0)?,
                    ImageLocation {
                        root_folder: row.get(1)?,
                        path_from_root: text(row, 2)?,
                        file_name,
                    },
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())
    }

    /// Finds each root folder on this machine. `mappings` maps a root's catalog
    /// path to a folder the user chose; a mapping that exists wins, then the
    /// catalog's own path, then the path Lightroom recorded relative to the catalog.
    pub fn resolve_roots(
        &self,
        mappings: &HashMap<String, String>,
    ) -> Result<ResolvedRoots, String> {
        let catalog_folder = self.path.parent().unwrap_or(Path::new(""));
        Ok(ResolvedRoots(
            self.root_folders()?
                .into_iter()
                .map(|root| (root.id, resolve_root(root, mappings, catalog_folder)))
                .collect(),
        ))
    }
}

impl ResolvedRoots {
    pub fn get(&self, id: i64) -> Option<&ResolvedRoot> {
        self.0.get(&id)
    }

    /// The local path of a catalog file, or `None` when its root is unknown or a
    /// path segment isn't a plain file or folder name.
    pub fn file_path(&self, location: &ImageLocation) -> Option<PathBuf> {
        let mut path = self.0.get(&location.root_folder)?.local_path.clone();
        for segment in location
            .path_from_root
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            if !is_plain_name(segment) {
                return None;
            }
            path.push(segment);
        }
        if !is_plain_name(&location.file_name) {
            return None;
        }
        path.push(&location.file_name);
        Some(path)
    }
}

fn resolve_root(
    root: RootFolder,
    mappings: &HashMap<String, String>,
    catalog_folder: &Path,
) -> ResolvedRoot {
    let mapped = mappings
        .get(&root.catalog_path)
        .map(|path| without_trailing_separator(Path::new(path)));
    let original = native_path(&root.catalog_path);
    let beside_catalog = root
        .relative_to_catalog
        .as_deref()
        .and_then(|relative| join_relative(catalog_folder, relative));

    let candidates = [
        mapped.clone().map(|path| (path, RootResolution::Mapped)),
        Some((original.clone(), RootResolution::Original)),
        beside_catalog.map(|path| (path, RootResolution::BesideCatalog)),
    ];
    let found = candidates
        .into_iter()
        .flatten()
        .find(|(path, _)| path.is_absolute() && path.is_dir());
    let (local_path, resolution, found) = match (found, mapped) {
        (Some((path, resolution)), _) => (path, resolution, true),
        (None, Some(path)) => (path, RootResolution::Mapped, false),
        (None, None) => (original, RootResolution::Original, false),
    };
    ResolvedRoot {
        catalog_path: root.catalog_path,
        local_path,
        resolution,
        found,
    }
}

/// Lightroom writes `/` on every platform and ends folders with one.
fn native_path(catalog_path: &str) -> PathBuf {
    #[cfg(windows)]
    let catalog_path = catalog_path.replace('/', "\\");
    without_trailing_separator(Path::new(&catalog_path))
}

fn without_trailing_separator(path: &Path) -> PathBuf {
    path.components().collect()
}

fn join_relative(base: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if !path.pop() {
                    return None;
                }
            }
            name if is_plain_name(name) => path.push(name),
            _ => return None,
        }
    }
    Some(path)
}

fn is_plain_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(first)), None) if first == OsStr::new(name)
    )
}

/// Reads a column as text whatever its storage class: Lightroom's columns are untyped.
pub fn text(row: &Row, index: usize) -> rusqlite::Result<String> {
    Ok(match row.get_ref(index)? {
        ValueRef::Null => String::new(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
    })
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn snapshot(path: &Path) -> std::io::Result<(tempfile::TempDir, PathBuf)> {
    let directory = tempfile::Builder::new()
        .prefix("rapidroom-lrcat-")
        .tempdir()?;
    let copy = directory.path().join("catalog.lrcat");
    fs::copy(path, &copy)?;
    for suffix in ["-wal", "-journal"] {
        let source = with_suffix(path, suffix);
        if source.is_file() {
            fs::copy(source, with_suffix(&copy, suffix))?;
        }
    }
    Ok((directory, copy))
}

/// An SQLite URI that opens `path` read-only and immutable.
fn immutable_uri(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy();
    #[cfg(windows)]
    let text = text.replace('\\', "/");
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.push_str("?mode=ro&immutable=1");
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightroom::test_catalog::{TestCatalog, folder_state};

    fn roots_of(catalog: &Catalog, mappings: &[(&str, &Path)]) -> ResolvedRoots {
        let mappings = mappings
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string_lossy().into_owned()))
            .collect();
        catalog.resolve_roots(&mappings).unwrap()
    }

    #[test]
    fn opens_a_closed_catalog_in_place_without_touching_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Kätalog #1 100% [LR].lrcat");
        TestCatalog::create(&path).root(1, "/Volumes/Fotos/", None);
        let before = folder_state(dir.path());

        let catalog = Catalog::open(&path).unwrap();
        assert_eq!(catalog.name(), "Kätalog #1 100% [LR]");
        assert_eq!(
            catalog.root_folders().unwrap()[0].catalog_path,
            "/Volumes/Fotos/"
        );
        assert!(catalog._snapshot.is_none());
        drop(catalog);

        assert_eq!(folder_state(dir.path()), before);
    }

    #[test]
    fn recovers_a_catalog_with_a_leftover_wal_from_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live/Catalog.lrcat");
        let fixture = TestCatalog::create(&live);
        fixture
            .connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        fixture.root(1, "C:/Users/Benny/Pictures/", None);

        // A copy taken while the writer is open looks like a catalog after a crash:
        // the root folder exists only in the -wal file.
        let crashed = dir.path().join("crashed");
        fs::create_dir_all(&crashed).unwrap();
        for name in ["Catalog.lrcat", "Catalog.lrcat-wal"] {
            fs::copy(dir.path().join("live").join(name), crashed.join(name)).unwrap();
        }
        drop(fixture);
        let before = folder_state(&crashed);

        let catalog = Catalog::open(&crashed.join("Catalog.lrcat")).unwrap();
        assert!(catalog._snapshot.is_some());
        let roots = catalog.root_folders().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].catalog_path, "C:/Users/Benny/Pictures/");
        assert!(
            catalog
                .connection()
                .execute("DELETE FROM AgLibraryRootFolder", [])
                .is_err()
        );
        drop(catalog);

        assert_eq!(folder_state(&crashed), before);
    }

    #[test]
    fn refuses_a_catalog_that_lightroom_has_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Catalog.lrcat");
        TestCatalog::create(&path);
        fs::write(dir.path().join("Catalog.lrcat.lock"), "").unwrap();

        let error = Catalog::open(&path).err().unwrap();
        assert!(error.contains("Close Lightroom Classic"), "{error}");
    }

    #[test]
    fn rejects_files_that_are_not_catalogs() {
        let dir = tempfile::tempdir().unwrap();

        let wrong_extension = dir.path().join("Catalog.sqlite");
        TestCatalog::create(&wrong_extension);
        assert!(Catalog::open(&wrong_extension).is_err());

        let not_sqlite = dir.path().join("Notes.lrcat");
        fs::write(
            &not_sqlite,
            "not a database, just some text that is long enough",
        )
        .unwrap();
        let error = Catalog::open(&not_sqlite).err().unwrap();
        assert!(error.contains("not a Lightroom Classic catalog"), "{error}");

        let other_database = dir.path().join("Other.lrcat");
        Connection::open(&other_database)
            .unwrap()
            .execute_batch("CREATE TABLE notes (text);")
            .unwrap();
        let error = Catalog::open(&other_database).err().unwrap();
        assert!(error.contains("AgLibraryRootFolder"), "{error}");

        assert!(Catalog::open(&dir.path().join("Missing.lrcat")).is_err());
    }

    #[test]
    fn resolves_roots_from_mappings_the_catalog_path_or_beside_the_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let native = dir.path().join("native");
        let mapped = dir.path().join("mapped");
        let beside = dir.path().join("Drive/Photos");
        for folder in [&native, &mapped, &beside] {
            fs::create_dir_all(folder).unwrap();
        }
        let path = dir.path().join("Drive/Lightroom/Catalog.lrcat");
        let fixture = TestCatalog::create(&path);
        let native_catalog_path = format!("{}/", native.to_string_lossy());
        fixture.root(1, &native_catalog_path, None);
        fixture.root(2, "C:/Users/Benny/Pictures/", None);
        fixture.root(3, "E:/Photos/", Some("../Photos/"));
        fixture.root(4, "/Volumes/Gone/", None);
        fixture.root(5, "/Volumes/Moved/", None);
        drop(fixture);

        let catalog = Catalog::open(&path).unwrap();
        let roots = roots_of(
            &catalog,
            &[
                ("C:/Users/Benny/Pictures/", &mapped),
                (&native_catalog_path, &dir.path().join("does-not-exist")),
                ("/Volumes/Moved/", &dir.path().join("also-missing")),
            ],
        );

        let root = roots.get(1).unwrap();
        assert_eq!(root.local_path, native);
        assert_eq!(root.resolution, RootResolution::Original);
        assert!(root.found);

        let root = roots.get(2).unwrap();
        assert_eq!(root.local_path, mapped);
        assert_eq!(root.resolution, RootResolution::Mapped);
        assert!(root.found);

        let root = roots.get(3).unwrap();
        assert_eq!(root.local_path, beside);
        assert_eq!(root.resolution, RootResolution::BesideCatalog);
        assert!(root.found);

        let root = roots.get(4).unwrap();
        assert_eq!(root.local_path, native_path("/Volumes/Gone/"));
        assert_eq!(root.resolution, RootResolution::Original);
        assert!(!root.found);

        let root = roots.get(5).unwrap();
        assert_eq!(root.local_path, dir.path().join("also-missing"));
        assert_eq!(root.resolution, RootResolution::Mapped);
        assert!(!root.found);
    }

    #[test]
    fn builds_file_paths_from_plain_names_only() {
        let roots = ResolvedRoots(HashMap::from([(
            1,
            ResolvedRoot {
                catalog_path: "C:/Users/Benny/Pictures/".into(),
                local_path: PathBuf::from("/mnt/pictures"),
                resolution: RootResolution::Mapped,
                found: true,
            },
        )]));
        let location = |root_folder, path_from_root: &str, file_name: &str| ImageLocation {
            root_folder,
            path_from_root: path_from_root.into(),
            file_name: file_name.into(),
        };

        assert_eq!(
            roots.file_path(&location(1, "2019/Ünïcödé 日本/", "IMG_0001.CR2")),
            Some(PathBuf::from(
                "/mnt/pictures/2019/Ünïcödé 日本/IMG_0001.CR2"
            ))
        );
        assert_eq!(
            roots.file_path(&location(1, "", "IMG_0002.CR2")),
            Some(PathBuf::from("/mnt/pictures/IMG_0002.CR2"))
        );
        assert_eq!(roots.file_path(&location(1, "../../etc/", "passwd")), None);
        assert_eq!(roots.file_path(&location(1, "2019/", "..")), None);
        assert_eq!(roots.file_path(&location(2, "2019/", "IMG_0003.CR2")), None);
    }

    #[cfg(unix)]
    #[test]
    fn escapes_uri_characters_in_catalog_paths() {
        assert_eq!(
            immutable_uri(Path::new("/Photos & More/Kätalog?#1%.lrcat")),
            "file:///Photos%20%26%20More/K%C3%A4talog%3F%231%25.lrcat?mode=ro&immutable=1"
        );
    }

    #[test]
    fn catalog_paths_lose_their_trailing_separator() {
        assert_eq!(
            native_path("/Volumes/Fotos/"),
            PathBuf::from("/Volumes/Fotos")
        );
        assert_eq!(native_path("/"), PathBuf::from("/"));
        assert_eq!(
            join_relative(Path::new("/Drive/Lightroom"), "../Photos/"),
            Some(PathBuf::from("/Drive/Photos"))
        );
        assert_eq!(join_relative(Path::new("/"), "../../Photos/"), None);
    }
}
