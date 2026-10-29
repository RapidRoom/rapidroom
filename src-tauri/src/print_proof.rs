use image::DynamicImage;
use lcms2::{Flags, Intent, PixelFormat, Profile, ThreadContext, Transform};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

const MAX_PROFILE_BYTES: u64 = 16 * 1024 * 1024;
const SRGB: &[u8] = include_bytes!("../icc/sRGB-v2-magic.icc");

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PrintIntent {
    Perceptual,
    #[default]
    Relative,
}

impl PrintIntent {
    fn lcms(self) -> Intent {
        match self {
            Self::Perceptual => Intent::Perceptual,
            Self::Relative => Intent::RelativeColorimetric,
        }
    }
}

fn default_bpc() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintProfileOptions {
    pub profile_path: String,
    #[serde(default)]
    pub intent: PrintIntent,
    #[serde(default = "default_bpc")]
    pub black_point_compensation: bool,
}

impl PrintProfileOptions {
    fn flags(&self) -> Flags {
        let flags = Flags::COPY_ALPHA;
        if self.black_point_compensation {
            flags | Flags::BLACKPOINT_COMPENSATION
        } else {
            flags
        }
    }
}

fn read_profile(path: &str) -> Result<Vec<u8>, String> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "Cannot open the chosen ICC profile".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect the ICC profile".to_string())?;
    if !metadata.is_file() || metadata.len() < 128 || metadata.len() > MAX_PROFILE_BYTES {
        return Err("Choose a regular ICC profile file between 128 bytes and 16 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROFILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read the ICC profile".to_string())?;
    if bytes.len() < 128
        || bytes.len() as u64 > MAX_PROFILE_BYTES
        || u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize != bytes.len()
        || &bytes[36..40] != b"acsp"
    {
        return Err("Invalid or incomplete ICC profile".into());
    }
    Ok(bytes)
}

fn parse_profile(
    context: &ThreadContext,
    bytes: &[u8],
    proof: bool,
) -> Result<Profile<ThreadContext>, String> {
    if bytes.len() < 128 {
        return Err("Invalid or incomplete ICC profile".into());
    }
    let valid_class = if proof {
        &bytes[12..16] == b"prtr"
    } else {
        matches!(&bytes[12..16], b"prtr" | b"mntr" | b"spac")
    };
    let valid_space = &bytes[16..20] == b"RGB " || (proof && &bytes[16..20] == b"CMYK");
    if !valid_class || !valid_space {
        return Err(if proof {
            "Soft proof needs an RGB or CMYK printer/paper output profile"
        } else {
            "Export needs an RGB output or display profile; CMYK export is not supported"
        }
        .into());
    }
    Profile::new_icc_context(context, bytes).map_err(|_| "Cannot parse the ICC profile".to_string())
}

pub fn convert_export(
    image: &DynamicImage,
    options: &PrintProfileOptions,
) -> Result<(DynamicImage, Vec<u8>), String> {
    let bytes = read_profile(&options.profile_path)?;
    let context = ThreadContext::new();
    let input = Profile::new_icc_context(&context, SRGB)
        .map_err(|_| "Cannot open built-in sRGB profile".to_string())?;
    let output = parse_profile(&context, &bytes, false)?;
    let transform: Transform<[u16; 4], [u16; 4], ThreadContext> = Transform::new_flags_context(
        &context,
        &input,
        PixelFormat::RGBA_16,
        &output,
        PixelFormat::RGBA_16,
        options.intent.lcms(),
        options.flags(),
    )
    .map_err(|_| "ICC profile cannot be used for this export transform".to_string())?;
    let mut rgba = image.to_rgba16();
    transform.transform_in_place(bytemuck::cast_slice_mut(rgba.as_mut()));
    Ok((DynamicImage::ImageRgba16(rgba), bytes))
}

pub fn soft_proof(
    image: &DynamicImage,
    options: &PrintProfileOptions,
) -> Result<DynamicImage, String> {
    let bytes = read_profile(&options.profile_path)?;
    let context = ThreadContext::new();
    let srgb = Profile::new_icc_context(&context, SRGB)
        .map_err(|_| "Cannot open built-in sRGB profile".to_string())?;
    let proof = parse_profile(&context, &bytes, true)?;
    let transform: Transform<[u8; 4], [u8; 4], ThreadContext> = Transform::new_proofing_context(
        &context,
        &srgb,
        PixelFormat::RGBA_8,
        &srgb,
        PixelFormat::RGBA_8,
        &proof,
        options.intent.lcms(),
        Intent::RelativeColorimetric,
        options.flags() | Flags::SOFT_PROOFING,
    )
    .map_err(|_| "ICC profile cannot be used for soft proofing".to_string())?;
    let mut rgba = image.to_rgba8();
    transform.transform_in_place(bytemuck::cast_slice_mut(rgba.as_mut()));
    Ok(DynamicImage::ImageRgba8(rgba))
}

