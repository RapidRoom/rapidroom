use std::path::Path;

// Video containers include Sony XAVC MP4/MXF; this list does not add import support.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mxf", "mts", "m2ts", "avi", "mkv", "webm", "mpg", "mpeg", "3gp",
];

fn count_videos(path: &Path, recursive: bool) -> Result<usize, String> {
    if !path.is_dir() {
        return Err("Video count requires a directory".into());
    }
    let mut count = 0;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_depth(if recursive { usize::MAX } else { 1 })
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let is_file = entry.file_type().is_file()
            || (entry.file_type().is_symlink() && entry.path().is_file());
        if is_file
            && entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    VIDEO_EXTENSIONS
                        .iter()
                        .any(|video| extension.eq_ignore_ascii_case(video))
                })
        {
            count += 1;
        }
    }
    Ok(count)
}

#[tauri::command]
pub async fn count_unshown_videos(path: String, recursive: bool) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || count_videos(Path::new(&path), recursive))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_camera_containers_without_sidecars_directories_or_recursive_symlink_loops() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::create_dir_all(root.join("PRIVATE/M4ROOT/CLIP")).unwrap();
        std::fs::create_dir(root.join("not-a-video.MP4")).unwrap();
        for file in ["C0001.MP4", "C0001M01.XML", "C0002.mxf", "photo.ARW"] {
            std::fs::write(root.join(file), file).unwrap();
        }
        let clip = root.join("PRIVATE/M4ROOT/CLIP/C0003.Mp4");
        std::fs::write(&clip, "owned video fixture").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root, root.join("PRIVATE/loop")).unwrap();
        assert_eq!(count_videos(root, false).unwrap(), 2);
        assert_eq!(count_videos(root, true).unwrap(), 3);
        assert_eq!(
            std::fs::read_to_string(clip).unwrap(),
            "owned video fixture"
        );
        assert!(count_videos(&root.join("missing"), true).is_err());
    }
}
