//! Publish encoded exports only after their bytes and file metadata are synced.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) fn write(path: &Path, bytes: &[u8], overwrite: bool) -> io::Result<()> {
    write_with(path, overwrite, |file| file.write_all(bytes))
}

fn write_with(
    path: &Path,
    overwrite: bool,
    encode: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    crate::file_management::ensure_card_writable(path)
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
    let existing = match fs::symlink_metadata(path) {
        Ok(_) if !overwrite => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Export already exists: {}", path.display()),
            ));
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut builder = tempfile::Builder::new();
    builder.prefix(".rapidroom-export-").suffix(".tmp");
    // Match ordinary file creation, including the user's umask. Existing regular
    // exports keep their permissions; replacing a symlink replaces the link.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut temp = builder.tempfile_in(parent)?;
    encode(temp.as_file_mut())?;
    if let Some(existing) = existing.filter(|m| m.is_file()) {
        fs::set_permissions(temp.path(), existing.permissions())?;
    }
    temp.as_file_mut().sync_all()?;
    if overwrite {
        temp.persist(path).map_err(|error| error.error)?;
    } else {
        temp.persist_noclobber(path).map_err(|error| error.error)?;
    }
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::card_mode_test_support::CardMode;
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn partial_write_failure_keeps_prior_export_or_no_export_and_cleans_staging_files() {
        let _mode = CardMode::off();
        for overwrite in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("export.tiff");
            if overwrite {
                fs::write(&target, b"prior complete TIFF").unwrap();
            }
            let error = write_with(&target, overwrite, |file| {
                file.write_all(b"partial TIFF")?;
                Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "injected full disk",
                ))
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::StorageFull);
            assert_eq!(
                fs::read(&target).ok(),
                overwrite.then(|| b"prior complete TIFF".to_vec())
            );
            assert_eq!(
                fs::read_dir(dir.path()).unwrap().count(),
                usize::from(overwrite)
            );
            write(&target, b"complete TIFF", overwrite).unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"complete TIFF");
        }
    }

    #[test]
    fn concurrent_destination_and_symlink_are_never_clobbered_without_overwrite() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("export.jpeg");
        let error = write_with(&target, false, |file| {
            file.write_all(b"our complete JPEG")?;
            fs::write(&target, b"concurrent complete JPEG")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&target).unwrap(), b"concurrent complete JPEG");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            write(&target, b"replacement", false).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let alias = dir.path().join("alias.jpeg");
            symlink(&target, &alias).unwrap();
            assert_eq!(
                write(&alias, b"replacement", false).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(fs::read(&target).unwrap(), b"concurrent complete JPEG");
            write(&alias, b"chosen replacement", true).unwrap();
            assert!(
                !fs::symlink_metadata(&alias)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read(&alias).unwrap(), b"chosen replacement");
            assert_eq!(fs::read(&target).unwrap(), b"concurrent complete JPEG");
        }
    }

    // This is a subprocess entry point, only called by the interruption test.
    #[test]
    #[ignore]
    fn interrupted_writer_child() {
        let dir = std::env::var_os("RAPIDROOM_EXPORT_FAULT_DIR")
            .expect("subprocess-only fixture directory");
        let dir = Path::new(&dir);
        let overwrite = std::env::var("RAPIDROOM_EXPORT_FAULT_OVERWRITE").unwrap() == "true";
        write_with(&dir.join("export.jpeg"), overwrite, |file| {
            file.write_all(&[0x5a; 65536])?;
            file.sync_all()?;
            fs::write(dir.join("writer-ready"), b"partial staged bytes synced")?;
            loop {
                std::thread::park();
            }
        })
        .unwrap();
    }

    #[test]
    fn killing_writer_during_staging_never_exposes_a_partial_final_export() {
        let _mode = CardMode::off();
        for overwrite in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("export.jpeg");
            if overwrite {
                fs::write(&target, b"prior complete JPEG").unwrap();
            }
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "atomic_export::tests::interrupted_writer_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("RAPIDROOM_EXPORT_FAULT_DIR", dir.path())
                .env("RAPIDROOM_EXPORT_FAULT_OVERWRITE", overwrite.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !dir.path().join("writer-ready").exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("Writer exited before barrier: {status}");
                }
                if Instant::now() > deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("Writer did not reach partial-write barrier");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            assert_eq!(
                fs::read(&target).ok(),
                overwrite.then(|| b"prior complete JPEG".to_vec())
            );
            // SIGKILL cannot run Drop. Verify the partial bytes have only a hidden
            // staging name; the owned test directory removes that orphan.
            let staged: Vec<_> = fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".rapidroom-export-")
                })
                .collect();
            assert_eq!(staged.len(), 1);
            assert_eq!(fs::metadata(staged[0].path()).unwrap().len(), 65536);
        }
    }
}
