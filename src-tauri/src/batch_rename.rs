//! Batch rename planning: turns selected images into photos (a RAW+JPEG pair
//! and every sidecar that belongs to it), orders them by capture time, renders
//! the template once per photo, and lists every file move with its conflicts.
//! The moves are carried out by `two_phase_rename`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::file_management::{ensure_card_writable_for_paths, parse_virtual_path};
use crate::file_naming::{self, NamingContext, PhotoFacts};
use crate::formats::{is_raw_file, is_supported_image_file};
use crate::two_phase_rename::FileMove;

/// Sidecars named after the image file: `IMG.ARW.xmp`.
const IMAGE_SIDECAR_EXTENSIONS: &[&str] = &["rrdata", "rrexif", "xmp", "dop", "pp3"];
/// Sidecars named after the stem: `IMG.xmp`.
const STEM_SIDECAR_EXTENSIONS: &[&str] = &["xmp", "acr", "dop"];
/// Camera files RapidRoom does not open but that are part of a pair.
const PAIR_ONLY_EXTENSIONS: &[&str] = &["hif", "heic", "heif"];

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum GroupMode {
    /// The whole selection is one group.
    Selection,
    /// Shots within `group_seconds` of the previous one share a group.
    #[default]
    Auto,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenameOptions {
    #[serde(default)]
    pub group_mode: GroupMode,
    #[serde(default)]
    pub group_seconds: Option<f64>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Image,
    Sidecar,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// Another file in this rename would get the same name.
    InBatch,
    /// A file that is not being renamed already has this name.
    Existing,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEntry {
    pub from: String,
    pub to: String,
    pub kind: EntryKind,
    /// Index of the photo this file belongs to, in capture order.
    pub photo: usize,
    pub conflict: Option<Conflict>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreview {
    pub entries: Vec<PreviewEntry>,
    pub errors: Vec<String>,
    pub photo_count: usize,
    pub conflict_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathChange {
    pub from: String,
    pub to: String,
}

/// What a rename (or its undo) did, for albums, caches and the frontend.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenameOutcome {
    /// Every file that moved.
    pub files: Vec<PathChange>,
    /// Image paths as the library knows them, including `?vc=` virtual copies.
    pub images: Vec<PathChange>,
}

impl RenameOutcome {
    pub fn image_map(&self) -> HashMap<String, String> {
        self.images
            .iter()
            .map(|c| (c.from.clone(), c.to.clone()))
            .collect()
    }

    fn reversed(&self) -> RenameOutcome {
        let flip = |changes: &[PathChange]| {
            changes
                .iter()
                .map(|c| PathChange {
                    from: c.to.clone(),
                    to: c.from.clone(),
                })
                .collect()
        };
        RenameOutcome {
            files: flip(&self.files),
            images: flip(&self.images),
        }
    }
}

#[derive(Debug, Clone)]
struct UnitFile {
    path: PathBuf,
    /// Everything after `stem.`, kept as written: `ARW`, `ARW.xmp`, `ARW.a1b2c3.rrdata`.
    rest: String,
    kind: EntryKind,
    /// The virtual copy id for `IMG.ARW.<id>.rrdata`.
    virtual_copy: Option<String>,
}

struct PhotoUnit {
    dir: PathBuf,
    stem: String,
    primary: PathBuf,
    facts: PhotoFacts,
    captured: DateTime<Utc>,
    sort_name: String,
    files: Vec<UnitFile>,
}

pub struct RenamePlan {
    pub preview: RenamePreview,
    moves: Vec<FileMove>,
    outcome: RenameOutcome,
}

impl RenamePlan {
    pub fn is_ready(&self) -> bool {
        self.preview.errors.is_empty() && self.preview.conflict_count == 0
    }

    pub fn refusal(&self) -> String {
        let mut reasons = self.preview.errors.clone();
        if self.preview.conflict_count > 0 {
            let first = self
                .preview
                .entries
                .iter()
                .find(|e| e.conflict.is_some())
                .map(|e| e.to.clone())
                .unwrap_or_default();
            reasons.push(format!(
                "{} file name(s) collide, for example {}",
                self.preview.conflict_count, first
            ));
        }
        reasons.join("; ")
    }
}

/// Builds the full rename plan without touching any file.
pub fn plan_rename(
    paths: &[String],
    template: &str,
    options: &RenameOptions,
) -> Result<RenamePlan, String> {
    ensure_card_writable_for_paths(paths)?;
    let mut units = discover_units(paths)?;
    units.sort_by(|a, b| {
        a.captured
            .cmp(&b.captured)
            .then_with(|| a.sort_name.cmp(&b.sort_name))
    });
    let groups = assign_groups(&units, options);
    let group_count = groups.last().map(|g| g.0).unwrap_or(0);
    let member_count = groups.iter().map(|g| g.1).max().unwrap_or(0);

    let mut errors: Vec<String> = Vec::new();
    let mut add_error = |e: String| {
        if !errors.contains(&e) {
            errors.push(e);
        }
    };
    for token in file_naming::unknown_tokens(template) {
        add_error(format!("Unknown token {{{}}}", token));
    }

    let mut stems: Vec<Option<String>> = Vec::with_capacity(units.len());
    for (index, unit) in units.iter().enumerate() {
        let ctx = NamingContext {
            source_path: &unit.primary,
            sequence: index + 1,
            total: units.len(),
            date: unit.captured,
            group: Some(groups[index]),
            group_count,
            member_count,
            facts: &unit.facts,
        };
        let stem = file_naming::render_strict(template, &ctx)
            .map_err(|e| e.to_string())
            .and_then(|stem| file_naming::validate_stem(&stem).map(|_| stem));
        match stem {
            Ok(stem) => stems.push(Some(stem)),
            Err(e) => {
                add_error(e);
                stems.push(None);
            }
        }
    }

    // Listed only now: reading metadata above can tidy up legacy sidecars.
    attach_files(&mut units)?;

    let mut entries = Vec::new();
    let mut moves = Vec::new();
    let mut outcome = RenameOutcome::default();
    for (index, (unit, new_stem)) in units.iter().zip(&stems).enumerate() {
        let Some(new_stem) = new_stem else {
            continue;
        };
        for file in &unit.files {
            let to = unit.dir.join(format!("{}.{}", new_stem, file.rest));
            entries.push(PreviewEntry {
                from: file.path.to_string_lossy().into_owned(),
                to: to.to_string_lossy().into_owned(),
                kind: file.kind,
                photo: index,
                conflict: None,
            });
            if to == file.path {
                continue;
            }
            moves.push(FileMove::new(file.path.clone(), to.clone()));
            outcome.files.push(PathChange {
                from: file.path.to_string_lossy().into_owned(),
                to: to.to_string_lossy().into_owned(),
            });
            if file.kind == EntryKind::Image {
                outcome.images.push(PathChange {
                    from: file.path.to_string_lossy().into_owned(),
                    to: to.to_string_lossy().into_owned(),
                });
            }
            if let Some(id) = &file.virtual_copy {
                let image_rest = file.rest.split('.').next().unwrap_or_default();
                let old_image = unit.dir.join(format!(
                    "{}.{}",
                    file_stem_of(&file.path, &file.rest),
                    image_rest
                ));
                let new_image = unit.dir.join(format!("{}.{}", new_stem, image_rest));
                outcome.images.push(PathChange {
                    from: format!("{}?vc={}", old_image.to_string_lossy(), id),
                    to: format!("{}?vc={}", new_image.to_string_lossy(), id),
                });
            }
        }
    }

    let conflict_count = mark_conflicts(&mut entries);
    Ok(RenamePlan {
        preview: RenamePreview {
            entries,
            errors,
            photo_count: units.len(),
            conflict_count,
        },
        moves,
        outcome,
    })
}

fn file_stem_of(path: &Path, rest: &str) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name[..name.len() - rest.len() - 1].to_string()
}

fn discover_units(paths: &[String]) -> Result<Vec<PhotoUnit>, String> {
    let mut units: Vec<PhotoUnit> = Vec::new();
    let mut selected: Vec<Vec<PathBuf>> = Vec::new();
    let mut index_by_key: HashMap<(PathBuf, String), usize> = HashMap::new();
    for path in paths {
        let (source, _) = parse_virtual_path(path);
        if !source.is_file() {
            return Err(format!("File not found: {}", path));
        }
        let dir = source
            .parent()
            .ok_or("Could not get parent directory")?
            .to_path_buf();
        let stem = source
            .file_stem()
            .ok_or_else(|| format!("Invalid file name: {}", path))?
            .to_string_lossy()
            .into_owned();
        let key = (dir.clone(), stem.to_lowercase());
        let index = *index_by_key.entry(key).or_insert_with(|| {
            units.push(PhotoUnit {
                dir: dir.clone(),
                stem: stem.clone(),
                primary: source.clone(),
                facts: PhotoFacts::new(&source),
                captured: DateTime::<Utc>::MIN_UTC,
                sort_name: String::new(),
                files: Vec::new(),
            });
            selected.push(Vec::new());
            units.len() - 1
        });
        if !selected[index].contains(&source) {
            selected[index].push(source);
        }
    }

    let mut listings = HashMap::new();
    for (unit, selected) in units.iter_mut().zip(&selected) {
        let raw_member = list_dir(&mut listings, &unit.dir)?
            .iter()
            .filter_map(|(name, path)| classify(name, &unit.stem, path))
            .find(|f| f.kind == EntryKind::Image && is_raw_file(&f.path))
            .map(|f| f.path);
        unit.primary = raw_member.unwrap_or_else(|| selected[0].clone());
        unit.facts = PhotoFacts::new(&unit.primary);
        unit.captured = capture_time(&unit.primary, &unit.facts);
        unit.sort_name = unit
            .primary
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
    }
    Ok(units)
}

type Listing = Vec<(String, PathBuf)>;

fn list_dir<'a>(
    listings: &'a mut HashMap<PathBuf, Listing>,
    dir: &Path,
) -> Result<&'a Listing, String> {
    if !listings.contains_key(dir) {
        let mut entries: Listing = fs::read_dir(dir)
            .map_err(|e| format!("Could not read {}: {}", dir.display(), e))?
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        entries.sort();
        listings.insert(dir.to_path_buf(), entries);
    }
    Ok(&listings[dir])
}

