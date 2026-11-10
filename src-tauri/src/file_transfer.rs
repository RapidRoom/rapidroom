//! Collision-safe photo transfers. A photo is published only after its sidecars
//! have been staged; existing destination files are never replaced.
use serde::Serialize;
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Planning, sharing an existing sidecar, publication and rollback must be one
// critical section across all native copy/import commands in this process.
static TRANSFER_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub completed: usize,
    pub renamed: usize,
    pub identical_skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

impl TransferSummary {
    pub fn record(&mut self, result: Result<TransferOutcome, String>) {
        match result {
            Ok(TransferOutcome::Identical) => self.identical_skipped += 1,
            Ok(TransferOutcome::Copied { renamed, .. }) => {
                self.completed += 1;
                self.renamed += usize::from(renamed);
            }
            Err(error) => {
                self.failed += 1;
                self.errors.push(error);
            }
        }
    }
}

pub enum TransferOutcome {
    Identical,
    Copied {
        renamed: bool,
        files: Vec<CopiedFile>,
    },
}

pub struct CopiedFile {
    source: PathBuf,
    destination: PathBuf,
    stem_sidecar: bool,
}

// Keep shared metadata for every remaining same-stem file, including formats
// unknown to this build. Inaccessible directory entries also mean "retain".
fn has_remaining_sibling(source: &Path) -> bool {
    let Some(parent) = source.parent() else {
        return true;
    };
    let stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let Ok(entries) = fs::read_dir(parent) else {
        return true;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let path = entry.path();
        if path == source
            || path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                != stem
        {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            return true;
        };
        if !kind.is_file() && !kind.is_symlink() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !["xmp", "acr", "dop", "pp3", "rrdata", "rrhistory", "rrexif"]
            .contains(&extension.as_str())
        {
            return true;
        }
    }
    false
}

