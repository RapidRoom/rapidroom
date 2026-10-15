//! Opt-in culling metadata edits. Image markers, scans and trailers are copied verbatim.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    ops::Range,
    path::Path,
    sync::Mutex,
};

const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const EXTENDED: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const MAX_FILE: u64 = 256 * 1024 * 1024;
static WRITE_LOCK: Mutex<()> = Mutex::new(());
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    xmp: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
}
#[derive(Debug)]
pub(crate) struct Change {
    pub before: Snapshot,
    pub after_hash: blake3::Hash,
}
struct Segment {
    marker: u8,
    range: Range<usize>,
    data: Range<usize>,
}
struct Jpeg {
    segments: Vec<Segment>,
    insert: usize,
    auxiliary: bool,
}

fn range(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(offset..offset.checked_add(len).ok_or("JPEG range overflow")?)
        .ok_or_else(|| "Truncated JPEG metadata".into())
}
fn be16(bytes: &[u8], offset: usize) -> Result<usize> {
    Ok(u16::from_be_bytes(range(bytes, offset, 2)?.try_into().unwrap()) as usize)
}
fn parse(bytes: &[u8]) -> Result<Jpeg> {
    if bytes.get(..2) != Some(&[255, 216]) {
        return Err("Not a JPEG file".into());
    }
    let mut pos = 2;
    let mut segments = Vec::new();
    let mut insert = 2;
    let mut leading = true;
    let mut frame = false;
    let mut scan = false;
    let mut entropy = false;
    let mut auxiliary = false;
    loop {
        if entropy {
            loop {
                if *bytes.get(pos).ok_or("Truncated JPEG scan")? != 255 {
                    pos += 1;
                    continue;
                }
                let start = pos;
                while bytes.get(pos) == Some(&255) {
                    pos += 1;
                }
                let code = *bytes.get(pos).ok_or("Truncated JPEG scan marker")?;
                if code == 0 || (0xd0..=0xd7).contains(&code) {
                    pos += 1;
                    continue;
                }
                pos = start;
                break;
            }
            entropy = false;
        }
        let start = pos;
        if bytes.get(pos) != Some(&255) {
            return Err("Malformed JPEG marker".into());
        }
        while bytes.get(pos) == Some(&255) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or("Truncated JPEG marker")?;
        pos += 1;
        if marker == 0xd9 {
            if !frame || !scan {
                return Err("JPEG has no image scan".into());
            }
            return Ok(Jpeg {
                segments,
                insert,
                auxiliary: auxiliary || pos < bytes.len(),
            });
        }
        if marker == 0 || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            return Err("Unexpected JPEG marker".into());
        }
        if marker == 1 {
            continue;
        }
        if segments.len() >= 1024 {
            return Err("Too many JPEG segments".into());
        }
        let len = be16(bytes, pos)?;
        if len < 2 {
            return Err("Invalid JPEG segment length".into());
        }
        range(bytes, pos, len)?;
        let data = pos + 2..pos + len;
        pos += len;
        if marker == 0xe1 && bytes[data.clone()].starts_with(EXTENDED) {
            return Err("Extended XMP is unsupported".into());
        }
        if marker == 0xe2 && bytes[data.clone()].starts_with(b"MPF\0") || marker == 0xeb {
            auxiliary = true;
        }
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            if frame {
                return Err("Multiple JPEG frames are unsupported".into());
            }
            let n = *bytes.get(data.start + 5).ok_or("Truncated JPEG frame")? as usize;
            if n == 0
                || n > 4
                || data.len() != 6 + 3 * n
                || be16(bytes, data.start + 1)? == 0
                || be16(bytes, data.start + 3)? == 0
            {
                return Err("Invalid JPEG frame".into());
            }
            frame = true;
        }
        if marker == 0xda {
            let n = *bytes.get(data.start).ok_or("Truncated JPEG scan header")? as usize;
            if !frame || n == 0 || n > 4 || data.len() != 4 + 2 * n {
                return Err("Invalid JPEG scan header".into());
            }
            scan = true;
            entropy = true;
        }
        if leading
            && (marker == 0xe0 || marker == 0xe1 && bytes[data.clone()].starts_with(b"Exif\0\0"))
        {
            insert = pos;
        } else {
            leading = false;
        }
        segments.push(Segment {
            marker,
            range: start..pos,
            data,
        });
    }
}
fn targets<'a>(jpeg: &'a Jpeg, bytes: &[u8], prefix: &[u8]) -> Result<Option<&'a Segment>> {
    let mut items = jpeg
        .segments
        .iter()
        .filter(|s| s.marker == 0xe1 && bytes[s.data.clone()].starts_with(prefix));
    let first = items.next();
    if items.next().is_some() {
        return Err("Multiple XMP or EXIF packets are unsupported".into());
    }
    Ok(first)
}
fn snapshot(jpeg: &Jpeg, bytes: &[u8]) -> Result<Snapshot> {
    Ok(Snapshot {
        xmp: targets(jpeg, bytes, XMP)?.map(|s| bytes[s.range.clone()].to_vec()),
        exif: targets(jpeg, bytes, b"Exif\0\0")?.map(|s| bytes[s.range.clone()].to_vec()),
    })
}
fn segment(data: &[u8]) -> Result<Vec<u8>> {
    let len = u16::try_from(data.len() + 2).map_err(|_| "XMP exceeds the JPEG APP1 limit")?;
    let mut out = vec![255, 0xe1];
    out.extend(len.to_be_bytes());
    out.extend(data);
    Ok(out)
}
fn padded_packet(packet: &str, len: usize) -> Option<String> {
    let end = packet.find("<?xpacket end=").unwrap_or(packet.len());
    let front = packet[..end].trim_end_matches([' ', '\t', '\r', '\n']);
    let tail = &packet[end..];
    let pad = len.checked_sub(front.len() + tail.len())?;
    Some(format!("{front}{}{tail}", " ".repeat(pad)))
}
fn xmp_segment(
    bytes: &[u8],
    old: Option<&Segment>,
    rating: i16,
    label: Option<&str>,
) -> Result<Vec<u8>> {
    if !(-1..=5).contains(&rating) {
        return Err("Rating must be -1 through 5".into());
    }
    let content = if let Some(s) = old {
        std::str::from_utf8(&bytes[s.data.start + XMP.len()..s.data.end])
            .map_err(|_| "XMP is not UTF-8")?
    } else {
        crate::xmp::SKELETON
    };
    let props = crate::xmp::photo_properties(content, "http://ns.adobe.com/xmp/note/")
        .map_err(|e| e.to_string())?;
    if props.contains_key("HasExtendedXMP") {
        return Err("Extended XMP is unsupported".into());
    }
    let mut packet =
        crate::xmp::update_rating_label(content, Some(rating), label).map_err(|e| e.to_string())?;
    if let Some(s) = old
        && let Some(padded) = padded_packet(&packet, s.data.len() - XMP.len())
    {
        packet = padded;
    }
    let mut data = XMP.to_vec();
    data.extend(packet.as_bytes());
    segment(&data)
}
fn tiff_u16(t: &[u8], p: usize, little: bool) -> Result<u16> {
    let a = range(t, p, 2)?.try_into().unwrap();
    Ok(if little {
        u16::from_le_bytes(a)
    } else {
        u16::from_be_bytes(a)
    })
}
fn tiff_u32(t: &[u8], p: usize, little: bool) -> Result<u32> {
    let a = range(t, p, 4)?.try_into().unwrap();
    Ok(if little {
        u32::from_le_bytes(a)
    } else {
        u32::from_be_bytes(a)
    })
}
fn patch_exif(segment: &mut [u8], start: usize, rating: i16) -> Result<()> {
    let t = segment.get_mut(start..).ok_or("Truncated EXIF header")?;
    let little = match range(t, 0, 2)? {
        b"II" => true,
        b"MM" => false,
        _ => return Err("Unsupported EXIF byte order".into()),
    };
    if tiff_u16(t, 2, little)? != 42 {
        return Err("Unsupported EXIF TIFF header".into());
    }
    let mut offsets = vec![tiff_u32(t, 4, little)? as usize];
    let mut visited = Vec::new();
    while let Some(off) = offsets.pop() {
        if off == 0 {
            continue;
        }
        if off < 8 || visited.contains(&off) || visited.len() >= 8 {
            return Err("Invalid or excessive EXIF IFDs".into());
        }
        visited.push(off);
        let count = tiff_u16(t, off, little)? as usize;
        if count > 1024 {
            return Err("Too many EXIF fields".into());
        }
        range(t, off + 2, count * 12 + 4)?;
        for i in 0..count {
            let e = off + 2 + i * 12;
            let tag = tiff_u16(t, e, little)?;
            if matches!(tag, 0x8769 | 0x8825 | 0xa005) {
                if !matches!(tiff_u16(t, e + 2, little)?, 4 | 13)
                    || tiff_u32(t, e + 4, little)? != 1
                {
                    return Err("Invalid EXIF IFD pointer".into());
                }
                offsets.push(tiff_u32(t, e + 8, little)? as usize);
            }
            if !matches!(tag, 0x4746 | 0x4749) {
                continue;
            }
            let kind = tiff_u16(t, e + 2, little)?;
            if !matches!(kind, 3 | 4) || tiff_u32(t, e + 4, little)? != 1 {
                return Err("Unsupported EXIF rating field".into());
            }
            let stars = rating.max(0) as usize;
            let value = if tag == 0x4749 {
                [0, 1, 25, 50, 75, 99][stars]
            } else {
                stars as u32
            };
            if kind == 3 {
                let v = value as u16;
                t[e + 8..e + 10].copy_from_slice(&if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                });
            } else {
                t[e + 8..e + 12].copy_from_slice(&if little {
                    value.to_le_bytes()
                } else {
                    value.to_be_bytes()
                });
            }
        }
        offsets.push(tiff_u32(t, off + 2 + count * 12, little)? as usize);
    }
    Ok(())
}
fn replace(bytes: &[u8], jpeg: &Jpeg, desired: &Snapshot) -> Result<Vec<u8>> {
    let old_xmp = targets(jpeg, bytes, XMP)?;
    let old_exif = targets(jpeg, bytes, b"Exif\0\0")?;
    let mut edits: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
    match (old_xmp, &desired.xmp) {
        (Some(s), v) => edits.push((s.range.clone(), v.clone().unwrap_or_default())),
        (None, Some(v)) => edits.push((jpeg.insert..jpeg.insert, v.clone())),
        _ => {}
    }
    if let Some(s) = old_exif {
        edits.push((
            s.range.clone(),
            desired.exif.clone().ok_or("EXIF packet disappeared")?,
        ));
    } else if desired.exif.is_some() {
        return Err("EXIF packet changed before Undo".into());
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = bytes.to_vec();
    for (r, v) in edits {
        out.splice(r, v);
    }
    if out.len() as u64 > MAX_FILE {
        return Err("Rewritten JPEG exceeds 256 MiB".into());
    }
    if jpeg.auxiliary && out.len() != bytes.len() {
        return Err(
            "Insufficient XMP padding: resizing would move auxiliary images or trailing data"
                .into(),
        );
    }
    let after = parse(&out)?;
    if let Some(old) = old_exif
        && targets(&after, &out, b"Exif\0\0")?.is_none_or(|new| new.range.start != old.range.start)
    {
        return Err("Insufficient XMP padding: resizing would move the EXIF block".into());
    }
    if protected_bytes(bytes, jpeg) != protected_bytes(&out, &after) {
        return Err("Refused rewrite outside JPEG APP1 metadata".into());
    }
    Ok(out)
}
fn protected_bytes(bytes: &[u8], jpeg: &Jpeg) -> blake3::Hash {
    let mut hash = blake3::Hasher::new();
    let mut p = 0;
    for s in &jpeg.segments {
        if s.marker == 0xe1 {
            hash.update(&bytes[p..s.range.start]);
            p = s.range.end;
        }
    }
    hash.update(&bytes[p..]);
    hash.finalize()
}
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn copy_attributes(source: &File, target: &File) -> Result<()> {
    use std::{ffi::CString, os::fd::AsRawFd};
    fn names(file: &File) -> Result<Vec<CString>> {
        use std::os::fd::AsRawFd;
        let fd = file.as_raw_fd();
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let size = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0) };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let size = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0, 0) };
        if size < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ENOTSUP) {
                return Ok(Vec::new());
            }
            return Err(format!("Cannot preserve JPEG attributes: {error}"));
        }
        if size > 65536 {
            return Err("JPEG filesystem attributes exceed limits".into());
        }
        let mut buffer = vec![0u8; size as usize];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let actual = unsafe { libc::flistxattr(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let actual = unsafe { libc::flistxattr(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0) };
        if actual != size {
            return Err("JPEG filesystem attributes changed while reading".into());
        }
        buffer
            .split(|v| *v == 0)
            .filter(|name| !name.is_empty())
            .map(|name| CString::new(name).map_err(|e| e.to_string()))
            .collect()
    }
    let source_names = names(source)?;
    if source_names.len() > 128 {
        return Err("Too many JPEG filesystem attributes".into());
    }
    for name in names(target)? {
        if source_names.contains(&name) {
            continue;
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let result = unsafe { libc::fremovexattr(target.as_raw_fd(), name.as_ptr()) };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let result = unsafe { libc::fremovexattr(target.as_raw_fd(), name.as_ptr(), 0) };
        if result != 0 {
            return Err(format!(
                "Cannot preserve JPEG attributes: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    let mut total = 0usize;
    for name in source_names {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let size =
            unsafe { libc::fgetxattr(source.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let size = unsafe {
            libc::fgetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                0,
            )
        };
        if size < 0 || size > 1024 * 1024 {
            return Err("Cannot read bounded JPEG filesystem attribute".into());
        }
        total += size as usize;
        if total > 1024 * 1024 {
            return Err("JPEG filesystem attributes exceed 1 MiB".into());
        }
        let mut value = vec![0u8; size as usize];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let actual = unsafe {
            libc::fgetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let actual = unsafe {
            libc::fgetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if actual != size {
            return Err("JPEG filesystem attributes changed while reading".into());
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let result = unsafe {
            libc::fsetxattr(
                target.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let result = unsafe {
            libc::fsetxattr(
                target.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if result != 0 {
            return Err(format!(
                "Cannot preserve JPEG attributes: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev()
            || a.ino() != b.ino()
            || a.ctime() != b.ctime()
            || a.ctime_nsec() != b.ctime_nsec()
        {
            return false;
        }
    }
    a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && a.permissions() == b.permissions()
}
fn edit(
    path: &Path,
    desired: impl FnOnce(&[u8], &Jpeg, &Snapshot) -> Result<Snapshot>,
    expected: Option<blake3::Hash>,
) -> Result<Change> {
    crate::file_management::ensure_card_writable(path)?;
    if !path
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("jpg") || v.eq_ignore_ascii_case("jpeg"))
    {
        return Err("Only original JPEG files are supported".into());
    }
    let _guard = WRITE_LOCK.lock().map_err(|_| "JPEG writer unavailable")?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE || meta.permissions().readonly() {
        return Err("JPEG must be a writable regular file of at most 256 MiB".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err("Hard-linked JPEG files are unsupported".into());
        }
    }
    let mut opts = OpenOptions::new();
    opts.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut original = opts.open(path).map_err(|e| e.to_string())?;
    original
        .try_lock()
        .map_err(|e| format!("JPEG is in use: {e}"))?;
    if !same_file(&meta, &original.metadata().map_err(|e| e.to_string())?) {
        return Err("JPEG changed while opening".into());
    }
    let mut bytes = Vec::new();
    (&mut original)
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != meta.len() {
        return Err("JPEG changed while reading".into());
    }
    if expected.is_some_and(|h| blake3::hash(&bytes) != h) {
        return Err("JPEG changed since this metadata write; Undo refused".into());
    }
    let jpeg = parse(&bytes)?;
    let before = snapshot(&jpeg, &bytes)?;
    let next = desired(&bytes, &jpeg, &before)?;
    let out = replace(&bytes, &jpeg, &next)?;
    if out != bytes {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temp.write_all(&out).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
            if unsafe { libc::fchown(temp.as_file().as_raw_fd(), meta.uid(), meta.gid()) } != 0 {
                return Err(format!(
                    "Cannot preserve JPEG ownership: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        temp.as_file()
            .set_permissions(meta.permissions())
            .map_err(|e| e.to_string())?;
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        ))]
        copy_attributes(&original, temp.as_file())?;
        filetime::set_file_handle_times(
            temp.as_file(),
            Some(filetime::FileTime::from_last_access_time(&meta)),
            Some(filetime::FileTime::from_last_modification_time(&meta)),
        )
        .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        original
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let mut hash = blake3::Hasher::new();
        std::io::copy(&mut (&mut original).take(MAX_FILE + 1), &mut hash)
            .map_err(|e| e.to_string())?;
        if hash.finalize() != blake3::hash(&bytes)
            || !same_file(
                &meta,
                &fs::symlink_metadata(path).map_err(|e| e.to_string())?,
            )
        {
            return Err("JPEG changed before replacement".into());
        }
        temp.persist(path).map_err(|e| e.error.to_string())?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(|e| format!("JPEG replaced but directory sync failed: {e}"))?;
    }
    Ok(Change {
        before,
        after_hash: blake3::hash(&out),
    })
}
pub(crate) fn write(path: &Path, rating: i16, label: Option<&str>) -> Result<Change> {
    edit(
        path,
        |bytes, jpeg, before| {
            let mut next = before.clone();
            next.xmp = Some(xmp_segment(
                bytes,
                targets(jpeg, bytes, XMP)?,
                rating,
                label,
            )?);
            if let Some(exif) = &mut next.exif {
                let s = targets(jpeg, bytes, b"Exif\0\0")?.unwrap();
                patch_exif(exif, s.data.start - s.range.start + 6, rating)?;
            }
            Ok(next)
        },
        None,
    )
}
pub(crate) fn restore(path: &Path, before: &Snapshot, expected: blake3::Hash) -> Result<()> {
    edit(path, |_, _, _| Ok(before.clone()), Some(expected)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn jpeg() -> Vec<u8> {
        let pixels: Vec<u8> = (0..8 * 8 * 3).map(|n| (n * 41) as u8).collect();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode(&pixels, 8, 8, image::ExtendedColorType::Rgb8)
            .unwrap();
        bytes
    }
    fn with_xmp(mut bytes: Vec<u8>, packet: &str) -> Vec<u8> {
        let mut data = XMP.to_vec();
        data.extend(packet.as_bytes());
        bytes.splice(2..2, segment(&data).unwrap());
        bytes
    }
    fn packet() -> String {
        format!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"original toolkit\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:z=\"http://ns.adobe.com/xap/1.0/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:f=\"urn:foreign\" z:Rating=\"2\" f:rating=\"untouched\"><z:Label>Blue</z:Label><dc:subject><rdf:Bag><rdf:li>café &amp; R&amp;D</rdf:li></rdf:Bag></dc:subject><f:data>keep this</f:data></rdf:Description></rdf:RDF></x:xmpmeta>{}",
            " ".repeat(1024)
        )
    }
    fn file(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.JPG");
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }
    fn xmp(bytes: &[u8]) -> String {
        let j = parse(bytes).unwrap();
        let s = targets(&j, bytes, XMP).unwrap().unwrap();
        String::from_utf8(bytes[s.data.start + XMP.len()..s.data.end].to_vec()).unwrap()
    }
    fn exif(little: bool) -> Vec<u8> {
        let mut t = if little {
            b"II".to_vec()
        } else {
            b"MM".to_vec()
        };
        let u16b = |v: u16| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let u32b = |v: u32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        t.extend(u16b(42));
        t.extend(u32b(8));
        t.extend(u16b(2));
        for (tag, value) in [(0x4746, 2), (0x4749, 25)] {
            t.extend(u16b(tag));
            t.extend(u16b(3));
            t.extend(u32b(1));
            t.extend(u16b(value));
            t.extend([0, 0]);
        }
        t.extend([0, 0, 0, 0]);
        let mut data = b"Exif\0\0".to_vec();
        data.extend(t);
        segment(&data).unwrap()
    }
    #[test]
    fn inserts_ratings_without_reencoding_and_undo_restores_original() {
        let before = jpeg();
        let (_dir, path) = file(&before);
        let change = write(&path, 4, Some("Red")).unwrap();
        let after = fs::read(&path).unwrap();
        assert_eq!(crate::exif_processing::read_embedded_rating(&path), Some(4));
        assert_eq!(
            protected_bytes(&before, &parse(&before).unwrap()),
            protected_bytes(&after, &parse(&after).unwrap())
        );
        assert_eq!(
            image::load_from_memory(&before).unwrap().to_rgb8(),
            image::load_from_memory(&after).unwrap().to_rgb8()
        );
        restore(&path, &change.before, change.after_hash).unwrap();
        assert_eq!(fs::read(path).unwrap(), before);
    }
    #[test]
    fn progressive_scans_and_unrelated_app1_packets_remain_identical() {
        let mut before = include_bytes!("../tests/fixtures/jpeg-metadata/progressive.jpg").to_vec();
        let foreign = segment(b"unrelated APP1 metadata and bytes").unwrap();
        before.splice(2..2, foreign.clone());
        let (_dir, path) = file(&before);
        write(&path, 2, Some("Purple")).unwrap();
        let after = fs::read(&path).unwrap();
        assert!(after.windows(foreign.len()).any(|v| v == foreign));
        assert_eq!(
            protected_bytes(&before, &parse(&before).unwrap()),
            protected_bytes(&after, &parse(&after).unwrap())
        );
        assert_eq!(
            image::load_from_memory(&before).unwrap().to_rgb8(),
            image::load_from_memory(&after).unwrap().to_rgb8()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn preserves_extended_filesystem_attributes() {
        use std::os::fd::AsRawFd;
        let (_dir, path) = file(&jpeg());
        let source = File::open(&path).unwrap();
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    source.as_raw_fd(),
                    c"user.rapidroom-test".as_ptr(),
                    b"kept".as_ptr().cast(),
                    4,
                    0,
                )
            },
            0
        );
        write(&path, 4, None).unwrap();
        let after = File::open(&path).unwrap();
        let mut value = [0u8; 4];
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    after.as_raw_fd(),
                    c"user.rapidroom-test".as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            },
            4
        );
        assert_eq!(&value, b"kept");
    }

    #[test]
    fn preserves_foreign_keywords_namespaces_toolkit_and_both_xmp_forms() {
        for p in [
            packet(),
            packet()
                .replace("z:Rating=\"2\"", "")
                .replace("<z:Label>", "<z:Rating>2</z:Rating><z:Label>"),
        ] {
            let before = with_xmp(jpeg(), &p);
            let (_dir, path) = file(&before);
            write(&path, 5, Some("Yellow & gold")).unwrap();
            let after = fs::read(&path).unwrap();
            let text = xmp(&after);
            let metadata = crate::xmp::metadata(&text);
            assert_eq!(metadata.rating, Some(5));
            assert_eq!(metadata.label.as_deref(), Some("Yellow & gold"));
            assert_eq!(metadata.tags, ["café & R&D"]);
            for raw in [
                "x:xmptk=\"original toolkit\"",
                "f:rating=\"untouched\"",
                "<f:data>keep this</f:data>",
                "<dc:subject><rdf:Bag><rdf:li>café &amp; R&amp;D</rdf:li></rdf:Bag></dc:subject>",
            ] {
                assert!(text.contains(raw));
            }
            assert_eq!(after.len(), before.len());
        }
    }
    #[test]
    fn reconciles_existing_exif_ratings_both_byte_orders_reject_and_clear() {
        for little in [false, true] {
            let mut before = jpeg();
            before.splice(2..2, exif(little));
            let (_dir, path) = file(&before);
            for (rating, percent) in [(5, 99), (-1, 0), (0, 0), (3, 50)] {
                write(&path, rating, None).unwrap();
                let bytes = fs::read(&path).unwrap();
                let j = parse(&bytes).unwrap();
                let e = targets(&j, &bytes, b"Exif\0\0").unwrap().unwrap();
                let t = &bytes[e.data.start + 6..e.data.end];
                assert_eq!(tiff_u16(t, 18, little).unwrap(), rating.max(0) as u16);
                assert_eq!(tiff_u16(t, 30, little).unwrap(), percent);
                assert_eq!(
                    crate::xmp::metadata(&xmp(&bytes)).rating,
                    Some(rating as i8)
                );
                assert_eq!(
                    crate::exif_processing::read_embedded_rating(&path),
                    Some(rating.max(0) as u8)
                );
            }
        }
    }
    #[test]
    fn auxiliary_trailers_keep_offsets_or_refuse_expansion_without_writing() {
        let mut padded = with_xmp(jpeg(), &packet());
        padded.extend(b"GAINMAP trailer stays at this exact offset\xff\xd8");
        let (_dir, path) = file(&padded);
        write(&path, 4, Some("Green")).unwrap();
        let after = fs::read(&path).unwrap();
        assert_eq!(after.len(), padded.len());
        assert!(after.ends_with(b"GAINMAP trailer stays at this exact offset\xff\xd8"));
        let mut mpf = jpeg();
        let mut app2 = segment(b"MPF\0original bytes").unwrap();
        app2[1] = 0xe2;
        mpf.splice(2..2, app2);
        let (_dir, path) = file(&mpf);
        assert!(write(&path, 4, None).unwrap_err().contains("auxiliary"));
        assert_eq!(fs::read(path).unwrap(), mpf);
    }
    #[test]
    fn rejects_truncation_extended_packets_duplicates_and_changed_undo() {
        let bytes = jpeg();
        for n in 0..bytes.len() {
            assert!(parse(&bytes[..n]).is_err(), "truncation{n}");
        }
        let mut extended = bytes.clone();
        extended.splice(2..2, segment(EXTENDED).unwrap());
        let (_dir, path) = file(&extended);
        assert!(write(&path, 2, None).is_err());
        assert_eq!(fs::read(&path).unwrap(), extended);
        let dup = with_xmp(with_xmp(bytes.clone(), &packet()), &packet());
        let (_dir, path) = file(&dup);
        assert!(write(&path, 2, None).is_err());
        assert_eq!(fs::read(&path).unwrap(), dup);
        let (_dir, path) = file(&bytes);
        let change = write(&path, 2, None).unwrap();
        let mut changed = fs::read(&path).unwrap();
        changed.push(0);
        fs::write(&path, &changed).unwrap();
        assert!(restore(&path, &change.before, change.after_hash).is_err());
        assert_eq!(fs::read(path).unwrap(), changed);
    }
    #[test]
    fn refuses_extended_pointer_oversized_xmp_and_malformed_exif_without_writing() {
        let pointer = packet().replace("xmlns:z=", "xmlns:note=\"http://ns.adobe.com/xmp/note/\" note:HasExtendedXMP=\"0123456789ABCDEF0123456789ABCDEF\" xmlns:z=");
        let oversized = packet().replace("keep this", &"x".repeat(65_000));
        let malformed = {
            let mut bytes = jpeg();
            bytes.splice(2..2, segment(b"Exif\0\0II\x2a\0\xff\xff\xff\x7f").unwrap());
            bytes
        };
        // A valid, nearly full packet cannot accommodate a longer label.
        let mut full = packet().replace("keep this", &"x".repeat(62_000));
        while full.len() + XMP.len() + 2 < u16::MAX as usize {
            full.push(' ');
        }
        for bytes in [
            with_xmp(jpeg(), &pointer),
            malformed,
            with_xmp(jpeg(), &full),
        ] {
            let (_dir, path) = file(&bytes);
            assert!(write(&path, 3, Some(&"Label".repeat(15_000))).is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(segment(oversized.as_bytes()).is_err());
    }
    #[test]
    fn preserves_permissions_and_mtime_and_refuses_read_only_files() {
        let (_dir, path) = file(&jpeg());
        let old = filetime::FileTime::from_unix_time(1_500_000_000, 123_000_000);
        filetime::set_file_mtime(&path, old).unwrap();
        let perms = fs::metadata(&path).unwrap().permissions();
        write(&path, 1, None).unwrap();
        let meta = fs::metadata(&path).unwrap();
        assert_eq!(filetime::FileTime::from_last_modification_time(&meta), old);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(meta.permissions().mode(), perms.mode());
            let mut readonly = perms.clone();
            readonly.set_mode(0o444);
            fs::set_permissions(&path, readonly).unwrap();
            assert!(write(&path, 2, None).is_err());
            fs::set_permissions(&path, perms).unwrap();
        }
    }
}