/// Finds every file that belongs to each photo: its images and sidecars.
fn attach_files(units: &mut [PhotoUnit]) -> Result<(), String> {
    let mut listings = HashMap::new();
    let mut claimed: HashSet<PathBuf> = HashSet::new();
    for unit in units.iter_mut() {
        let mut files = Vec::new();
        for (name, path) in list_dir(&mut listings, &unit.dir)? {
            let Some(file) = classify(name, &unit.stem, path) else {
                continue;
            };
            if !claimed.insert(path.clone()) {
                return Err(format!(
                    "{} belongs to more than one selected photo",
                    path.display()
                ));
            }
            files.push(file);
        }
        files.sort_by_key(|f| (f.kind != EntryKind::Image, f.rest.to_lowercase()));
        unit.files = files;
    }
    Ok(())
}

/// Decides whether a directory entry belongs to the photo with this stem.
fn classify(name: &str, stem: &str, path: &Path) -> Option<UnitFile> {
    let rest = strip_prefix_ignore_case(name, stem)?.strip_prefix('.')?;
    let parts: Vec<&str> = rest.split('.').collect();
    let is_one_of = |ext: &str, list: &[&str]| list.iter().any(|e| e.eq_ignore_ascii_case(ext));
    let is_image_ext = |ext: &str| {
        !ext.is_empty()
            && (is_supported_image_file(format!("x.{}", ext))
                || is_one_of(ext, PAIR_ONLY_EXTENSIONS))
    };

    let (kind, virtual_copy) = match parts.as_slice() {
        [ext] if is_image_ext(ext) => (EntryKind::Image, None),
        [ext] if is_one_of(ext, STEM_SIDECAR_EXTENSIONS) => (EntryKind::Sidecar, None),
        [image, ext] if is_image_ext(image) && is_one_of(ext, IMAGE_SIDECAR_EXTENSIONS) => {
            (EntryKind::Sidecar, None)
        }
        [image, id, ext]
            if is_image_ext(image) && !id.is_empty() && ext.eq_ignore_ascii_case("rrdata") =>
        {
            (EntryKind::Sidecar, Some(id.to_string()))
        }
        _ => return None,
    };
    Some(UnitFile {
        path: path.to_path_buf(),
        rest: rest.to_string(),
        kind,
        virtual_copy,
    })
}