#[tauri::command]
pub async fn validate_print_profile(
    options: PrintProfileOptions,
    proof: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let pixel = DynamicImage::new_rgba8(1, 1);
        if proof {
            soft_proof(&pixel, &options).map(|_| ())
        } else {
            convert_export(&pixel, &options).map(|_| ())
        }
    })
    .await
    .map_err(|_| "ICC validation task failed".to_string())?
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use lcms2::{CIExyY, CIExyYTRIPLE, ProfileClassSignature, ToneCurve};
    use std::io::Write;

    pub(crate) fn fixture() -> (tempfile::NamedTempFile, Vec<u8>) {
        let gamma = ToneCurve::new(1.8);
        let mut profile = Profile::new_rgb(
            &CIExyY {
                x: 0.3127,
                y: 0.3290,
                Y: 1.0,
            },
            &CIExyYTRIPLE {
                Red: CIExyY {
                    x: 0.64,
                    y: 0.33,
                    Y: 1.0,
                },
                Green: CIExyY {
                    x: 0.30,
                    y: 0.60,
                    Y: 1.0,
                },
                Blue: CIExyY {
                    x: 0.15,
                    y: 0.06,
                    Y: 1.0,
                },
            },
            &[&gamma, &gamma, &gamma],
        )
        .unwrap();
        profile.set_device_class(ProfileClassSignature::OutputClass);
        let bytes = profile.icc().unwrap();
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();
        (file, bytes)
    }

    #[test]
    fn selected_profile_converts_rgb_preserves_alpha_and_keeps_original_profile_bytes() {
        let (file, bytes) = fixture();
        let options = PrintProfileOptions {
            profile_path: file.path().to_string_lossy().into(),
            intent: PrintIntent::Relative,
            black_point_compensation: true,
        };
        let image = DynamicImage::ImageRgba16(ImageBuffer::from_pixel(
            2,
            1,
            Rgba([32000, 12000, 45000, 12345]),
        ));
        let (converted, embedded) = convert_export(&image, &options).unwrap();
        assert_eq!(embedded, bytes);
        assert_ne!(converted.to_rgb16(), image.to_rgb16());
        assert!(converted.to_rgba16().pixels().all(|p| p[3] == 12345));
        assert!(soft_proof(&image, &options).is_ok());
    }

    #[test]
    fn malformed_oversized_non_file_and_ineligible_profiles_are_rejected() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&[0; 128]).unwrap();
        assert!(read_profile(file.path().to_str().unwrap()).is_err());
        file.as_file().set_len(MAX_PROFILE_BYTES + 1).unwrap();
        assert!(read_profile(file.path().to_str().unwrap()).is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(read_profile(dir.path().to_str().unwrap()).is_err());
        assert!(read_profile("/does-not-exist/profile.icc").is_err());
        let context = ThreadContext::new();
        let mut profile = Profile::new_srgb();
        profile.set_device_class(ProfileClassSignature::LinkClass);
        assert!(parse_profile(&context, &profile.icc().unwrap(), false).is_err());
        let profile = Profile::new_srgb();
        assert!(parse_profile(&context, &profile.icc().unwrap(), true).is_err());
    }
    /// Lossless outputs from the real transforms for independent tificc checks.
    #[test]
    #[ignore = "writes reference artifacts only when an owned case is supplied"]
    fn write_littlecms_reference_artifacts() {
        let dir =
            std::path::PathBuf::from(std::env::var("RAPIDROOM_PRINT_REFERENCE_CASE").unwrap());
        assert!(dir.is_dir() && dir.components().any(|p| p.as_os_str() == "samples"));
        let image = DynamicImage::ImageRgba16(ImageBuffer::from_fn(257, 65, |x, y| {
            Rgba([
                (x * 255) as u16,
                (y * 1023) as u16,
                ((x * 187 + y * 887) % 65536) as u16,
                [0, 16384, 32768, 65535][((x + y) % 4) as usize],
            ])
        }));
        image.to_rgb16().save(dir.join("source16.tiff")).unwrap();
        image.to_rgb8().save(dir.join("source8.tiff")).unwrap();
        std::fs::write(dir.join("srgb.icc"), SRGB).unwrap();
        let (_file, rgb_bytes) = fixture();
        std::fs::write(dir.join("rgb-output.icc"), &rgb_bytes).unwrap();
        let mut cases = Vec::new();
        for (name, intent) in [
            ("relative", PrintIntent::Relative),
            ("perceptual", PrintIntent::Perceptual),
        ] {
            for bpc in [false, true] {
                let options = PrintProfileOptions {
                    profile_path: dir.join("rgb-output.icc").to_str().unwrap().into(),
                    intent,
                    black_point_compensation: bpc,
                };
                let (output, profile_bytes) = convert_export(&image, &options).unwrap();
                assert_eq!(profile_bytes, rgb_bytes);
                for (input, output) in image.to_rgba16().pixels().zip(output.to_rgba16().pixels()) {
                    assert_eq!(input[3], output[3]);
                }
                let export_name = format!("export-{name}-{bpc}.tiff");
                output.to_rgb16().save(dir.join(&export_name)).unwrap();
                let proof_options = PrintProfileOptions {
                    profile_path: dir.join("proof.icc").to_str().unwrap().into(),
                    ..options
                };
                let proof = soft_proof(&image, &proof_options).unwrap();
                for (input, output) in image.to_rgba8().pixels().zip(proof.to_rgba8().pixels()) {
                    assert_eq!(input[3], output[3]);
                }
                let proof_name = format!("proof-{name}-{bpc}.tiff");
                proof.to_rgb8().save(dir.join(&proof_name)).unwrap();
                assert!(
                    convert_export(&image, &proof_options).is_err(),
                    "CMYK profile must not be tagged onto RGB export"
                );
                cases.push(serde_json::json!({"intent":name,"bpc":bpc,"export":export_name,"proof":proof_name}));
            }
        }
        std::fs::write(
            dir.join("cases.json"),
            serde_json::to_vec_pretty(&cases).unwrap(),
        )
        .unwrap();
    }
}