pub fn remove_import_sources(
    files: &[CopiedFile],
    mut remove: impl FnMut(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let _guard = TRANSFER_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    // Recheck the complete bundle before removing any member. Skipped bundles
    // never reach this function; the image is always the last member removed.
    for file in files {
        crate::file_management::ensure_card_writable(&file.source)?;
        if !identical(&file.source, &file.destination).map_err(|e| e.to_string())? {
            return Err(format!(
                "Source changed after copying; retained: {}",
                file.source.display()
            ));
        }
    }
    let retain_stem_sidecars = files
        .last()
        .is_none_or(|image| has_remaining_sibling(&image.source));
    for file in files {
        if file.stem_sidecar && retain_stem_sidecars {
            continue;
        }
        remove(&file.source)?;
    }
    Ok(())
}

// The suffix is retained separately so a renamed photo keeps all its metadata,
// including virtual copies. Stem-sidecars (e.g. A7C01234.xmp) follow its stem.
struct Member {
    source: PathBuf,
    suffix: OsString,
    stem_sidecar: bool,
}

fn members(source: &Path) -> Result<Vec<Member>, String> {
    let name = source.file_name().ok_or("Source has no filename")?;
    let mut result = Vec::new();
    let prefix = format!("{}.", name.to_string_lossy());
    for entry in
        fs::read_dir(source.parent().ok_or("Source has no parent")?).map_err(|e| e.to_string())?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() || path == source {
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lower = extension.to_ascii_lowercase();
        let image_sidecar = path.file_stem() == Some(name)
            && ["rrdata", "rrhistory", "rrexif", "xmp", "dop", "pp3"].contains(&lower.as_str());
        let stem_sidecar = !image_sidecar
            && path.file_stem() == source.file_stem()
            && ["xmp", "acr", "dop"].contains(&lower.as_str());
        if image_sidecar || stem_sidecar {
            let suffix = format!(".{extension}").into();
            result.push(Member {
                source: path,
                suffix,
                stem_sidecar,
            });
            continue;
        }
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        // Exact virtual copy ids only: `IMG.CR2.jpg.rrdata` belongs to IMG.CR2.jpg.
        if let Some(id) = filename.strip_prefix(&prefix).and_then(|s| {
            s.strip_suffix(".rrdata")
                .or_else(|| s.strip_suffix(".rrhistory"))
        }) && crate::file_management::is_virtual_copy_id(id)
        {
            let suffix = format!(".{id}.{extension}").into();
            result.push(Member {
                source: path,
                suffix,
                stem_sidecar: false,
            });
        }
    }
    result.sort_by(|a, b| a.source.cmp(&b.source));
    result.dedup_by(|a, b| a.source == b.source);
    // Publish the image last. An interrupted staging operation exposes no image.
    result.push(Member {
        source: source.into(),
        suffix: OsString::new(),
        stem_sidecar: false,
    });
    Ok(result)
}

fn target(member: &Member, image: &Path) -> PathBuf {
    let mut name = if member.stem_sidecar {
        image.file_stem()
    } else {
        image.file_name()
    }
    .unwrap_or_default()
    .to_os_string();
    name.push(&member.suffix);
    image.with_file_name(name)
}

fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub fn identical(source: &Path, destination: &Path) -> io::Result<bool> {
    let dest = match fs::symlink_metadata(destination) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if fs::metadata(source)?.len() != dest.len() {
        return Ok(false);
    }
    let hash = |path: &Path| -> io::Result<blake3::Hash> {
        let mut hasher = blake3::Hasher::new();
        hasher.update_reader(fs::File::open(path)?)?;
        Ok(hasher.finalize())
    };
    Ok(hash(source)? == hash(destination)?)
}

fn numbered(destination: &Path, index: usize, in_place: bool) -> PathBuf {
    let mut name = destination.file_stem().unwrap_or_default().to_os_string();
    name.push(if in_place {
        format!("_copy_{index}")
    } else {
        format!("-{index}")
    });
    if let Some(extension) = destination.extension() {
        name.push(".");
        name.push(extension);
    }
    destination.with_file_name(name)
}

pub fn transfer(
    source: &Path,
    destination: &Path,
    in_place: bool,
) -> Result<TransferOutcome, String> {
    transfer_with(source, destination, in_place, |from, to| {
        fs::copy(from, to).map(|_| ())
    })
}

fn transfer_with(
    source: &Path,
    destination: &Path,
    in_place: bool,
    copy: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<TransferOutcome, String> {
    transfer_with_publish_hook(source, destination, in_place, copy, |_| {})
}

fn transfer_with_publish_hook(
    source: &Path,
    destination: &Path,
    in_place: bool,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<()>,
    mut published_hook: impl FnMut(&Path),
) -> Result<TransferOutcome, String> {
    let _guard = TRANSFER_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let bundle = members(source)?;
    let mut candidate = destination.to_path_buf();
    let mut index = if in_place { 1 } else { 2 };
    if in_place {
        candidate = numbered(destination, index, true);
        index += 1;
    }
    // A concurrently-created destination is handled by the same no-clobber
    // retry as a collision found before staging. Never use exists()+fs::copy.
    loop {
        if !in_place
            && occupied(&candidate)
            && bundle
                .iter()
                .all(|m| identical(&m.source, &target(m, &candidate)).unwrap_or(false))
        {
            return Ok(TransferOutcome::Identical);
        }
        // Identical stem sidecars may be shared by a RAW/JPEG pair. A distinct
        // sidecar, including an orphan, reserves that basename.
        if bundle.iter().any(|m| {
            let dest = target(m, &candidate);
            occupied(&dest) && !(m.stem_sidecar && identical(&m.source, &dest).unwrap_or(false))
        }) {
            candidate = numbered(destination, index, in_place);
            index += 1;
            continue;
        }
        let attempt = (|| -> io::Result<Option<TransferOutcome>> {
            let mut staged = Vec::new();
            let mut files = Vec::new();
            for member in &bundle {
                let dest = target(member, &candidate);
                files.push(CopiedFile {
                    source: member.source.clone(),
                    destination: dest.clone(),
                    stem_sidecar: member.stem_sidecar,
                });
                if member.stem_sidecar && identical(&member.source, &dest)? {
                    continue;
                }
                let mut temp = tempfile::NamedTempFile::new_in(
                    dest.parent()
                        .ok_or_else(|| io::Error::other("Destination has no parent"))?,
                )?;
                copy(&member.source, temp.path())?;
                temp.as_file_mut().sync_all()?;
                if !identical(&member.source, temp.path())? {
                    return Err(io::Error::other(
                        "Source changed while copying; source retained",
                    ));
                }
                staged.push((temp, dest));
            }
            let mut published = Vec::new();
            for (temp, dest) in staged {
                if let Err(error) = temp.persist_noclobber(&dest) {
                    for path in published.iter().rev() {
                        fs::remove_file(path)?;
                    }
                    if error.error.kind() == io::ErrorKind::AlreadyExists {
                        return Ok(None);
                    }
                    return Err(error.error);
                }
                published.push(dest);
                published_hook(published.last().unwrap());
            }
            #[cfg(unix)]
            fs::File::open(candidate.parent().unwrap())?.sync_all()?;
            Ok(Some(TransferOutcome::Copied {
                renamed: candidate != destination,
                files,
            }))
        })();
        match attempt {
            Ok(Some(outcome)) => return Ok(outcome),
            Ok(None) => {
                candidate = numbered(destination, index, in_place);
                index += 1;
            }
            Err(e) => return Err(format!("{}: {e}", source.display())),
        }
    }
}

#[cfg(target_os = "android")]
pub fn transfer_bytes(bytes: &[u8], destination: &Path) -> Result<TransferOutcome, String> {
    let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = staging.path().join("source");
    fs::write(&source, bytes).map_err(|e| e.to_string())?;
    transfer(&source, destination, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_images_sidecar_is_not_a_virtual_copy() {
        let root = tempfile::tempdir().unwrap();
        let (src, dest) = (root.path().join("src"), root.path().join("dest"));
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dest).unwrap();
        // RapidRoom and Lightroom-import virtual copies travel; IMG.CR2.jpg's edits don't.
        let lightroom = "IMG.CR2.0f8fad5b-d9cb-469f-a165-70867728950e.rrdata";
        for name in [
            "IMG.CR2",
            "IMG.CR2.a1b2c3.rrdata",
            lightroom,
            "IMG.CR2.jpg",
            "IMG.CR2.jpg.rrdata",
        ] {
            fs::write(src.join(name), name).unwrap();
        }
        transfer(&src.join("IMG.CR2"), &dest.join("IMG.CR2"), false).unwrap();
        let mut copied: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        copied.sort();
        assert_eq!(copied, ["IMG.CR2", lightroom, "IMG.CR2.a1b2c3.rrdata"]);
    }

    // Two cards with recycled image numbers, edits and Lightroom sidecars. This
    // exercises disk contents, not an implementation-shaped mock of decisions.
    #[test]
    fn overlapping_cards_keep_images_and_paired_edits_and_skip_exact_reimports() {
        let root = tempfile::tempdir().unwrap();
        let (a, b, dest) = (
            root.path().join("card-a"),
            root.path().join("card-b"),
            root.path().join("import"),
        );
        for dir in [&a, &b, &dest] {
            fs::create_dir(dir).unwrap();
        }
        for (dir, bytes) in [(&a, b"first"), (&b, b"other")] {
            fs::write(dir.join("A7C01234.ARW"), bytes).unwrap();
            fs::write(dir.join("A7C01234.ARW.rrdata"), bytes).unwrap();
            fs::write(dir.join("A7C01234.ARW.a1b2c3.rrdata"), bytes).unwrap();
            fs::write(dir.join("A7C01234.ARW.rrhistory"), bytes).unwrap();
            fs::write(dir.join("A7C01234.ARW.a1b2c3.rrhistory"), bytes).unwrap();
            fs::write(dir.join("A7C01234.xmp"), bytes).unwrap();
        }
        let mut summary = TransferSummary::default();
        for card in [&a, &b, &a, &b] {
            summary.record(transfer(
                &card.join("A7C01234.ARW"),
                &dest.join("A7C01234.ARW"),
                false,
            ));
        }
        assert_eq!(
            (
                summary.completed,
                summary.renamed,
                summary.identical_skipped,
                summary.failed
            ),
            (2, 1, 2, 0)
        );
        for (stem, expected) in [("A7C01234", b"first"), ("A7C01234-2", b"other")] {
            for suffix in [
                ".ARW",
                ".ARW.rrdata",
                ".ARW.a1b2c3.rrdata",
                ".ARW.rrhistory",
                ".ARW.a1b2c3.rrhistory",
                ".xmp",
            ] {
                assert_eq!(
                    fs::read(dest.join(format!("{stem}{suffix}"))).unwrap(),
                    expected
                );
            }
        }
        // Same photo bytes with different edits must not discard those edits.
        fs::write(b.join("A7C01234.ARW.rrdata"), b"new edits").unwrap();
        summary.record(transfer(
            &b.join("A7C01234.ARW"),
            &dest.join("A7C01234.ARW"),
            false,
        ));
        assert_eq!(
            fs::read(dest.join("A7C01234-3.ARW.rrdata")).unwrap(),
            b"new edits"
        );
        assert_eq!(
            fs::read(dest.join("A7C01234-2.ARW.rrdata")).unwrap(),
            b"other"
        );
        assert_eq!((summary.completed, summary.renamed), (3, 2));
    }

    #[test]
    fn staging_disk_full_preserves_source_and_destination_and_next_transfer_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("card");
        let dest = root.path().join("dest");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(source.join("photo.ARW"), b"new photo").unwrap();
        fs::write(source.join("photo.ARW.rrdata"), b"new edits").unwrap();
        fs::write(dest.join("photo.ARW"), b"old photo").unwrap();
        let mut summary = TransferSummary::default();
        let mut calls = 0;
        summary.record(transfer_with(
            &source.join("photo.ARW"),
            &dest.join("photo.ARW"),
            false,
            |from, to| {
                calls += 1;
                if calls == 2 {
                    fs::write(to, b"partial")?;
                    return Err(io::Error::from_raw_os_error(28));
                }
                fs::copy(from, to).map(|_| ())
            },
        ));
        assert_eq!(summary.failed, 1);
        assert_eq!(fs::read(dest.join("photo.ARW")).unwrap(), b"old photo");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
        assert_eq!(fs::read(source.join("photo.ARW")).unwrap(), b"new photo");
        summary.record(transfer(
            &source.join("photo.ARW"),
            &dest.join("photo.ARW"),
            false,
        ));
        assert_eq!((summary.completed, summary.failed), (1, 1));
        assert_eq!(
            fs::read(dest.join("photo-2.ARW.rrdata")).unwrap(),
            b"new edits"
        );
    }

    #[test]
    fn concurrent_collision_rolls_back_only_own_sidecars_and_retries_without_overwriting() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("card");
        let dest = root.path().join("dest");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(source.join("photo.ARW"), b"ours").unwrap();
        fs::write(source.join("photo.ARW.rrdata"), b"our edits").unwrap();
        let mut injected = false;
        let outcome = transfer_with(
            &source.join("photo.ARW"),
            &dest.join("photo.ARW"),
            false,
            |from, to| {
                fs::copy(from, to)?;
                if !injected && from.extension().is_some_and(|e| e == "ARW") {
                    fs::write(dest.join("photo.ARW"), b"external writer")?;
                    injected = true;
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(
            outcome,
            TransferOutcome::Copied { renamed: true, .. }
        ));
        assert_eq!(
            fs::read(dest.join("photo.ARW")).unwrap(),
            b"external writer"
        );
        assert!(!dest.join("photo.ARW.rrdata").exists());
        assert_eq!(fs::read(dest.join("photo-2.ARW")).unwrap(), b"ours");
        assert_eq!(
            fs::read(dest.join("photo-2.ARW.rrdata")).unwrap(),
            b"our edits"
        );
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 3);
    }

    #[test]
    fn delete_after_import_keeps_metadata_for_unselected_or_later_failed_siblings() {
        let _mode = crate::file_management::card_mode_test_support::CardMode::off();
        for fail_later_sibling in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let card = root.path().join("card");
            let dest = root.path().join("dest");
            fs::create_dir(&card).unwrap();
            fs::create_dir(&dest).unwrap();
            fs::write(card.join("photo.ARW"), b"raw image").unwrap();
            fs::write(card.join("photo.JPG"), b"unselected JPEG").unwrap();
            for suffix in ["XMP", "acr", "dop"] {
                fs::write(card.join(format!("photo.{suffix}")), b"shared edits").unwrap();
            }
            fs::write(card.join("photo.ARW.rrdata"), b"RAW edits").unwrap();
            let TransferOutcome::Copied { files, .. } =
                transfer(&card.join("photo.ARW"), &dest.join("photo.ARW"), false).unwrap()
            else {
                panic!("Expected copied RAW");
            };
            remove_import_sources(&files, |path| {
                fs::remove_file(path).map_err(|e| e.to_string())
            })
            .unwrap();
            assert!(!card.join("photo.ARW").exists());
            assert!(!card.join("photo.ARW.rrdata").exists());
            assert_eq!(
                fs::read(card.join("photo.JPG")).unwrap(),
                b"unselected JPEG"
            );
            for suffix in ["XMP", "acr", "dop"] {
                assert_eq!(
                    fs::read(card.join(format!("photo.{suffix}"))).unwrap(),
                    b"shared edits"
                );
                assert_eq!(
                    fs::read(dest.join(format!("photo.{suffix}"))).unwrap(),
                    b"shared edits"
                );
            }
            if fail_later_sibling {
                let error = transfer_with(
                    &card.join("photo.JPG"),
                    &dest.join("photo.JPG"),
                    false,
                    |_, _| {
                        Err(io::Error::new(
                            io::ErrorKind::StorageFull,
                            "later JPEG import failed",
                        ))
                    },
                );
                assert!(error.is_err());
                assert_eq!(
                    fs::read(card.join("photo.JPG")).unwrap(),
                    b"unselected JPEG"
                );
                for suffix in ["XMP", "acr", "dop"] {
                    assert_eq!(
                        fs::read(card.join(format!("photo.{suffix}"))).unwrap(),
                        b"shared edits"
                    );
                }
                assert!(!dest.join("photo.JPG").exists());
            }
            // Once the final sibling actually succeeds, its own metadata cleanup
            // can remove the retained sidecars. Both destination images keep them.
            let TransferOutcome::Copied { files, .. } =
                transfer(&card.join("photo.JPG"), &dest.join("photo.JPG"), false).unwrap()
            else {
                panic!("Expected copied JPEG");
            };
            remove_import_sources(&files, |path| {
                fs::remove_file(path).map_err(|e| e.to_string())
            })
            .unwrap();
            assert_eq!(fs::read_dir(&card).unwrap().count(), 0);
            assert_eq!(
                fs::read(dest.join("photo.JPG")).unwrap(),
                b"unselected JPEG"
            );
            for suffix in ["XMP", "acr", "dop"] {
                assert_eq!(
                    fs::read(dest.join(format!("photo.{suffix}"))).unwrap(),
                    b"shared edits"
                );
            }
        }
    }

    #[test]
    fn concurrent_bundle_cannot_share_a_sidecar_before_its_owner_finishes_rollback() {
        use std::{sync::mpsc, thread, time::Duration};
        let root = tempfile::tempdir().unwrap();
        let card = root.path().join("card");
        let dest = root.path().join("dest");
        fs::create_dir(&card).unwrap();
        fs::create_dir(&dest).unwrap();
        fs::write(card.join("photo.ARW"), b"RAW image").unwrap();
        fs::write(card.join("photo.JPG"), b"JPEG image").unwrap();
        fs::write(card.join("photo.XMP"), b"shared XMP edits").unwrap();
        let (published_tx, published_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (b_started_tx, b_started_rx) = mpsc::channel();
        let (b_done_tx, b_done_rx) = mpsc::channel();
        thread::scope(|scope| {
            let card = &card;
            let dest = &dest;
            let a = scope.spawn(move || {
                let mut paused = false;
                transfer_with_publish_hook(
                    &card.join("photo.ARW"),
                    &dest.join("photo.ARW"),
                    false,
                    |from, to| fs::copy(from, to).map(|_| ()),
                    |path| {
                        if !paused && path == dest.join("photo.XMP") {
                            paused = true;
                            published_tx.send(()).unwrap();
                            resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                        }
                    },
                )
            });
            published_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                fs::read(dest.join("photo.XMP")).unwrap(),
                b"shared XMP edits"
            );
            let b = scope.spawn(move || {
                b_started_tx.send(()).unwrap();
                let result = transfer(&card.join("photo.JPG"), &dest.join("photo.JPG"), false);
                b_done_tx.send(()).unwrap();
                result
            });
            b_started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let premature_b = b_done_rx.recv_timeout(Duration::from_millis(200)).is_ok();
            // Force A's photo publication to fail after its XMP was published.
            fs::write(dest.join("photo.ARW"), b"external collision").unwrap();
            resume_tx.send(()).unwrap();
            let a = a.join().unwrap().unwrap();
            let b = b.join().unwrap().unwrap();
            assert!(
                !premature_b,
                "B shared metadata before A's publication/rollback finished"
            );
            assert!(matches!(a, TransferOutcome::Copied { renamed: true, .. }));
            assert!(matches!(b, TransferOutcome::Copied { renamed: false, .. }));
        });
        assert_eq!(
            fs::read(dest.join("photo.ARW")).unwrap(),
            b"external collision"
        );
        assert_eq!(fs::read(dest.join("photo-2.ARW")).unwrap(), b"RAW image");
        assert_eq!(
            fs::read(dest.join("photo-2.XMP")).unwrap(),
            b"shared XMP edits"
        );
        assert_eq!(fs::read(dest.join("photo.JPG")).unwrap(), b"JPEG image");
        assert_eq!(
            fs::read(dest.join("photo.XMP")).unwrap(),
            b"shared XMP edits"
        );
    }
}
