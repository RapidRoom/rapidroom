//! RawForge's MIT model/preprocessing contract. Third-party notices: src-tauri/resources/raw-denoise-licenses/.
use crate::denoising::DenoiseJob;
use anyhow::{Result, anyhow, ensure};
use half::f16;
use nalgebra::{Matrix3, Vector3};
use ort::session::{RunOptions, Session};
use ort::value::Tensor;
use rawler::{
    decoders::RawDecodeParams,
    rawimage::{RawImage, RawImageData, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::Emitter;
use tauri::Manager;

const TILE: usize = 256;
const OVERLAP: usize = 64;
const PIPELINE: &str = "rawforge-bayer-malvar-f16-cpu-v1";
const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

pub struct Model {
    pub name: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
}
pub const BEST: Model = Model {
    name: "TreeNet Best",
    file: "ShadowWeightedL1.onnx",
    sha256: "76a07047a33dba3fa27330cec483b43bb57ecb1c1f10db8d979355ce110e34c9",
};
pub const FAST: Model = Model {
    name: "TreeNet Fast",
    file: "ShadowWeightedL1_super_light.onnx",
    sha256: "1684f549fec52812ffacc3020bfd61c5fde610e77460fcc03b2a7003f61e60dd",
};
pub const SHARPEN: Model = Model {
    name: "DeepSharpen",
    file: "Deblur_deep_24.onnx",
    sha256: "4b714af59026352990f5cc1b84af5dc01cf4a66abd0b5f8d845b0c43d99511a7",
};
pub type Models = Vec<Arc<Mutex<Session>>>;

fn cpu_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from).min(4)
}
fn cpu_session(path: &Path) -> Result<Session> {
    Ok(Session::builder()?
        .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default().build()])?
        .with_intra_threads(cpu_threads())?
        .commit_from_file(path)?)
}