fn strip_prefix_ignore_case<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let mut name_chars = name.char_indices();
    for p in prefix.chars() {
        let (_, n) = name_chars.next()?;
        if !n.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    match name_chars.next() {
        Some((index, _)) => Some(&name[index..]),
        None => Some(""),
    }
}

/// DateTimeOriginal plus SubSecTimeOriginal, so bursts sort in shooting order.
fn capture_time(path: &Path, facts: &PhotoFacts) -> DateTime<Utc> {
    let base = crate::exif_processing::get_creation_date_from_path(path);
    let sub_seconds = facts
        .exif()
        .get("SubSecTimeOriginal")
        .map(|s| {
            s.trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|digits| !digits.is_empty())
        .map(|digits| {
            let digits: String = digits.chars().take(9).collect();
            let nanos: i64 = digits.parse().unwrap_or(0);
            nanos * 10i64.pow(9 - digits.len() as u32)
        })
        .unwrap_or(0);
    base + chrono::Duration::nanoseconds(sub_seconds)
}

/// `(group, member)` for each unit, both 1-based. Units must be in capture order.
fn assign_groups(units: &[PhotoUnit], options: &RenameOptions) -> Vec<(usize, usize)> {
    let max_gap_ms = (options.group_seconds.unwrap_or(1.0).max(0.0) * 1000.0).round() as i64;
    let mut groups = Vec::with_capacity(units.len());
    let (mut group, mut member) = (0, 0);
    for (index, unit) in units.iter().enumerate() {
        let new_group = match options.group_mode {
            GroupMode::Selection => index == 0,
            GroupMode::Auto => {
                index == 0
                    || (unit.captured - units[index - 1].captured).num_milliseconds() > max_gap_ms
            }
        };
        if new_group {
            group += 1;
            member = 0;
        }
        member += 1;
        groups.push((group, member));
    }
    groups
}

fn mark_conflicts(entries: &mut [PreviewEntry]) -> usize {
    let sources: HashSet<String> = entries.iter().map(|e| e.from.clone()).collect();
    let sources_folded: HashSet<String> = sources.iter().map(|s| s.to_lowercase()).collect();

    let mut target_count: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        *target_count.entry(entry.to.to_lowercase()).or_default() += 1;
    }

    let mut listings: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut conflicts = 0;
    for entry in entries.iter_mut() {
        if target_count[&entry.to.to_lowercase()] > 1 {
            entry.conflict = Some(Conflict::InBatch);
        } else if entry.to != entry.from && !sources.contains(&entry.to) {
            let to = Path::new(&entry.to);
            let dir = to.parent().unwrap_or(Path::new("")).to_path_buf();
            let names = listings.entry(dir.clone()).or_insert_with(|| {
                fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .filter_map(Result::ok)
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let name = to
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // An exact entry is a different file. A hit that only differs in
            // case is the same file on a case-insensitive disk if it is moving.
            let taken = names.contains(&name)
                || (fs::symlink_metadata(to).is_ok()
                    && !sources_folded.contains(&entry.to.to_lowercase()));
            if taken {
                entry.conflict = Some(Conflict::Existing);
            }
        }
        if entry.conflict.is_some() {
            conflicts += 1;
        }
    }
    conflicts
}

/// Carries out a plan. Nothing is renamed if the plan has errors or conflicts.
pub fn apply_plan(plan: &RenamePlan) -> Result<RenameOutcome, String> {
    if !plan.is_ready() {
        return Err(plan.refusal());
    }
    crate::two_phase_rename::execute(&plan.moves)?;
    Ok(plan.outcome.clone())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenameJournal {
    pub renamed_at: String,
    pub outcome: RenameOutcome,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UndoInfo {
    pub renamed_at: String,
    pub photo_count: usize,
    pub file_count: usize,
}

pub fn save_journal(journal_path: &Path, outcome: &RenameOutcome) -> Result<(), String> {
    if outcome.files.is_empty() {
        return Ok(());
    }
    let journal = RenameJournal {
        renamed_at: Utc::now().to_rfc3339(),
        outcome: outcome.clone(),
    };
    let json = serde_json::to_string_pretty(&journal).map_err(|e| e.to_string())?;
    if let Some(parent) = journal_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::file_management::write_file_atomically(journal_path, json).map_err(|e| e.to_string())
}

pub fn load_journal(journal_path: &Path) -> Option<RenameJournal> {
    let content = fs::read_to_string(journal_path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn undo_info(journal_path: &Path) -> Option<UndoInfo> {
    let journal = load_journal(journal_path)?;
    let photos: HashSet<String> = journal
        .outcome
        .images
        .iter()
        .filter(|c| !c.from.contains("?vc="))
        .map(|c| {
            let path = Path::new(&c.from);
            path.with_extension("").to_string_lossy().to_lowercase()
        })
        .collect();
    Some(UndoInfo {
        renamed_at: journal.renamed_at,
        photo_count: photos.len(),
        file_count: journal.outcome.files.len(),
    })
}

/// Moves the files of the last rename back. Refuses if any of them was moved,
/// deleted or replaced since, or if an old name has been taken.
pub fn undo_from_journal(journal_path: &Path) -> Result<RenameOutcome, String> {
    let journal = load_journal(journal_path).ok_or("There is no rename to undo.")?;
    let reversed = journal.outcome.reversed();
    let sources: Vec<String> = reversed.files.iter().map(|c| c.from.clone()).collect();
    ensure_card_writable_for_paths(&sources)?;

    let moving: HashSet<&str> = reversed.files.iter().map(|c| c.from.as_str()).collect();
    for change in &reversed.files {
        if !Path::new(&change.from).is_file() {
            return Err(format!("Cannot undo: {} is no longer there.", change.from));
        }
        if fs::symlink_metadata(&change.to).is_ok() && !moving.contains(change.to.as_str()) {
            return Err(format!("Cannot undo: {} already exists.", change.to));
        }
    }

    let moves: Vec<FileMove> = reversed
        .files
        .iter()
        .map(|c| FileMove::new(&c.from, &c.to))
        .collect();
    crate::two_phase_rename::execute(&moves)?;
    let _ = fs::remove_file(journal_path);
    Ok(reversed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::card_mode_test_support::CardMode;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, name).unwrap();
        path
    }

    fn set_exif(dir: &Path, image: &str, date: &str, subsec: Option<&str>) {
        let mut exif = serde_json::Map::new();
        exif.insert("DateTimeOriginal".into(), date.into());
        if let Some(s) = subsec {
            exif.insert("SubSecTimeOriginal".into(), s.into());
        }
        let sidecar = serde_json::json!({
            "version": 1, "rating": 0, "adjustments": null, "exif": exif
        });
        fs::write(
            dir.join(format!("{}.rrdata", image)),
            serde_json::to_string(&sidecar).unwrap(),
        )
        .unwrap();
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn p(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn rename(paths: &[String], template: &str) -> Result<RenameOutcome, String> {
        apply_plan(&plan_rename(paths, template, &RenameOptions::default())?)
    }

    #[test]
    fn renames_a_pair_and_every_sidecar_as_one_photo() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let raw = touch(d, "IMG_0001.ARW");
        for name in [
            "IMG_0001.JPG",
            "IMG_0001.HIF",
            "IMG_0001.ARW.rrdata",
            "IMG_0001.ARW.a1b2c3.rrdata",
            "IMG_0001.ARW.rrexif",
            "IMG_0001.JPG.rrdata",
            "IMG_0001.xmp",
            "IMG_0001.ARW.xmp",
            "IMG_0001.acr",
            "IMG_0001.ARW.dop",
            "IMG_0001.dop",
            "IMG_0001.ARW.pp3",
            "IMG_0001.ARW.cos",
            "IMG_0001.txt",
            "IMG_00011.ARW",
            "IMG_0001-1.ARW",
            "other.JPG",
        ] {
            touch(d, name);
        }

        let outcome = rename(&[p(&raw)], "trip_{sequence}").unwrap();
        assert_eq!(
            names(d),
            vec![
                "IMG_0001-1.ARW",
                "IMG_0001.ARW.cos",
                "IMG_0001.txt",
                "IMG_00011.ARW",
                "other.JPG",
                "trip_1.ARW",
                "trip_1.ARW.a1b2c3.rrdata",
                "trip_1.ARW.dop",
                "trip_1.ARW.pp3",
                "trip_1.ARW.rrdata",
                "trip_1.ARW.rrexif",
                "trip_1.ARW.xmp",
                "trip_1.HIF",
                "trip_1.JPG",
                "trip_1.JPG.rrdata",
                "trip_1.acr",
                "trip_1.dop",
                "trip_1.xmp",
            ]
        );
        assert_eq!(outcome.files.len(), 13);
        let images = outcome.image_map();
        assert_eq!(images[&p(&raw)], p(&d.join("trip_1.ARW")));
        assert_eq!(
            images[&p(&d.join("IMG_0001.JPG"))],
            p(&d.join("trip_1.JPG"))
        );
        assert_eq!(
            images[&format!("{}?vc=a1b2c3", p(&raw))],
            format!("{}?vc=a1b2c3", p(&d.join("trip_1.ARW")))
        );
    }

    #[test]
    fn selecting_both_members_of_a_pair_uses_one_sequence_number() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let a_raw = touch(d, "A.CR3");
        let a_jpg = touch(d, "a.jpg");
        let b = touch(d, "B.CR3");
        set_exif(d, "A.CR3", "2025:10:09 10:00:00", None);
        set_exif(d, "B.CR3", "2025:10:09 10:00:05", None);

        rename(&[p(&b), p(&a_jpg), p(&a_raw)], "x_{sequence}").unwrap();
        assert_eq!(
            names(d),
            vec![
                "x_1.CR3",
                "x_1.CR3.rrdata",
                "x_1.jpg",
                "x_2.CR3",
                "x_2.CR3.rrdata"
            ]
        );
    }

    #[test]
    fn a_prefix_lookalike_is_never_picked_up() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let one = touch(d, "IMG_1.ARW");
        touch(d, "IMG_10.ARW");
        touch(d, "IMG_10.ARW.xmp");
        touch(d, "IMG_1.ARW.xmp");
        let plan = plan_rename(&[p(&one)], "new", &RenameOptions::default()).unwrap();
        let from: Vec<&str> = plan
            .preview
            .entries
            .iter()
            .map(|e| e.from.as_str())
            .collect();
        assert_eq!(from, vec![p(&one), p(&d.join("IMG_1.ARW.xmp"))]);
    }

    #[test]
    fn sidecars_match_the_stem_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let raw = touch(d, "IMG_0002.NEF");
        touch(d, "img_0002.nef.XMP");
        touch(d, "Img_0002.Xmp");
        rename(&[p(&raw)], "n").unwrap();
        assert_eq!(names(d), vec!["n.NEF", "n.Xmp", "n.nef.XMP"]);
    }

    #[test]
    fn sequence_follows_capture_order_with_sub_seconds_then_name() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let shots = [
            ("c.ARW", "2025:10:09 10:00:00", Some("50")),
            ("a.ARW", "2025:10:09 10:00:00", Some("90")),
            ("b.ARW", "2025:10:09 10:00:00", Some("10")),
            ("e.ARW", "2025:10:09 10:00:00", Some("50")),
            ("d.ARW", "2025:10:09 09:59:59", None),
        ];
        let mut paths = Vec::new();
        for (name, date, subsec) in shots {
            paths.push(p(&touch(d, name)));
            set_exif(d, name, date, subsec);
        }
        let outcome = rename(&paths, "{sequence}_{original_filename}").unwrap();
        let mut renamed: Vec<String> = outcome
            .images
            .iter()
            .map(|c| {
                Path::new(&c.to)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        renamed.sort();
        assert_eq!(
            renamed,
            vec!["1_d.ARW", "2_b.ARW", "3_c.ARW", "4_e.ARW", "5_a.ARW"]
        );
    }

    #[test]
    fn groups_bursts_by_time_gap_or_by_selection() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let shots = [
            ("1.ARW", "2025:10:09 10:00:00", "00"),
            ("2.ARW", "2025:10:09 10:00:00", "50"),
            ("3.ARW", "2025:10:09 10:00:01", "40"),
            ("4.ARW", "2025:10:09 10:00:05", "00"),
            ("5.ARW", "2025:10:09 10:00:05", "30"),
        ];
        let paths: Vec<String> = shots
            .iter()
            .map(|(name, date, subsec)| {
                set_exif(d, name, date, Some(subsec));
                p(&touch(d, name))
            })
            .collect();
        let targets = |options: RenameOptions| -> Vec<String> {
            plan_rename(&paths, "{group}-{member}", &options)
                .unwrap()
                .preview
                .entries
                .iter()
                .filter(|e| e.kind == EntryKind::Image)
                .map(|e| {
                    Path::new(&e.to)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        };
        assert_eq!(
            targets(RenameOptions::default()),
            vec![
                "0001-01.ARW",
                "0001-02.ARW",
                "0001-03.ARW",
                "0002-01.ARW",
                "0002-02.ARW"
            ]
        );
        assert_eq!(
            targets(RenameOptions {
                group_mode: GroupMode::Auto,
                group_seconds: Some(0.5),
            }),
            vec![
                "0001-01.ARW",
                "0001-02.ARW",
                "0002-01.ARW",
                "0003-01.ARW",
                "0003-02.ARW"
            ]
        );
        assert_eq!(
            targets(RenameOptions {
                group_mode: GroupMode::Selection,
                group_seconds: None,
            }),
            vec![
                "0001-01.ARW",
                "0001-02.ARW",
                "0001-03.ARW",
                "0001-04.ARW",
                "0001-05.ARW"
            ]
        );
    }

    #[test]
    fn flags_collisions_inside_the_batch_and_with_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let a = touch(d, "a.jpg");
        let b = touch(d, "b.jpg");
        touch(d, "taken.png");
        let c = touch(d, "c.png");

        let plan = plan_rename(&[p(&a), p(&b)], "same", &RenameOptions::default()).unwrap();
        assert_eq!(plan.preview.conflict_count, 2);
        assert!(
            plan.preview
                .entries
                .iter()
                .all(|e| e.conflict == Some(Conflict::InBatch))
        );
        assert!(apply_plan(&plan).unwrap_err().contains("collide"));

        let plan = plan_rename(&[p(&c)], "taken", &RenameOptions::default()).unwrap();
        assert_eq!(plan.preview.entries[0].conflict, Some(Conflict::Existing));
        assert!(apply_plan(&plan).is_err());
        assert_eq!(names(d), vec!["a.jpg", "b.jpg", "c.png", "taken.png"]);
    }

    #[test]
    fn renaming_onto_a_name_the_batch_frees_is_not_a_collision() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let one = touch(d, "1.jpg");
        let two = touch(d, "2.jpg");
        set_exif(d, "1.jpg", "2025:10:09 10:00:02", None);
        set_exif(d, "2.jpg", "2025:10:09 10:00:01", None);
        let plan =
            plan_rename(&[p(&one), p(&two)], "{sequence}", &RenameOptions::default()).unwrap();
        assert_eq!(plan.preview.conflict_count, 0);
        apply_plan(&plan).unwrap();
        assert_eq!(fs::read_to_string(d.join("1.jpg")).unwrap(), "2.jpg");
        assert_eq!(fs::read_to_string(d.join("2.jpg")).unwrap(), "1.jpg");
    }

    #[test]
    fn unknown_tokens_and_bad_names_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let a = touch(dir.path(), "a.jpg");
        let plan = plan_rename(&[p(&a)], "{sequense}_{foo}", &RenameOptions::default()).unwrap();
        assert_eq!(
            plan.preview.errors,
            vec!["Unknown token {sequense}", "Unknown token {foo}"]
        );
        assert!(apply_plan(&plan).is_err());
        let plan = plan_rename(&[p(&a)], "../x", &RenameOptions::default()).unwrap();
        assert_eq!(plan.preview.errors.len(), 1);
        assert!(apply_plan(&plan).is_err());
        assert_eq!(names(dir.path()), vec!["a.jpg"]);
    }

    #[test]
    fn rating_and_label_tokens_read_the_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("Lofoten");
        fs::create_dir(&d).unwrap();
        let a = touch(&d, "a.jpg");
        fs::write(
            d.join("a.jpg.rrdata"),
            r#"{"version":1,"rating":4,"rating_is_explicit":true,"adjustments":null,"tags":["color:green"],
                "exif":{"Model":"X-T5","LensModel":"XF33mmF1.4","PhotographicSensitivity":"200","FocalLength":"33"}}"#,
        )
        .unwrap();
        rename(
            &[p(&a)],
            "{folder}_{stars}_{rating}_{label}_{camera}_{lens}_{iso}_{focal}",
        )
        .unwrap();
        assert_eq!(
            names(&d),
            vec![
                "Lofoten_4star_4_green_X-T5_XF33mmF1.4_200_33mm.jpg",
                "Lofoten_4star_4_green_X-T5_XF33mmF1.4_200_33mm.jpg.rrdata"
            ]
        );
    }

    #[test]
    fn undo_restores_the_old_names_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("photos");
        fs::create_dir(&d).unwrap();
        let journal = dir.path().join("data").join("last_rename.json");
        let raw = touch(&d, "IMG_0001.ARW");
        touch(&d, "IMG_0001.JPG");
        touch(&d, "IMG_0001.ARW.xmp");
        let before = names(&d);

        let outcome = rename(&[p(&raw)], "trip").unwrap();
        save_journal(&journal, &outcome).unwrap();
        assert_ne!(names(&d), before);

        // A fresh load from disk, as after a restart.
        let info = undo_info(&journal).unwrap();
        assert_eq!((info.photo_count, info.file_count), (1, 3));
        let undone = undo_from_journal(&journal).unwrap();
        assert_eq!(names(&d), before);
        assert_eq!(undone.image_map()[&p(&d.join("trip.ARW"))], p(&raw));
        assert!(!journal.exists());
        assert!(undo_from_journal(&journal).is_err());
    }

    #[test]
    fn undo_refuses_when_files_changed_since() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let journal = d.join("journal.json");
        let a = touch(d, "a.jpg");
        let outcome = rename(&[p(&a)], "b").unwrap();
        save_journal(&journal, &outcome).unwrap();

        touch(d, "a.jpg");
        assert!(
            undo_from_journal(&journal)
                .unwrap_err()
                .contains("already exists")
        );
        fs::remove_file(d.join("a.jpg")).unwrap();
        fs::remove_file(d.join("b.jpg")).unwrap();
        assert!(
            undo_from_journal(&journal)
                .unwrap_err()
                .contains("no longer there")
        );
        assert!(journal.exists());
    }

    #[test]
    fn refuses_the_card() {
        let root = tempfile::tempdir().unwrap();
        let card = root.path().join("card");
        fs::create_dir(&card).unwrap();
        let a = touch(&card, "a.jpg");
        let _mode = CardMode::on(&card);
        assert!(plan_rename(&[p(&a)], "b", &RenameOptions::default()).is_err());
        assert_eq!(names(&card), vec!["a.jpg"]);
    }
}
