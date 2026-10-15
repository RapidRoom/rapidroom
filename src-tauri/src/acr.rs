//! Read-only, bounded ACR object store shared by Lightroom companion importers.
//! Container rules adapted from skymanbp/autoshade@cbca12b5 (MIT).
//! Full notice: resources/licenses/autoshade-mask-semantics.txt.

use std::collections::HashMap;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_OBJECT_BYTES: usize = 16 * 1024 * 1024;
const MAX_READ_BYTES: usize = 64 * 1024 * 1024;
const MAX_READS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcrError {
    Unavailable,
    ContainerInvalid,
    ReferenceMismatch,
    DigestMismatch,
    SourceChanged,
    LimitExceeded,
}

impl std::fmt::Display for AcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "the companion .acr file is unavailable",
            Self::ContainerInvalid => "the companion .acr container is invalid or unsupported",
            Self::ReferenceMismatch => "the brush reference is missing or ambiguous",
            Self::DigestMismatch => "the referenced companion object failed its content check",
            Self::SourceChanged => "the companion changed while it was being read",
            Self::LimitExceeded => "the companion exceeds the import safety limits",
        })
    }
}

#[derive(Debug)]
struct Entry {
    offset: u64,
    len: u64,
}

/// One open file per conversion; no global/path-only cache and no reopening
/// between directory validation and object reads.
#[derive(Debug)]
pub(crate) struct AcrFile {
    file: File,
    entries: HashMap<[u8; 16], Entry>,
    len: u64,
    modified: Option<SystemTime>,
    bytes_read: usize,
    reads: usize,
}

impl AcrFile {
    pub(crate) fn open(path: &Path) -> Result<Self, AcrError> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Opening an attacker-supplied FIFO must not block the importer.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = options.open(path).map_err(|_| AcrError::Unavailable)?;
        let metadata = file.metadata().map_err(|_| AcrError::Unavailable)?;
        if !metadata.is_file() {
            return Err(AcrError::ContainerInvalid);
        }
        let len = metadata.len();
        if len > MAX_FILE_BYTES {
            return Err(AcrError::LimitExceeded);
        }
        let mut header = [0u8; 20];
        file.read_exact(&mut header)
            .map_err(|_| AcrError::ContainerInvalid)?;
        // Only the measured ACR/1/ARW store is supported. Other camera
        // signatures can be added with evidence by future companion readers.
        if &header[..4] != b"ACR\0"
            || u32_at(&header, 4) != 1
            || &header[8..12] != b"ARW\0"
            || u32_at(&header, 16) != 0
        {
            return Err(AcrError::ContainerInvalid);
        }
        let count = u32_at(&header, 12) as usize;
        if count > MAX_ENTRIES {
            return Err(AcrError::LimitExceeded);
        }
        let directory_end = 20 + count as u64 * 32;
        if directory_end > len {
            return Err(AcrError::ContainerInvalid);
        }
        let mut directory = vec![0; count * 32];
        file.read_exact(&mut directory)
            .map_err(|_| AcrError::ContainerInvalid)?;
        let mut entries = HashMap::with_capacity(count);
        let mut ranges = Vec::with_capacity(count);
        for record in directory.as_chunks::<32>().0 {
            let key: [u8; 16] = record[..16].try_into().expect("fixed directory key");
            let object_len = u64_at(record, 16);
            let offset = u64_at(record, 24);
            let end = offset
                .checked_add(object_len)
                .ok_or(AcrError::ContainerInvalid)?;
            if object_len == 0 || offset < directory_end || end > len {
                return Err(AcrError::ContainerInvalid);
            }
            if entries
                .insert(
                    key,
                    Entry {
                        offset,
                        len: object_len,
                    },
                )
                .is_some()
            {
                return Err(AcrError::ReferenceMismatch);
            }
            ranges.push((offset, end));
        }
        ranges.sort_unstable();
        let mut cursor = directory_end;
        for (start, end) in ranges {
            let padding = (4 - cursor % 4) % 4;
            if start != cursor + padding {
                return Err(AcrError::ContainerInvalid);
            }
            check_padding(&mut file, cursor, padding)?;
            cursor = end;
        }
        let padding = (4 - cursor % 4) % 4;
        if len != cursor + padding {
            return Err(AcrError::ContainerInvalid);
        }
        check_padding(&mut file, cursor, padding)?;
        let store = Self {
            file,
            entries,
            len,
            modified: metadata.modified().ok(),
            bytes_read: 0,
            reads: 0,
        };
        store.check_unchanged()?;
        Ok(store)
    }

    /// MD5 is the file format's content address, never a security signature.
    pub(crate) fn read_object(&mut self, reference: &str) -> Result<Vec<u8>, AcrError> {
        self.reads += 1;
        if self.reads > MAX_READS {
            return Err(AcrError::LimitExceeded);
        }
        let key = parse_key(reference)?;
        self.check_unchanged()?;
        let entry = self.entries.get(&key).ok_or(AcrError::ReferenceMismatch)?;
        let object_len = usize::try_from(entry.len).map_err(|_| AcrError::LimitExceeded)?;
        if object_len > MAX_OBJECT_BYTES
            || self.bytes_read.saturating_add(object_len) > MAX_READ_BYTES
        {
            return Err(AcrError::LimitExceeded);
        }
        self.bytes_read += object_len;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(object_len)
            .map_err(|_| AcrError::LimitExceeded)?;
        bytes.resize(object_len, 0);
        self.file
            .seek(SeekFrom::Start(entry.offset))
            .map_err(|_| AcrError::ContainerInvalid)?;
        self.file
            .read_exact(&mut bytes)
            .map_err(|_| AcrError::ContainerInvalid)?;
        self.check_unchanged()?;
        if md5::compute(&bytes).0 != key {
            return Err(AcrError::DigestMismatch);
        }
        Ok(bytes)
    }

    fn check_unchanged(&self) -> Result<(), AcrError> {
        let current: Metadata = self.file.metadata().map_err(|_| AcrError::SourceChanged)?;
        if current.len() != self.len || current.modified().ok() != self.modified {
            return Err(AcrError::SourceChanged);
        }
        Ok(())
    }
}

