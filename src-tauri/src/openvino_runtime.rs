//! Optional, pre-staged Linux x86_64 CPU runtime. No downloads or model changes.
use anyhow::{Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};

const MANIFEST: &str = include_str!("../../rapidroom/openvino/runtime-manifest.json");

#[derive(Deserialize)]
struct Manifest {
    ort: String,
    binary_fingerprint: String,
    files: Vec<RuntimeFile>,
    aliases: BTreeMap<String, String>,
    notices: Vec<RuntimeFile>,
}
#[derive(Deserialize)]
struct RuntimeFile {
    file: String,
    bytes: u64,
    sha256: String,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct Runtime {
    core: PathBuf,
    fingerprint: String,
    _library: libloading::Library,
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();

fn filename(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name).file_name().is_some_and(|part| part == name)
        && name != "."
        && name != ".."
}

fn verify_at(root: &Path, manifest: &Manifest) -> Result<()> {
    let mut files = std::collections::HashSet::new();
    for file in &manifest.files {
        ensure!(
            filename(&file.file) && files.insert(&file.file),
            "Invalid runtime filename"
        );
        let path = root.join(&file.file);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && metadata.len() == file.bytes,
            "Invalid runtime file: {}",
            file.file
        );
        let mut reader = fs::File::open(path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        ensure!(
            hex::encode(digest.finalize()) == file.sha256,
            "Runtime checksum mismatch: {}",
            file.file
        );
    }
    for notice in &manifest.notices {
        ensure!(filename(&notice.file), "Invalid runtime notice filename");
        let path = root.join("notices").join(&notice.file);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && metadata.len() == notice.bytes,
            "Missing or invalid runtime notice"
        );
        let digest = Sha256::digest(fs::read(path)?);
        ensure!(
            hex::encode(digest) == notice.sha256,
            "Runtime notice checksum mismatch"
        );
    }
    for (alias, target) in &manifest.aliases {
        ensure!(
            filename(alias) && files.contains(target) && !files.contains(alias),
            "Invalid runtime alias"
        );
        ensure!(
            fs::read_link(root.join(alias))? == Path::new(target),
            "Runtime alias leaves its bundle"
        );
    }
    Ok(())
}

/// Preflight before touching ort's process-wide, irreversible library choice.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn load_at(root: &Path) -> Result<Runtime> {
    let manifest: Manifest = serde_json::from_str(MANIFEST)?;
    verify_at(root, &manifest)?;
    let core = root
        .join(format!("libonnxruntime.so.{}", manifest.ort))
        .canonicalize()?;
    // Only the embedded checksum-pinned core is loaded. Its $ORIGIN dependencies
    // were checked above; retain the handle for the process lifetime.
    let library = unsafe { libloading::Library::new(&core)? };
    unsafe {
        let entry: libloading::Symbol<unsafe extern "system" fn() -> *const ort::sys::OrtApiBase> =
            library.get(b"OrtGetApiBase\0")?;
        let base = entry();
        ensure!(!base.is_null(), "Runtime has no API base");
        ensure!(
            !(((*base).GetApi)(ort::MINOR_VERSION)).is_null(),
            "Runtime has no compatible C API"
        );
        let version = ((*base).GetVersionString)();
        ensure!(!version.is_null(), "Runtime has no version");
        ensure!(
            std::ffi::CStr::from_ptr(version).to_str()? == manifest.ort,
            "Unexpected runtime version"
        );
    }
    Ok(Runtime {
        core,
        fingerprint: manifest.binary_fingerprint,
        _library: library,
    })
}

/// Called once by app setup, before any model session is created.
pub(crate) fn configure(resources: &Path) -> Option<PathBuf> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let runtime = RUNTIME.get_or_init(|| {
            let root = resources.join("openvino-cpu/runtime");
            if !root.exists() {
                return None;
            }
            match load_at(&root) {
                Ok(runtime) => Some(runtime),
                Err(error) => {
                    log::warn!(
                        "Optional OpenVINO CPU runtime rejected; using bundled ORT CPU: {error:#}"
                    );
                    None
                }
            }
        });
        runtime.as_ref().map(|runtime| runtime.core.clone())
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = resources;
        None
    }
}

pub(crate) fn selected() -> bool {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        RUNTIME.get().is_some_and(Option::is_some)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        false
    }
}

pub(crate) fn fingerprint() -> Option<&'static str> {
    RUNTIME
        .get()
        .and_then(|runtime| runtime.as_ref())
        .map(|runtime| runtime.fingerprint.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_manifest_and_source_paths_are_closed() {
        let manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest.ort, "1.24.1");
        assert_eq!(manifest.files.len(), 8);
        assert!(
            manifest
                .files
                .iter()
                .all(|file| filename(&file.file) && file.sha256.len() == 64 && file.bytes > 0)
        );
        for name in ["", "..", ".", "../lib.so", "/lib.so", "nested/lib.so"] {
            assert!(!filename(name));
        }
    }

    #[test]
    fn runtime_verification_rejects_modified_files_and_alias_escapes() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.so"), b"expected").unwrap();
        let mut manifest = Manifest {
            ort: "1.24.1".into(),
            binary_fingerprint: "fixture".into(),
            files: vec![RuntimeFile {
                file: "lib.so".into(),
                bytes: 8,
                sha256: hex::encode(Sha256::digest(b"expected")),
            }],
            aliases: BTreeMap::new(),
            notices: Vec::new(),
        };
        verify_at(root.path(), &manifest).unwrap();
        manifest.files[0].file = "../lib.so".into();
        assert!(verify_at(root.path(), &manifest).is_err());
        manifest.files[0].file = "lib.so".into();
        fs::write(root.path().join("lib.so"), b"modified").unwrap();
        assert!(verify_at(root.path(), &manifest).is_err());
        fs::write(root.path().join("lib.so"), b"expected").unwrap();
        manifest
            .aliases
            .insert("../alias.so".into(), "lib.so".into());
        assert!(verify_at(root.path(), &manifest).is_err());
    }
}