pub async fn models(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
    fast: bool,
    sharpen: bool,
    job: &DenoiseJob,
) -> Result<Models> {
    let wanted: Vec<&Model> = if sharpen {
        vec![if fast { &FAST } else { &BEST }, &SHARPEN]
    } else {
        vec![if fast { &FAST } else { &BEST }]
    };
    let acquire = state.ai_init_lock.lock();
    tokio::pin!(acquire);
    let _guard = loop {
        tokio::select! {
            guard = &mut acquire => break guard,
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => { job.check().map_err(anyhow::Error::msg)?; }
        }
    };
    let dir = crate::ai_processing::get_models_dir(app)?;
    let mut result = Vec::new();
    for model in wanted {
        job.check().map_err(anyhow::Error::msg)?;
        if let Some(session) = state
            .ai_state
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.tree_models.get(model.file).cloned())
        {
            result.push(session);
            continue;
        }
        job.progress(format!("Preparing {} model…", model.name));
        let url = format!(
            "https://github.com/rymuelle/RawForge/releases/download/onnx_v1.0.0/{}",
            model.file
        );
        let download = crate::ai_processing::download_and_verify_model(
            app,
            &dir,
            model.file,
            &url,
            model.sha256,
            model.name,
        );
        tokio::pin!(download);
        loop {
            tokio::select! {
                r = &mut download => { r?; break; }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    if let Err(e) = job.check() {
                        let _ = app.emit("ai-model-download-finish", model.name);
                        return Err(anyhow::Error::msg(e));
                    }
                }
            }
        }
        job.check().map_err(anyhow::Error::msg)?;
        let _ = ort::init().with_name("AI-Raw-Denoise").commit();
        let path = dir.join(model.file);
        let session = tokio::task::spawn_blocking(move || cpu_session(&path)).await??;
        job.check().map_err(anyhow::Error::msg)?;
        let session = Arc::new(Mutex::new(session));
        let mut ai = state.ai_state.lock().unwrap();
        ai.get_or_insert_with(crate::ai_processing::AiState::default)
            .tree_models
            .insert(model.file, session.clone());
        result.push(session);
    }
    crate::register_exit_handler();
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
struct Area {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
fn area(raw: &RawImage) -> Result<Area> {
    ensure!(
        raw.cpp == 1,
        "AI raw denoise requires a Bayer RAW, not a linear DNG or RGB image. Choose More methods for this file."
    );
    let RawPhotometricInterpretation::Cfa(cfa) = &raw.photometric else {
        return Err(anyhow!(
            "AI raw denoise requires a Bayer RAW. Choose More methods for this file."
        ));
    };
    ensure!(
        cfa.cfa.width == 2 && cfa.cfa.height == 2 && cfa.cfa.is_rgb(),
        "AI raw denoise currently supports RGB Bayer sensors, not X-Trans."
    );
    let active = raw.active_area;
    let (mut x, mut y, right, bottom) = active.map_or((0, 0, raw.width, raw.height), |r| {
        (r.p.x, r.p.y, r.p.x + r.d.w, r.p.y + r.d.h)
    });
    ensure!(
        right <= raw.width && bottom <= raw.height && right >= x + 2 && bottom >= y + 2,
        "Invalid RAW active area"
    );
    let red = (0..2)
        .flat_map(|dy| (0..2).map(move |dx| (dx, dy)))
        .find(|&(dx, dy)| cfa.cfa.color_at(y + dy, x + dx) == 0)
        .ok_or_else(|| anyhow!("Invalid Bayer pattern"))?;
    x += red.0;
    y += red.1;
    let w = (right - x) / 2 * 2;
    let h = (bottom - y) / 2 * 2;
    ensure!(
        w >= TILE && h >= TILE && w <= 20000 && h <= 20000 && w * h <= 200_000_000,
        "Unsupported Bayer dimensions for AI raw denoise"
    );
    ensure!(
        cfa.cfa.color_at(y + 1, x + 1) == 2
            && cfa.cfa.color_at(y, x + 1) == 1
            && cfa.cfa.color_at(y + 1, x) == 1,
        "Unsupported Bayer arrangement"
    );
    ensure!(
        raw.blacklevel.cpp == 1
            && [1, 2].contains(&raw.blacklevel.width)
            && [1, 2].contains(&raw.blacklevel.height)
            && raw.blacklevel.levels.len() == raw.blacklevel.width * raw.blacklevel.height
            && raw.whitelevel.0.len() == 1,
        "Unsupported RAW black/white levels"
    );
    // Pinned rawler reads repeating DNG black levels relative to the full buffer.
    let (left, top) = raw.active_area.map_or((0, 0), |r| (r.p.x, r.p.y));
    ensure!(raw.blacklevel.shift(left, top).as_vec() == raw.blacklevel.as_vec(),
        "This sensor's black-level layout is unsupported by AI raw denoise. Choose More methods.");
    let pixels = match &raw.data {
        RawImageData::Integer(data) => data.len(),
        RawImageData::Float(data) => data.len(),
    };
    ensure!(pixels == raw.width * raw.height, "Invalid RAW buffer");
    Ok(Area { x, y, w, h })
}
fn black(raw: &RawImage, y: usize, x: usize) -> f32 {
    raw.blacklevel.levels
        [(y % raw.blacklevel.height) * raw.blacklevel.width + x % raw.blacklevel.width]
        .as_f32()
}
fn colour_matrix(raw: &RawImage) -> Result<Matrix3<f64>> {
    let xyz_to_cam = if raw.xyz_to_cam[..3].iter().flatten().any(|v| *v != 0.) {
        Matrix3::from_fn(|r, c| raw.xyz_to_cam[r][c] as f64)
    } else {
        let matrix = raw
            .color_matrix
            .get(&rawler::imgop::xyz::Illuminant::D65)
            .or_else(|| raw.color_matrix.get(&rawler::imgop::xyz::Illuminant::D50))
            .ok_or_else(|| {
                anyhow!("This camera has no daylight colour calibration for AI denoise.")
            })?;
        ensure!(
            matrix.len() == 9,
            "AI denoise requires a three-colour camera calibration."
        );
        Matrix3::from_fn(|r, c| matrix[r * 3 + c] as f64)
    };
    let cam_to_xyz = xyz_to_cam
        .try_inverse()
        .ok_or_else(|| anyhow!("This camera has no usable colour calibration for AI denoise."))?;
    let xyz_to_rec2020 = Matrix3::from_row_slice(&[
        1.71666343,
        -0.35567332,
        -0.25336809,
        -0.66667384,
        1.61645574,
        0.0157683,
        0.01764248,
        -0.04277698,
        0.94224328,
    ]);
    let matrix = xyz_to_rec2020 * cam_to_xyz;
    ensure!(
        matrix.iter().all(|v| v.is_finite()),
        "Invalid camera colour calibration"
    );
    Ok(matrix)
}
fn normalized_mosaic(
    raw: &RawImage,
    a: Area,
    matrix: &Matrix3<f64>,
    job: &DenoiseJob,
) -> Result<Vec<f64>> {
    let data = raw.data.as_f32();
    let white = raw.whitelevel.0[0] as f32;
    let mut mosaic = vec![0.; a.w * a.h];
    for y in (0..a.h).step_by(2) {
        job.check().map_err(anyhow::Error::msg)?;
        for x in (0..a.w).step_by(2) {
            let mut v = [0.; 4];
            for (i, (dy, dx)) in [(0, 0), (0, 1), (1, 0), (1, 1)].iter().enumerate() {
                let b = black(raw, a.y + y + dy, a.x + x + dx);
                ensure!(white > b && b.is_finite(), "Invalid RAW normalization");
                v[i] = ((data[(a.y + y + dy) * raw.width + a.x + x + dx] - b) / (white - b)) as f64;
            }
            let green = (v[1] + v[2]) * 0.5;
            mosaic[y * a.w + x] =
                matrix[(0, 0)] * v[0] + matrix[(0, 1)] * green + matrix[(0, 2)] * v[3];
            mosaic[y * a.w + x + 1] =
                matrix[(1, 0)] * v[0] + matrix[(1, 1)] * v[1] + matrix[(1, 2)] * v[3];
            mosaic[(y + 1) * a.w + x] =
                matrix[(1, 0)] * v[0] + matrix[(1, 1)] * v[2] + matrix[(1, 2)] * v[3];
            mosaic[(y + 1) * a.w + x + 1] =
                matrix[(2, 0)] * v[0] + matrix[(2, 1)] * green + matrix[(2, 2)] * v[3];
        }
    }
    ensure!(mosaic.iter().all(|v| v.is_finite()), "Nonfinite RAW data");
    Ok(mosaic)
}
fn reflected(i: isize, n: usize) -> usize {
    if i < 0 {
        (-i - 1) as usize
    } else if i >= n as isize {
        (2 * n as isize - i - 1) as usize
    } else {
        i as usize
    }
}
fn convolve(
    data: &[f64],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    kernel: &[f64; 25],
    transpose: bool,
) -> f64 {
    let mut sum = 0.;
    for ky in 0..5 {
        for kx in 0..5 {
            let k = if transpose {
                kernel[kx * 5 + ky]
            } else {
                kernel[ky * 5 + kx]
            };
            if k != 0. {
                sum += k * data[reflected(y as isize + ky as isize - 2, h) * w
                    + reflected(x as isize + kx as isize - 2, w)];
            }
        }
    }
    sum / 8.
}
fn malvar(mosaic: &[f64], w: usize, h: usize, job: &DenoiseJob) -> Result<Vec<f16>> {
    // Malvar 5x5 kernels, adapted from colour-demosaicing (BSD-3-Clause).
    const G: [f64; 25] = [
        0., 0., -1., 0., 0., 0., 0., 2., 0., 0., -1., 2., 4., 2., -1., 0., 0., 2., 0., 0., 0., 0.,
        -1., 0., 0.,
    ];
    const RB: [f64; 25] = [
        0., 0., 0.5, 0., 0., 0., -1., 0., -1., 0., -1., 4., 5., 4., -1., 0., -1., 0., -1., 0., 0.,
        0., 0.5, 0., 0.,
    ];
    const CROSS: [f64; 25] = [
        0., 0., -1.5, 0., 0., 0., 2., 0., 2., 0., -1.5, 0., 6., 0., -1.5, 0., 2., 0., 2., 0., 0.,
        0., -1.5, 0., 0.,
    ];
    let mut rgb = vec![f16::ZERO; 3 * w * h];
    for y in 0..h {
        job.check().map_err(anyhow::Error::msg)?;
        for x in 0..w {
            let v = mosaic[y * w + x];
            let channels = match (y % 2, x % 2) {
                (0, 0) => [
                    v,
                    convolve(mosaic, w, h, x, y, &G, false),
                    convolve(mosaic, w, h, x, y, &CROSS, false),
                ],
                (1, 1) => [
                    convolve(mosaic, w, h, x, y, &CROSS, false),
                    convolve(mosaic, w, h, x, y, &G, false),
                    v,
                ],
                (0, 1) => [
                    convolve(mosaic, w, h, x, y, &RB, false),
                    v,
                    convolve(mosaic, w, h, x, y, &RB, true),
                ],
                _ => [
                    convolve(mosaic, w, h, x, y, &RB, true),
                    v,
                    convolve(mosaic, w, h, x, y, &RB, false),
                ],
            };
            for c in 0..3 {
                rgb[c * w * h + y * w + x] = f16::from_f64(channels[c].clamp(0., 1.));
            }
        }
    }
    Ok(rgb)
}

#[derive(Debug)]
struct Axis {
    starts: Vec<usize>,
    overlaps: Vec<(usize, usize)>,
}
fn axis(n: usize) -> Axis {
    let mut starts = vec![0];
    let mut overlaps = Vec::new();
    let mut c = 1;
    while starts.last().unwrap() + TILE < n {
        let next = (TILE - OVERLAP) * c;
        let last = next + TILE >= n;
        let start = if last { n - TILE } else { next };
        starts.push(start);
        let mut zeros = if last {
            starts[starts.len() - 2] + TILE - OVERLAP - start
        } else {
            0
        };
        let mut ramp = OVERLAP;
        if last && c == 1 && TILE > n / 2 && TILE != n {
            ramp += zeros;
            zeros = 0;
        }
        if overlaps.is_empty() {
            overlaps.push((zeros, ramp));
        }
        overlaps.push((zeros, ramp));
        c += 1;
    }
    Axis { starts, overlaps }
}
fn weight(a: &Axis, tile: usize, pixel: usize) -> f16 {
    if a.starts.len() == 1 {
        return f16::ONE;
    }
    let (zeros, ramp) = a.overlaps[tile];
    let rising = |p: usize| {
        if p < zeros {
            0.
        } else if p < zeros + ramp {
            (p - zeros) as f32 / (ramp - 1) as f32
        } else {
            1.
        }
    };
    let mut v = f16::ONE;
    if tile > 0 {
        v = f16::from_f32(rising(pixel));
    }
    if tile + 1 < a.starts.len() {
        v = f16::from_f32(v.to_f32() * f16::from_f32(rising(TILE - 1 - pixel)).to_f32());
    }
    v
}
fn infer(
    rgb: &[f16],
    w: usize,
    h: usize,
    iso: u32,
    session: &Arc<Mutex<Session>>,
    label: &str,
    job: &DenoiseJob,
) -> Result<Vec<f16>> {
    let xs = axis(w);
    let ys = axis(h);
    let total = xs.starts.len() * ys.starts.len();
    let options = Arc::new(RunOptions::new()?);
    job.attach_run_options(options.clone())
        .map_err(anyhow::Error::msg)?;
    let cond = Tensor::from_array((
        [1usize, 1],
        vec![f16::from_f32(iso.min(65535) as f32 / 6400.)].into_boxed_slice(),
    ))?;
    let mut output = vec![f16::ZERO; rgb.len()];
    for (yi, &y) in ys.starts.iter().enumerate() {
        for (xi, &x) in xs.starts.iter().enumerate() {
            job.check().map_err(anyhow::Error::msg)?;
            let mut input = Vec::with_capacity(3 * TILE * TILE);
            for c in 0..3 {
                for row in y..y + TILE {
                    input.extend_from_slice(
                        &rgb[c * w * h + row * w + x..c * w * h + row * w + x + TILE],
                    );
                }
            }
            let input = Tensor::from_array(([1usize, 3, TILE, TILE], input.into_boxed_slice()))?;
            let tile = {
                let mut model = session.lock().unwrap();
                job.check().map_err(anyhow::Error::msg)?;
                let values = match model
                    .run_with_options(ort::inputs!["input"=>input,"cond"=>&cond], &*options)
                {
                    Ok(v) => v,
                    Err(e) => {
                        job.check().map_err(anyhow::Error::msg)?;
                        return Err(e.into());
                    }
                };
                let (shape, values) = values["output"].try_extract_tensor::<f16>()?;
                ensure!(
                    shape.as_ref() == [1, 3, TILE as i64, TILE as i64],
                    "Unexpected TreeNet output shape"
                );
                values.to_vec()
            };
            ensure!(
                tile.iter().all(|v| v.is_finite()),
                "TreeNet returned nonfinite pixels"
            );
            for dy in 0..TILE {
                for dx in 0..TILE {
                    let mask =
                        f16::from_f32(weight(&ys, yi, dy).to_f32() * weight(&xs, xi, dx).to_f32());
                    for c in 0..3 {
                        let i = c * w * h + (y + dy) * w + x + dx;
                        let prediction = f16::from_f32(
                            tile[c * TILE * TILE + dy * TILE + dx].to_f32() * mask.to_f32(),
                        );
                        output[i] = f16::from_f32(output[i].to_f32() + prediction.to_f32());
                    }
                }
            }
            job.progress(format!(
                "{}: {} / {} tiles",
                label,
                yi * xs.starts.len() + xi + 1,
                total
            ));
        }
    }
    Ok(output)
}
fn restore(
    raw: &mut RawImage,
    a: Area,
    matrix: &Matrix3<f64>,
    rgb: &[f16],
    job: &DenoiseJob,
) -> Result<()> {
    let inverse = matrix
        .try_inverse()
        .ok_or_else(|| anyhow!("Invalid camera matrix"))?;
    let mut data = raw.data.as_f32().into_owned();
    let white = raw.whitelevel.0[0] as f32;
    for y in 0..a.h {
        job.check().map_err(anyhow::Error::msg)?;
        for x in 0..a.w {
            let i = y * a.w + x;
            let v = inverse
                * Vector3::new(
                    rgb[i].to_f64(),
                    rgb[a.w * a.h + i].to_f64(),
                    rgb[2 * a.w * a.h + i].to_f64(),
                );
            let c = if y % 2 == 0 && x % 2 == 0 {
                0
            } else if y % 2 == 1 && x % 2 == 1 {
                2
            } else {
                1
            };
            let camera = ((v[c] * 65536. - 1.).clamp(0., 65535.).floor() / 65535.) as f32;
            let b = black(raw, a.y + y, a.x + x);
            data[(a.y + y) * raw.width + a.x + x] = camera * (white - b) + b;
        }
    }
    raw.data = RawImageData::Float(data);
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct CacheRecord {
    key: String,
    hash: String,
}
fn cache_hit(dir: &Path, key: &str) -> Option<PathBuf> {
    let record: CacheRecord =
        serde_json::from_slice(&fs::read(dir.join("cache.json")).ok()?).ok()?;
    if record.key != key {
        return None;
    }
    let path = dir.join("sensor.dng");
    let bytes = fs::read(&path).ok()?;
    (blake3::hash(&bytes).to_hex().as_str() == record.hash).then_some(path)
}
fn write_dng(path: &Path, raw: &RawImage, metadata: &rawler::decoders::RawMetadata) -> Result<()> {
    use rawler::{
        dng::{
            CropMode, DNG_VERSION_V1_4, DngCompression, DngPhotometricConversion, writer::DngWriter,
        },
        tags::{ExifTag, TiffCommonTag},
    };
    let mut file = fs::File::create(path)?;
    let mut writer = DngWriter::new(&mut file, DNG_VERSION_V1_4)?;
    let mut frame = writer.subframe_on_root(0);
    frame.raw_image(
        raw,
        CropMode::Best,
        DngCompression::Uncompressed,
        DngPhotometricConversion::Original,
        1,
    )?;
    // Pinned rawler emits a vector here; TIFF requires one scalar strip height.
    let threshold = std::env::var("RAWLER_DNG_MULTISTRIP_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let rows = if raw.height > threshold {
        std::env::var("RAWLER_DNG_ROWS_PER_STRIP")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(256)
    } else {
        raw.height
    };
    frame
        .ifd_mut()
        .add_tag(TiffCommonTag::RowsPerStrip, rows as u32);
    frame.finalize()?;
    writer.load_base_tags(raw)?;
    writer.load_metadata(metadata)?;
    writer.root_ifd_mut().add_tag(
        ExifTag::Orientation,
        metadata
            .exif
            .orientation
            .unwrap_or(raw.orientation.to_u16()),
    );
    writer.exif_ifd_mut().remove_tag(ExifTag::MakerNotes);
    writer.close()?;
    file.sync_all()?;
    Ok(())
}
fn evict(root: &Path, keep: &Path) {
    let mut entries = Vec::new();
    let mut size = 0;
    if let Ok(dirs) = fs::read_dir(root) {
        for entry in dirs.flatten() {
            let p = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir())
                || p.file_name()
                    .is_some_and(|s| s.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if let Ok(metadata) = fs::metadata(p.join("sensor.dng")) {
                size += metadata.len();
                entries.push((metadata.modified().ok(), metadata.len(), p));
            }
        }
    }
    entries.sort_by_key(|e| e.0);
    for (_, bytes, p) in entries {
        if size <= CACHE_LIMIT {
            break;
        }
        if p != keep && fs::remove_dir_all(p).is_ok() {
            size = size.saturating_sub(bytes);
        }
    }
}

pub fn cached_sensor(
    bytes: &[u8],
    app: &tauri::AppHandle,
    fast: bool,
    sharpen: bool,
    models: &Models,
    job: &DenoiseJob,
) -> Result<PathBuf> {
    let root = app.path().app_cache_dir()?.join("raw-denoise");
    cached_sensor_at(bytes, &root, fast, sharpen, models, job)
}
fn cached_sensor_at(
    bytes: &[u8],
    root: &Path,
    fast: bool,
    sharpen: bool,
    models: &Models,
    job: &DenoiseJob,
) -> Result<PathBuf> {
    job.check().map_err(anyhow::Error::msg)?;
    job.progress("Reading RAW sensor…");
    let source = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&source)?;
    let mut raw = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;
    let metadata = decoder.raw_metadata(&source, &RawDecodeParams::default())?;
    let a = area(&raw)?;
    let matrix = colour_matrix(&raw)?;
    let identity = serde_json::json!({"pipeline":PIPELINE,"cpu_threads":cpu_threads(),"source":blake3::hash(bytes).to_hex().as_str(),"model":if fast {FAST.sha256} else {BEST.sha256},"sharpen":sharpen.then_some(SHARPEN.sha256),"shape":[raw.width,raw.height,a.x,a.y,a.w,a.h],"matrix":matrix.as_slice(),"black":raw.blacklevel.as_vec(),"white":raw.whitelevel.0});
    let key = blake3::hash(&serde_json::to_vec(&identity)?)
        .to_hex()
        .to_string();
    fs::create_dir_all(root)?;
    let cached = root.join(&key);
    if let Some(path) = cache_hit(&cached, &key) {
        job.check().map_err(anyhow::Error::msg)?;
        job.progress("Using cached AI raw denoise…");
        return Ok(path);
    }
    if cached.exists() {
        fs::remove_dir_all(&cached)?;
    }
    let iso = metadata
        .exif
        .iso_speed
        .or(metadata.exif.iso_speed_ratings.map(u32::from))
        .ok_or_else(|| anyhow!("RAW has no ISO metadata for TreeNet conditioning."))?;
    ensure!(iso > 0, "RAW ISO must be positive");
    ensure!(
        models.len() == if sharpen { 2 } else { 1 },
        "Incomplete TreeNet model chain"
    );
    job.progress("Preparing linear RAW model input…");
    let mosaic = normalized_mosaic(&raw, a, &matrix, job)?;
    let mut rgb = malvar(&mosaic, a.w, a.h, job)?;
    drop(mosaic);
    for (i, model) in models.iter().enumerate() {
        rgb = infer(
            &rgb,
            a.w,
            a.h,
            iso,
            model,
            if i == 0 { "Denoising" } else { "Sharpening" },
            job,
        )?;
    }
    restore(&mut raw, a, &matrix, &rgb, job)?;
    drop(rgb);
    job.check().map_err(anyhow::Error::msg)?;
    let staging = tempfile::Builder::new().prefix(".tree-").tempdir_in(root)?;
    let path = staging.path().join("sensor.dng");
    write_dng(&path, &raw, &metadata)?;
    job.check().map_err(anyhow::Error::msg)?;
    let record = CacheRecord {
        key: key.clone(),
        hash: blake3::hash(&fs::read(&path)?).to_hex().to_string(),
    };
    fs::write(
        staging.path().join("cache.json"),
        serde_json::to_vec(&record)?,
    )?;
    job.check().map_err(anyhow::Error::msg)?;
    match fs::rename(staging.path(), &cached) {
        Ok(()) => {}
        Err(e) => {
            if cache_hit(&cached, &key).is_none() {
                return Err(e.into());
            }
        }
    }
    evict(root, &cached);
    Ok(cached.join("sensor.dng"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawler::{
        cfa::CFA,
        decoders::Camera,
        pixarray::PixU16,
        rawimage::{BlackLevel, CFAConfig, WhiteLevel},
    };
    fn sensor(pattern: &str) -> RawImage {
        let mut camera = Camera::new();
        camera.cfa = CFA::new(pattern);
        let photometric = RawPhotometricInterpretation::Cfa(CFAConfig::new_from_camera(&camera));
        let levels = [100u16, 200, 300, 400];
        let pixels = (0..258 * 258)
            .map(|i| {
                let b = levels[(i / 258 % 2) * 2 + i % 2];
                b + (4100 - b) / 4
            })
            .collect();
        RawImage::new(
            camera,
            PixU16::new_with(pixels, 258, 258),
            1,
            [2., 1., 1.5, 1.],
            photometric,
            Some(BlackLevel::new(&levels, 2, 2, 1)),
            Some(WhiteLevel::new(vec![4100])),
            false,
        )
    }
    #[test]
    fn every_bayer_pattern_aligns_without_moving_sensor_metadata() {
        for pattern in ["RGGB", "BGGR", "GBRG", "GRBG"] {
            let raw = sensor(pattern);
            let a = area(&raw).unwrap();
            let mosaic =
                normalized_mosaic(&raw, a, &Matrix3::identity(), &DenoiseJob::testing()).unwrap();
            assert!(mosaic.iter().all(|v| (*v - 0.25).abs() < 1e-6));
            assert_eq!((raw.width, raw.height), (258, 258));
        }
    }
    #[test]
    fn odd_active_area_black_pattern_is_rejected_before_inference() {
        use rawler::imgop::{Dim2, Point, Rect};
        let mut raw = sensor("RGGB");
        raw.active_area = Some(Rect::new(Point::new(1, 1), Dim2::new(257, 257)));
        assert!(area(&raw).unwrap_err().to_string().contains("black-level layout"));
        raw.blacklevel = BlackLevel::new(&[100_u16; 4], 2, 2, 1);
        assert!(area(&raw).is_ok());
    }

    #[test]
    fn malvar_matches_published_small_image_including_reflected_edges() {
        let input = [
            0.30980393, 0.36078432, 0.30588236, 0.3764706, 0.35686275, 0.39607844, 0.36078432,
            0.40000001,
        ];
        let expected = [
            [0.30980393, 0.31666668, 0.32941177],
            [0.33039216, 0.36078432, 0.38112746],
            [0.30588236, 0.32794118, 0.34877452],
            [0.36274511, 0.3764706, 0.38480393],
            [0.34828432, 0.35686275, 0.36568628],
            [0.35318628, 0.38186275, 0.39607844],
            [0.3379902, 0.36078432, 0.3754902],
            [0.37769609, 0.39558825, 0.40000001],
        ];
        let actual = malvar(&input, 4, 2, &DenoiseJob::testing()).unwrap();
        for (i, pixel) in expected.iter().enumerate() {
            for c in 0..3 {
                assert!((actual[c * 8 + i].to_f64() - pixel[c]).abs() < 0.0002);
            }
        }
    }
    #[test]
    fn irregular_last_tiles_blend_constant_data_without_seams() {
        for n in [256, 258, 320, 448, 512, 7028] {
            let a = axis(n);
            let mut sums = vec![0.; n];
            for (t, &start) in a.starts.iter().enumerate() {
                for p in 0..TILE {
                    sums[start + p] += weight(&a, t, p).to_f32();
                }
            }
            assert!(
                sums.iter().all(|v| (v - 1.).abs() < 0.001),
                "{n}: uncovered or double-weighted edge"
            );
        }
    }
    #[test]
    fn restore_keeps_inactive_edge_and_black_white_metadata() {
        let mut raw = sensor("BGGR");
        let before = raw.data.as_f32().into_owned();
        let a = area(&raw).unwrap();
        restore(
            &mut raw,
            a,
            &Matrix3::identity(),
            &vec![f16::from_f32(0.25); 3 * a.w * a.h],
            &DenoiseJob::testing(),
        )
        .unwrap();
        assert_eq!(raw.data.as_f32()[0], before[0]);
        assert_eq!(raw.blacklevel.as_vec(), vec![100., 200., 300., 400.]);
        assert_eq!(raw.whitelevel.0, vec![4100]);
        let b = black(&raw, a.y, a.x);
        let expected = b + 16383. / 65535. * (4100. - b);
        assert!((raw.data.as_f32()[a.y * raw.width + a.x] - expected).abs() < 0.001);
    }
    #[test]
    fn sensor_dng_roundtrip_preserves_cfa_crop_levels_and_orientation() {
        use rawler::imgop::{Dim2, Point, Rect};
        let mut raw = sensor("BGGR");
        raw.clean_make = "RapidRoom".into();
        raw.clean_model = "Test Bayer".into();
        raw.active_area = Some(Rect::new(Point::new(2, 2), Dim2::new(256, 256)));
        raw.crop_area = Some(Rect::new(Point::new(4, 4), Dim2::new(252, 250)));
        raw.color_matrix.insert(
            rawler::imgop::xyz::Illuminant::D65,
            vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
        );
        raw.data = RawImageData::Float(raw.data.as_f32().into_owned());
        let exif = rawler::exif::Exif {
            orientation: Some(6),
            iso_speed: Some(12800),
            ..Default::default()
        };
        let metadata = rawler::decoders::RawMetadata {
            exif,
            make: raw.clean_make.clone(),
            model: raw.clean_model.clone(),
            lens: None,
            unique_image_id: None,
            rating: None,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sensor.dng");
        write_dng(&path, &raw, &metadata).unwrap();
        let bytes = fs::read(path).unwrap();
        let source = RawSource::new_from_slice(&bytes);
        let decoder = rawler::get_decoder(&source).unwrap();
        let restored = decoder
            .raw_image(&source, &RawDecodeParams::default(), false)
            .unwrap();
        assert_eq!(restored.active_area, raw.active_area);
        assert_eq!(restored.crop_area, raw.crop_area);
        assert_eq!(restored.blacklevel.as_vec(), raw.blacklevel.as_vec());
        assert_eq!(restored.whitelevel.0, raw.whitelevel.0);
        assert_eq!(restored.orientation.to_u16(), 6);
        assert_eq!(restored.data.as_f32().as_ref(), raw.data.as_f32().as_ref());
        let RawPhotometricInterpretation::Cfa(config) = restored.photometric else {
            panic!("CFA became RGB");
        };
        assert_eq!(config.cfa.color_at(0, 0), 2);
        assert_eq!(config.cfa.color_at(1, 1), 0);
    }

    #[test]
    fn corrupt_or_wrong_identity_cache_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let pixels = dir.path().join("sensor.dng");
        fs::write(&pixels, b"sensor data").unwrap();
        let record = CacheRecord {
            key: "settings-a".into(),
            hash: blake3::hash(b"sensor data").to_hex().to_string(),
        };
        fs::write(
            dir.path().join("cache.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert!(cache_hit(dir.path(), "settings-a").is_some());
        assert!(cache_hit(dir.path(), "settings-b").is_none());
        fs::write(pixels, b"corrupted sensor data").unwrap();
        assert!(cache_hit(dir.path(), "settings-a").is_none());
    }
    #[test]
    fn cancellation_stops_preprocessing_and_raw_decode_token() {
        let job = DenoiseJob::testing();
        let token = job.decode_cancel_token();
        job.cancel_for_test();
        assert_ne!(token.0.load(std::sync::atomic::Ordering::SeqCst), token.1);
        assert!(malvar(&[0.25; 8], 4, 2, &job).is_err());
    }
    #[test]
    #[ignore = "Local model integration; set RAPIDROOM_TREE_MODELS, under flock"]
    fn real_model_tile_smoke() {
        let root = PathBuf::from(std::env::var("RAPIDROOM_TREE_MODELS").unwrap());
        let input = vec![f16::from_f32(0.2); 3 * TILE * TILE];
        for model in [&BEST, &FAST, &SHARPEN] {
            let session = Arc::new(Mutex::new(
                cpu_session(&root.join(model.file)).unwrap(),
            ));
            let output = infer(
                &input,
                TILE,
                TILE,
                12800,
                &session,
                model.name,
                &DenoiseJob::testing(),
            )
            .unwrap();
            assert_eq!(output.len(), input.len());
            assert!(output.iter().all(|v| v.is_finite()));
            println!("{}: CPU float16 model tile passed", model.name);
            let job = DenoiseJob::testing_cancel_on_progress();
            assert!(
                infer(
                    &vec![f16::from_f32(0.2); 3 * 448 * TILE],
                    448,
                    TILE,
                    12800,
                    &session,
                    model.name,
                    &job
                )
                .is_err()
            );
            assert!(job.is_cancelled());
            println!(
                "{}: cancellation stopped before the second tile",
                model.name
            );
        }
    }

    #[test]
    #[ignore = "Local CPU thread tuning; set RAPIDROOM_TREE_MODELS and RAPIDROOM_TREE_SAMPLES, under flock"]
    fn real_cpu_thread_sweep() {
        let root = PathBuf::from(std::env::var("RAPIDROOM_TREE_MODELS").unwrap());
        let input = vec![f16::from_f32(0.2); 3 * TILE * TILE];
        let mut timings = Vec::new();
        for threads in [1, 2, 4, 8] {
            for model in [&FAST, &BEST, &SHARPEN] {
                let session = Arc::new(Mutex::new(
                    Session::builder()
                        .unwrap()
                        .with_intra_threads(threads)
                        .unwrap()
                        .commit_from_file(root.join(model.file))
                        .unwrap(),
                ));
                let job = DenoiseJob::testing();
                infer(&input, TILE, TILE, 12800, &session, model.name, &job).unwrap();
                let start = std::time::Instant::now();
                for _ in 0..10 {
                    infer(&input, TILE, TILE, 12800, &session, model.name, &job).unwrap();
                }
                let row = serde_json::json!({"threads":threads,"model":model.file,"seconds_per_tile":start.elapsed().as_secs_f64()/10.});
                println!("{row}");
                timings.push(row);
            }
        }
        fs::write(
            PathBuf::from(std::env::var("RAPIDROOM_TREE_SAMPLES").unwrap())
                .join("cpu-threads.json"),
            serde_json::to_vec_pretty(&timings).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[ignore = "Local real-RAW/model integration; set RAPIDROOM_TREE_RAW and RAPIDROOM_TREE_MODELS, under flock"]
    fn real_raw_chain_cache_smoke() {
        let source = std::env::var("RAPIDROOM_TREE_RAW").unwrap();
        let root = PathBuf::from(std::env::var("RAPIDROOM_TREE_MODELS").unwrap());
        let bytes = fs::read(source).unwrap();
        let cache = tempfile::Builder::new()
            .prefix("tree-91-")
            .tempdir_in(std::env::var("RAPIDROOM_TREE_SAMPLES").unwrap())
            .unwrap();
        let fast =
            std::env::var("RAPIDROOM_TREE_PRESET").unwrap_or_else(|_| "fast".into()) == "fast";
        let sharpen =
            std::env::var("RAPIDROOM_TREE_SHARPEN").unwrap_or_else(|_| "on".into()) == "on";
        let mut wanted = vec![if fast { &FAST } else { &BEST }];
        if sharpen {
            wanted.push(&SHARPEN);
        }
        let models = wanted
            .into_iter()
            .map(|m| {
                Arc::new(Mutex::new(
                    cpu_session(&root.join(m.file)).unwrap(),
                ))
            })
            .collect();
        let job = DenoiseJob::testing();
        let started = std::time::Instant::now();
        let path = cached_sensor_at(&bytes, cache.path(), fast, sharpen, &models, &job).unwrap();
        let inference_seconds = started.elapsed().as_secs_f64();
        let started = std::time::Instant::now();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let hit = cached_sensor_at(&bytes, cache.path(), fast, sharpen, &Vec::new(), &job).unwrap();
        assert_eq!(hit, path);
        assert_eq!(fs::metadata(&hit).unwrap().modified().unwrap(), before);
        let cache_seconds = started.elapsed().as_secs_f64();
        assert!(cached_sensor_at(&bytes, cache.path(), !fast, sharpen, &Vec::new(), &job).is_err());
        assert!(cached_sensor_at(&bytes, cache.path(), fast, !sharpen, &Vec::new(), &job).is_err());
        let output = fs::read(path).unwrap();
        let source = RawSource::new_from_slice(&output);
        let decoder = rawler::get_decoder(&source).unwrap();
        let raw = decoder
            .raw_image(&source, &RawDecodeParams::default(), false)
            .unwrap();
        assert!(matches!(raw.data, RawImageData::Float(_)));
        assert!(raw.data.as_f32().iter().all(|v| v.is_finite()));
        let metadata = decoder
            .raw_metadata(&source, &RawDecodeParams::default())
            .unwrap();
        assert!(
            metadata
                .exif
                .iso_speed
                .or(metadata.exif.iso_speed_ratings.map(u32::from))
                .is_some()
        );
        let original_source = RawSource::new_from_slice(&bytes);
        let original = rawler::get_decoder(&original_source)
            .unwrap()
            .raw_image(&original_source, &RawDecodeParams::default(), false)
            .unwrap();
        assert_eq!((raw.width, raw.height), (original.width, original.height));
        assert_eq!(raw.active_area, original.active_area);
        assert_eq!(raw.crop_area, original.crop_area);
        assert_eq!(raw.blacklevel.as_vec(), original.blacklevel.as_vec());
        assert_eq!(raw.whitelevel.0, original.whitelevel.0);
        assert!(
            raw.wb_coeffs
                .iter()
                .zip(original.wb_coeffs)
                .all(|(a, b)| (a.is_nan() && b.is_nan()) || (*a - b).abs() < 0.0001)
        );
        let name = format!(
            "{}-{}",
            if fast { "fast" } else { "best" },
            if sharpen { "sharpen" } else { "plain" }
        );
        let samples = PathBuf::from(std::env::var("RAPIDROOM_TREE_SAMPLES").unwrap());
        fs::write(samples.join(format!("{name}.dng")), output).unwrap();
        let report = serde_json::json!({"preset":name,"inference_seconds":inference_seconds,"cache_seconds":cache_seconds,"shape":[raw.width,raw.height],"matrix":colour_matrix(&original).unwrap().as_slice(),"metadata_preserved":true,"cache_hit":true,"setting_invalidation":true});
        fs::write(
            samples.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{report}");
    }
}