fn check_padding(file: &mut File, offset: u64, len: u64) -> Result<(), AcrError> {
    let mut padding = [0u8; 3];
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| AcrError::ContainerInvalid)?;
    let bytes = &mut padding[..len as usize];
    file.read_exact(bytes)
        .map_err(|_| AcrError::ContainerInvalid)?;
    if bytes.iter().any(|&b| b != 0) {
        return Err(AcrError::ContainerInvalid);
    }
    Ok(())
}

pub(crate) fn parse_key(reference: &str) -> Result<[u8; 16], AcrError> {
    if reference.len() != 32 || !reference.is_ascii() {
        return Err(AcrError::ReferenceMismatch);
    }
    let mut key = [0u8; 16];
    for (i, pair) in reference.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let hex = |b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        let (Some(a), Some(b)) = (hex(pair[0]), hex(pair[1])) else {
            return Err(AcrError::ReferenceMismatch);
        };
        key[i] = a * 16 + b;
    }
    Ok(key)
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated fixed-width field"),
    )
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated fixed-width field"),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    pub(crate) fn container(objects: &[Vec<u8>]) -> (Vec<u8>, Vec<String>) {
        let mut bytes = b"ACR\0".to_vec();
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(b"ARW\0");
        bytes.extend((objects.len() as u32).to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.resize(20 + objects.len() * 32, 0);
        let mut refs = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            while !bytes.len().is_multiple_of(4) {
                bytes.push(0);
            }
            let offset = bytes.len();
            let at = 20 + i * 32;
            let key = md5::compute(object);
            bytes[at..at + 16].copy_from_slice(&key.0);
            bytes[at + 16..at + 24].copy_from_slice(&(object.len() as u64).to_le_bytes());
            bytes[at + 24..at + 32].copy_from_slice(&(offset as u64).to_le_bytes());
            bytes.extend(object);
            refs.push(format!("{key:x}"));
        }
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        (bytes, refs)
    }

    #[test]
    fn reads_selected_objects_and_checks_digest_and_source_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.acr");
        let (bytes, refs) = container(&[vec![1, 2, 3], vec![9, 8, 7, 6]]);
        fs::write(&path, &bytes).unwrap();
        let mut reader = AcrFile::open(&path).unwrap();
        assert_eq!(
            reader.read_object(&refs[1].to_uppercase()).unwrap(),
            vec![9, 8, 7, 6]
        );
        assert_eq!(reader.read_object(&refs[0]).unwrap(), vec![1, 2, 3]);
        let mut corrupt = bytes.clone();
        corrupt[84] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert_eq!(reader.read_object(&refs[0]), Err(AcrError::SourceChanged));
        assert_eq!(
            AcrFile::open(&path).unwrap().read_object(&refs[0]),
            Err(AcrError::DigestMismatch)
        );
    }

    #[test]
    fn rejects_truncated_duplicate_overlapping_and_unbounded_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.acr");
        let (valid, _) = container(&[vec![1, 2, 3], vec![4, 5, 6]]);
        for n in 0..20 {
            fs::write(&path, &valid[..n]).unwrap();
            assert!(AcrFile::open(&path).is_err());
        }
        let mut mutations = Vec::new();
        let mut x = valid.clone();
        x[12..16].copy_from_slice(&4097u32.to_le_bytes());
        mutations.push(x);
        let mut x = valid.clone();
        x[52..68].copy_from_slice(&valid[20..36]);
        mutations.push(x);
        let mut x = valid.clone();
        x[76..84].copy_from_slice(&84u64.to_le_bytes());
        mutations.push(x);
        let mut x = valid.clone();
        x[44..52].copy_from_slice(&u64::MAX.to_le_bytes());
        mutations.push(x);
        let mut x = valid.clone();
        x[87] = 1;
        mutations.push(x);
        let mut x = valid.clone();
        x.push(0);
        mutations.push(x);
        for x in mutations {
            fs::write(&path, x).unwrap();
            assert!(AcrFile::open(&path).is_err());
        }
        let file = File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert_eq!(AcrFile::open(&path).unwrap_err(), AcrError::LimitExceeded);
    }

    #[test]
    fn bounds_object_and_conversion_read_budgets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.acr");
        let (bytes, refs) = container(&[vec![1]]);
        fs::write(&path, bytes).unwrap();
        let mut r = AcrFile::open(&path).unwrap();
        r.bytes_read = MAX_READ_BYTES;
        assert_eq!(r.read_object(&refs[0]), Err(AcrError::LimitExceeded));
        r.bytes_read = 0;
        r.reads = MAX_READS;
        assert_eq!(r.read_object(&refs[0]), Err(AcrError::LimitExceeded));
        assert!(parse_key("../other").is_err());
        assert!(parse_key(&"z".repeat(32)).is_err());
    }
}
