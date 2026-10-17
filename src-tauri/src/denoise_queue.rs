//! Persistent marks and crash-safe, content-addressed denoise results, shared by CLI and UI.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

const PIPELINE: &str = "rapidroom-denoise-queue-v3";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Options {
    pub method: String,
    pub strength: f32,
    pub sharpen: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            method: "best".into(),
            strength: 50.,
            sharpen: false,
        }
    }
}
impl Options {
    fn internal(&self) -> Result<DenoiseOptions, String> {
        let method = match self.method.as_str() {
            "best" => "tree_best",
            "fast" => "tree_fast",
            "nind" => "ai",
            "bm3d" => "bm3d",
            "raw9" if cfg!(target_os = "macos") => "raw9",
            _ => return Err("Method must be best, fast, nind, bm3d or raw9 (macOS only)".into()),
        };
        if !self.strength.is_finite() || !(0. ..=100.).contains(&self.strength) {
            return Err("Strength must be between 0 and 100".into());
        }
        if self.sharpen && !matches!(self.method.as_str(), "best" | "fast") {
            return Err("Sharpen is supported only by best and fast".into());
        }
        Ok(DenoiseOptions {
            method: method.into(),
            intensity: if matches!(self.method.as_str(), "best" | "fast") {
                1.0
            } else {
                self.strength / 100.0
            },
            sharpen: self.sharpen,
            exposure_policies: HashMap::new(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Cli {
    pub folder: String,
    pub recursive: bool,
    pub dry_run: bool,
    pub options: Options,
}

pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut result = Cli {
        folder: String::new(),
        recursive: false,
        dry_run: false,
        options: Options::default(),
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--marked" if result.folder.is_empty() => {
                result.folder = iter.next().ok_or("Missing folder after --marked")?.clone()
            }
            "--recursive" => result.recursive = true,
            "--dry-run" => result.dry_run = true,
            "--sharpen" => result.options.sharpen = true,
            "--method" => {
                result.options.method = iter.next().ok_or("Missing --method value")?.clone()
            }
            "--strength" => {
                result.options.strength = iter
                    .next()
                    .ok_or("Missing --strength value")?
                    .parse()
                    .map_err(|_| "Invalid --strength")?
            }
            _ => return Err(format!("Unknown denoise argument: {arg}")),
        }
    }
    if result.folder.is_empty() {
        return Err("Usage: rapidroom denoise --marked <folder> [--recursive] [--method best|fast|nind|bm3d|raw9] [--strength 0..100] [--sharpen] [--dry-run]".into());
    }
    result.options.internal()?;
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Receipt {
    key: String,
    source_hash: String,
    identity: Value,
    output: String,
    output_hash: String,
    edits_hash: String,
    completed_at: String,
}

fn strict_json(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return serde_json::to_value(crate::image_processing::ImageMetadata::default())
            .map_err(|e| e.to_string());
    }
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid sidecar {}: {e}", path.display()))?;
    if !value.is_object() || (!value["adjustments"].is_null() && !value["adjustments"].is_object())
    {
        return Err(format!(
            "Invalid sidecar object or adjustments: {}",
            path.display()
        ));
    }
    // Validate edit schema too; never replace unreadable edits with defaults.
    serde_json::from_value::<crate::image_processing::ImageMetadata>(value.clone())
        .map_err(|e| format!("Invalid sidecar {}: {e}", path.display()))?;
    Ok(value)
}

fn mutate(
    path: &Path,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<(), String> {
    crate::file_management::ensure_card_writable_for_paths(&[path.to_string_lossy().as_ref()])?;
    let _guard = crate::file_management::lock_sidecar(path).map_err(|e| e.to_string())?;
    let mut value = strict_json(path)?;
    change(&mut value)?;
    crate::file_management::write_file_atomically_unmerged(
        path,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn queue(value: &Value) -> Result<Value, String> {
    match value.get("denoise") {
        None | Some(Value::Null) => Ok(json!({ "version":1, "marked":false, "completed":[] })),
        Some(q) if q["version"] == 1 && q["marked"].is_boolean() && q["completed"].is_array() => {
            Ok(q.clone())
        }
        _ => Err("Unsupported denoise sidecar version or invalid queue metadata".into()),
    }
}

#[tauri::command]
pub fn set_denoise_mark(paths: Vec<String>, marked: bool) -> Result<(), String> {
    crate::file_management::ensure_card_writable_for_paths(&paths)?;
    for path in paths {
        let (source, sidecar) = parse_virtual_path(&path);
        if !source.is_file() {
            return Err(format!("Missing source: {}", source.display()));
        }
        mutate(&sidecar, |value| {
            let mut q = queue(value)?;
            q["marked"] = marked.into();
            value["denoise"] = q;
            Ok(())
        })?;
    }
    Ok(())
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkStatus {
    pub marked: bool,
    pub denoised: bool,
}

#[tauri::command]
pub async fn denoise_mark_status(
    paths: Vec<String>,
) -> Result<HashMap<String, MarkStatus>, String> {
    tokio::task::spawn_blocking(move || mark_status(paths))
        .await
        .map_err(|e| e.to_string())
}

fn mark_status(paths: Vec<String>) -> HashMap<String, MarkStatus> {
    paths
        .into_iter()
        .map(|path| {
            let (_, sidecar) = parse_virtual_path(&path);
            let status = strict_json(&sidecar)
                .and_then(|v| queue(&v))
                .map(|q| {
                    let denoised = q["completed"].as_array().is_some_and(|rs| {
                        rs.iter().any(|r| {
                            r["output"]
                                .as_str()
                                .and_then(|name| safe_output(&sidecar, name).ok())
                                .is_some_and(|p| p.is_file())
                        })
                    });
                    MarkStatus {
                        marked: q["marked"] == true,
                        denoised,
                    }
                })
                .unwrap_or_default();
            (path, status)
        })
        .collect()
}

pub fn discover(folder: &str, recursive: bool) -> Result<Vec<String>, String> {
    let root = Path::new(folder);
    if !root.is_dir() {
        return Err("Marked folder is not a directory".into());
    }
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(if recursive { usize::MAX } else { 1 })
    {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|e| e.to_str()) != Some("rrdata")
        {
            continue;
        }
        let value = strict_json(entry.path())?;
        if queue(&value)?["marked"] != true {
            continue;
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or("Non-UTF8 sidecar filename")?
            .strip_suffix(".rrdata")
            .unwrap();
        let direct = entry.path().with_file_name(name);
        let path = if direct.is_file() {
            direct.to_string_lossy().into_owned()
        } else {
            let (base, vc) = name
                .rsplit_once('.')
                .ok_or("Missing source for marked sidecar")?;
            let source = entry.path().with_file_name(base);
            if !source.is_file() {
                return Err(format!("Missing source for {}", entry.path().display()));
            }
            format!("{}?vc={vc}", source.to_string_lossy())
        };
        result.push(path);
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 128 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
fn hash_value(value: &Value) -> String {
    blake3::hash(&serde_json::to_vec(value).expect("JSON serialization"))
        .to_hex()
        .to_string()
}
fn edits(mut metadata: Value) -> Value {
    metadata.as_object_mut().unwrap().remove("denoise");
    metadata
}
fn edit_hash(metadata: &Value) -> String {
    let mut value = metadata.clone();
    value.as_object_mut().unwrap().remove("exif");
    for key in crate::file_management::SIDECAR_OWNED_FIELDS {
        value.as_object_mut().unwrap().remove(key);
    }
    hash_value(&value)
}
fn safe_output(sidecar: &Path, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err("Invalid denoise output filename".into());
    }
    Ok(sidecar.with_file_name(name))
}
fn receipt_valid(receipt: &Receipt, sidecar: &Path) -> bool {
    safe_output(sidecar, &receipt.output)
        .and_then(|p| hash_file(&p))
        .is_ok_and(|h| h == receipt.output_hash)
}

fn source_baseline_eligible(adjustments: &Value) -> bool {
    adjustments.is_null() || adjustments["baselineExposure"].as_bool().unwrap_or(false)
}

fn ensure_rendered_eligible(metadata: &Value, source: &Path, ident: &Value) -> Result<(), String> {
    if !is_raw_file(source) || matches!(ident["method"].as_str(), Some("best" | "fast")) {
        return Ok(());
    }
    let adjustments = &metadata["adjustments"];
    if ident["method"] == "nind"
        && let Some(expected) = ident["sourceBaselineEligible"].as_bool()
        && expected != source_baseline_eligible(adjustments)
    {
        return Err(
            "Source baseline setting changed during denoise; run the marked queue again.".into(),
        );
    }
    if ident["method"] == "nind"
        && let Some(expected) = ident.get("sourceExposurePolicy")
        && expected != &nind_raw_defaults(adjustments)
    {
        return Err(
            "Source exposure policy changed during denoise; run the marked queue again.".into(),
        );
    }
    let override_mode = (ident["settings"]["toneOverride"] == true).then(|| {
        if ident["settings"]["rawTone"].as_str().unwrap_or("agx") == "agx" {
            1
        } else {
            0
        }
    });
    let cropped = adjustments.get("crop").is_some_and(|c| !c.is_null());
    if cropped
        || !adjustments["whiteBalance"].is_null()
        || crate::image_processing::is_image_edited(adjustments, true, override_mode)
    {
        return Err("Edited photo: use Fast or Best, or reset edits first. Rendered-method RAW edit baking is not supported yet.".into());
    }
    if ident["settings"]["toneOverride"] == true && ident["settings"]["nonRawTone"] == "agx" {
        return Err("Rendered-image AgX override would tone-map this TIFF again; use Fast or Best, or select Basic for rendered images in Processing settings.".into());
    }
    Ok(())
}

fn save_result_edits(
    source: &Path,
    source_image: &Path,
    output: &Path,
    ident: &Value,
    previous: Option<&Receipt>,
) -> Result<String, String> {
    let original = strict_json(source)?;
    ensure_rendered_eligible(&original, source_image, ident)?;
    let mut source_edits = edits(original);
    if source_edits.get("exif").is_none_or(Value::is_null)
        && let Some(exif) = crate::exif_processing::read_rrexif_sidecar(source_image)
    {
        source_edits["exif"] = json!(exif);
    }
    if is_raw_file(source_image) && !matches!(ident["method"].as_str(), Some("best" | "fast")) {
        // These pixels already contain default RAW development/tone and camera WB.
        // Only unedited RAW is eligible; never replay its RAW defaults on encoded TIFF.
        source_edits["adjustments"] = json!({"toneMapper":"basic","baselineExposure":false});
        if let Some(exif) = source_edits.get_mut("exif").and_then(Value::as_object_mut) {
            exif.insert("Orientation".into(), "1".into());
        }
    }
    let (_, dest) = parse_virtual_path(&output.to_string_lossy());
    let _guard = crate::file_management::lock_sidecar(&dest).map_err(|e| e.to_string())?;
    // The destination ownership check and write are one transaction with ordinary saves.
    if dest.exists() {
        let existing = strict_json(&dest)?;
        if source_edits.get("exif").is_none_or(Value::is_null)
            && let Some(exif) = existing.get("exif")
        {
            source_edits["exif"] = exif.clone();
        }
        if edit_hash(&existing) != edit_hash(&source_edits)
            && !previous.is_some_and(|r| edit_hash(&existing) == r.edits_hash)
        {
            return Err("Denoised output has independent edits; keeping them. Resolve those edits before synchronizing.".into());
        }
        crate::file_management::preserve_sidecar_owned_fields(
            &existing,
            source_edits.as_object_mut().unwrap(),
        );
    }
    crate::file_management::write_file_atomically_unmerged(
        &dest,
        serde_json::to_vec_pretty(&source_edits).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(edit_hash(&source_edits))
}

fn record(sidecar: &Path, receipt: &Receipt) -> Result<(), String> {
    mutate(sidecar, |value| {
        let mut q = queue(value)?;
        q.as_object_mut().unwrap().remove("last_error");
        let rows = q["completed"].as_array_mut().unwrap();
        rows.retain(|row| row["key"] != receipt.key);
        rows.push(serde_json::to_value(receipt).map_err(|e| e.to_string())?);
        if rows.len() > 32 {
            rows.remove(0);
        }
        value["denoise"] = q;
        Ok(())
    })
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub errors: Vec<String>,
    pub outputs: Vec<String>,
}

fn publish(
    output: DenoisedOutput,
    path: &Path,
    raw: bool,
    job: &DenoiseJob,
    before_publish: impl FnOnce(&str) -> Result<(), String>,
) -> Result<String, String> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
    match output {
        DenoisedOutput::Raw(bytes) => temp.write_all(&bytes).map_err(|e| e.to_string())?,
        DenoisedOutput::Image(image) => {
            let (image, format) = if raw {
                (
                    DynamicImage::ImageRgb16(image.to_rgb16()),
                    ImageFormat::Tiff,
                )
            } else {
                (DynamicImage::ImageRgb8(image.to_rgb8()), ImageFormat::Png)
            };
            image
                .save_with_format(temp.path(), format)
                .map_err(|e| e.to_string())?;
        }
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    job.check()?;
    let hash = hash_file(temp.path())?;
    before_publish(&hash)?;
    job.check()?;
    temp.persist_noclobber(path)
        .map_err(|e| format!("Cannot publish without overwriting {}: {e}", path.display()))?;
    Ok(hash)
}

fn identity(models: &DenoiseModels, options: &Options, settings: &Value) -> Value {
    let profiles = match models {
        DenoiseModels::Tree { sessions, .. } => sessions
            .iter()
            .map(|s| s.profile.clone())
            .collect::<Vec<_>>(),
        DenoiseModels::LegacyAi(_) => vec![format!("CPU:{}", ort::info())],
        DenoiseModels::Basic => vec!["CPU".into()],
    };
    let model = match options.method.as_str() {
        "best" => crate::tree_denoise::BEST.sha256,
        "fast" => crate::tree_denoise::FAST.sha256,
        "nind" => crate::ai_processing::DENOISE_SHA256,
        _ => "builtin",
    };
    json!({ "pipeline":PIPELINE, "treePipeline":crate::tree_denoise::PIPELINE, "method":options.method, "strength":if matches!(options.method.as_str(),"best"|"fast") {0.} else {options.strength}, "sharpen":options.sharpen, "model":model, "sharpenModel":options.sharpen.then_some(crate::tree_denoise::SHARPEN.sha256), "runtime":profiles, "settings":settings })
}

fn processing_identity(settings: &crate::app_settings::AppSettings, options: &Options) -> Value {
    if matches!(options.method.as_str(), "best" | "fast") {
        return json!({});
    }
    // Whitelist processing controls; never persist library paths, terminal settings or credentials.
    json!({"rawHighlightCompression":settings.raw_highlight_compression.unwrap_or(2.5),
        "linearRawMode":settings.linear_raw_mode,
        "colorNR":settings.raw_preprocessing_color_nr.unwrap_or(0.5),
        "preSharpen":settings.raw_preprocessing_sharpening.unwrap_or(0.35),
        "applyToNonRaws":settings.apply_preprocessing_to_non_raws.unwrap_or(false),
        "appleRaw9":settings.use_apple_raw9.unwrap_or(false),
        "baselineEnabled":settings.enable_baseline_exposure,"baselineExposure":settings.baseline_exposure,
        "highlightCompensation":settings.compensate_highlight_preservation,
        "toneOverride":settings.tonemapper_override_enabled.unwrap_or(false),
        "rawTone":settings.default_raw_tonemapper,"nonRawTone":settings.default_non_raw_tonemapper})
}

fn require_models(app: &AppHandle, options: &Options) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let mut wanted = Vec::new();
    match options.method.as_str() {
        "best" => wanted.push((
            crate::tree_denoise::BEST.file,
            crate::tree_denoise::BEST.sha256,
        )),
        "fast" => wanted.push((
            crate::tree_denoise::FAST.file,
            crate::tree_denoise::FAST.sha256,
        )),
        "nind" => wanted.push((
            crate::ai_processing::DENOISE_FILENAME,
            crate::ai_processing::DENOISE_SHA256,
        )),
        _ => {}
    }
    if options.sharpen {
        wanted.push((
            crate::tree_denoise::SHARPEN.file,
            crate::tree_denoise::SHARPEN.sha256,
        ));
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let dir = crate::ai_processing::get_models_dir(app).map_err(|e| e.to_string())?;
    for (name, hash) in wanted {
        let mut f = File::open(dir.join(name)).map_err(|_| {
            format!(
                "Install the {name} model in the Denoise dialog before running an unattended queue"
            )
        })?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 128 * 1024];
        loop {
            let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            digest.update(&buffer[..n]);
        }
        if hex::encode(digest.finalize()) != hash {
            return Err(format!("Model checksum mismatch: {name}"));
        }
    }
    Ok(())
}

pub async fn run(
    job_id: u64,
    paths: Vec<String>,
    options: Options,
    app: AppHandle,
) -> Result<Summary, String> {
    let state = app.state::<AppState>();
    let Some(job) = state.denoise_jobs.start(job_id, Some(event_sink(&app))) else {
        return Ok(Summary {
            cancelled: true,
            ..Default::default()
        });
    };
    let outcome = async {
        let internal = options.internal()?;
        crate::file_management::ensure_card_writable_for_paths(&paths)?;
        if paths.is_empty() {
            return Ok(Summary::default());
        }
        let frozen_settings = load_settings(app.clone())?;
        if frozen_settings.ai_provider.as_deref() == Some("ai-free")
            && matches!(options.method.as_str(), "best" | "fast" | "nind")
        {
            return Err("AI-Free mode allows only Basic or Apple RAW 9 denoise".into());
        }
        require_models(&app, &options)?;
        let models = match options.method.as_str() {
            "best" | "fast" => DenoiseModels::Tree {
                sessions: crate::tree_denoise::cached_models(
                    &app,
                    &state,
                    options.method == "fast",
                    options.sharpen,
                    &job,
                )
                .await
                .map_err(|e| e.to_string())?,
                fast: options.method == "fast",
                sharpen: options.sharpen,
            },
            "nind" => DenoiseModels::LegacyAi(
                crate::ai_processing::cached_denoise_model(
                    &app,
                    &state.ai_state,
                    &state.ai_init_lock,
                )
                .await
                .map_err(|e| e.to_string())?,
            ),
            _ => DenoiseModels::Basic,
        };
        let settings = processing_identity(&frozen_settings, &options);
        let ident = identity(&models, &options, &settings);
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            #[cfg(unix)]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            let mut summary = Summary::default();
            for (index, path) in paths.iter().enumerate() {
                if job.is_cancelled() {
                    summary.cancelled = true;
                    break;
                }
                job.emit(
                    "denoise-queue-progress",
                    json!({"current":index+1,"total":paths.len(),"path":path,"summary":summary}),
                );
                let result = run_item(path, &ident, &job, |item_identity| {
                    denoise_image_with_settings(
                        path,
                        &internal,
                        &app,
                        models.clone(),
                        &job,
                        frozen_settings.clone(),
                        item_identity.get("sourceExposurePolicy").cloned(),
                    )
                    .map(|(image, _, raw)| {
                        raw.map_or_else(|| DenoisedOutput::Image(image), DenoisedOutput::Raw)
                    })
                });
                match result {
                    Ok((output, skipped)) => {
                        if skipped {
                            summary.skipped += 1;
                        } else {
                            summary.completed += 1;
                        }
                        summary.outputs.push(output);
                    }
                    Err(e) if e == DENOISE_CANCELLED => {
                        summary.cancelled = true;
                        break;
                    }
                    Err(e) => {
                        summary.failed += 1;
                        let (_, sidecar) = parse_virtual_path(path);
                        let _ = mutate(&sidecar, |v| {
                            let mut q = queue(v)?;
                            q["last_error"] =
                                json!({"message":e,"at":chrono::Utc::now().to_rfc3339()});
                            v["denoise"] = q;
                            Ok(())
                        });
                        summary.errors.push(format!("{path}: {e}"));
                    }
                }
                job.emit(
                    "denoise-queue-progress",
                    json!({"current":index+1,"total":paths.len(),"path":path,"summary":summary}),
                );
            }
            summary
        })
        .await
        .map_err(|e| e.to_string())
    }
    .await;
    state.denoise_jobs.finish(job_id);
    match outcome {
        Err(error) if error == DENOISE_CANCELLED => Ok(Summary {
            cancelled: true,
            ..Default::default()
        }),
        other => other,
    }
}

fn run_item(
    path: &str,
    ident: &Value,
    job: &DenoiseJob,
    denoise: impl FnOnce(&Value) -> Result<DenoisedOutput, String>,
) -> Result<(String, bool), String> {
    let (source, sidecar) = parse_virtual_path(path);
    if source.is_symlink() {
        return Err(
            "Linked sources are not supported by the denoise queue; mark the original photo".into(),
        );
    }
    let source = fs::canonicalize(source).map_err(|e| e.to_string())?;
    // Lock the canonical source, independently of the short metadata lock.
    let claim = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(source.with_file_name(format!(
            ".{}.denoise-lock",
            source.file_name().unwrap().to_string_lossy()
        )))
        .map_err(|e| e.to_string())?;
    claim
        .try_lock()
        .map_err(|_| "Source is already being denoised by another queue".to_string())?;
    let metadata = strict_json(&sidecar)?;
    let mut item_identity = ident.clone();
    if is_raw_file(&source) && ident["method"] == "nind" {
        item_identity["sourceBaselineEligible"] =
            source_baseline_eligible(&metadata["adjustments"]).into();
        item_identity["sourceExposurePolicy"] = nind_raw_defaults(&metadata["adjustments"]);
    }
    let ident = &item_identity;
    ensure_rendered_eligible(&metadata, &source, ident)?;
    let q = queue(&metadata)?;
    if q["marked"] != true {
        return Err("Photo is no longer marked for denoise".into());
    }
    let source_hash = hash_file(&source)?;
    let key =
        hash_value(&json!({"source":source_hash,"sidecar":sidecar.file_name(),"identity":ident}));
    let previous = q["completed"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| serde_json::from_value::<Receipt>(r.clone()).ok())
        .find(|r| r.key == key);
    if let Some(mut receipt) = previous.clone().filter(|r| receipt_valid(r, &sidecar)) {
        let output = safe_output(&sidecar, &receipt.output)?;
        receipt.edits_hash = save_result_edits(&sidecar, &source, &output, ident, Some(&receipt))?;
        record(&sidecar, &receipt)?;
        return Ok((output.to_string_lossy().into_owned(), true));
    }
    let raw = is_raw_file(&source);
    if matches!(ident["method"].as_str(), Some("best" | "fast")) && !raw {
        return Err(
            "Best and Fast require a Bayer RAW source; choose another method for rendered photos"
                .into(),
        );
    }
    let ext = if matches!(ident["method"].as_str(), Some("best" | "fast")) {
        "dng"
    } else if raw {
        "tiff"
    } else {
        "png"
    };
    let output = source.with_file_name(format!(
        "{}_Denoised_{}.{}",
        source.file_name().unwrap().to_string_lossy(),
        key,
        ext
    ));
    let provenance = output.with_extension(format!("{ext}.rrqueue"));
    if output.exists() {
        let mut receipt: Receipt = serde_json::from_slice(
            &fs::read(&provenance).map_err(|_| "Unverified denoise output exists; keeping it")?,
        )
        .map_err(|e| e.to_string())?;
        if receipt.key != key || !receipt_valid(&receipt, &sidecar) {
            return Err(
                "Existing denoise output failed provenance/hash validation; keeping it".into(),
            );
        }
        receipt.edits_hash = save_result_edits(&sidecar, &source, &output, ident, Some(&receipt))?;
        record(&sidecar, &receipt)?;
        return Ok((output.to_string_lossy().into_owned(), true));
    }
    let staged_receipt = fs::read(&provenance)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .filter(|r| r.key == key);
    job.check()?;
    let image = denoise(ident)?;
    job.check()?;
    if hash_file(&source)? != source_hash {
        return Err("Source changed during denoise; result discarded".into());
    }
    let mut committed = None;
    publish(image, &output, raw, job, |hash| {
        let edits_hash = save_result_edits(
            &sidecar,
            &source,
            &output,
            ident,
            previous.as_ref().or(staged_receipt.as_ref()),
        )?;
        let receipt = Receipt {
            key,
            source_hash,
            identity: ident.clone(),
            output: output.file_name().unwrap().to_string_lossy().into_owned(),
            output_hash: hash.into(),
            edits_hash,
            completed_at: chrono::Utc::now().to_rfc3339(),
        };
        crate::file_management::write_file_atomically(
            &provenance,
            serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        committed = Some(receipt);
        Ok(())
    })?;
    record(&sidecar, &committed.unwrap())?;
    Ok((output.to_string_lossy().into_owned(), false))
}

#[tauri::command]
pub async fn run_denoise_queue(
    job_id: u64,
    paths: Vec<String>,
    options: Options,
    app: AppHandle,
) -> Result<Summary, String> {
    run(job_id, paths, options, app).await
}

pub async fn headless(session: Cli, app: AppHandle) -> i32 {
    #[cfg(unix)]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    let paths = match discover(&session.folder, session.recursive) {
        Ok(p) => p,
        Err(e) => {
            cli_eprintln!("Denoise: {e}");
            return 1;
        }
    };
    cli_println!("Denoise: {} marked photos", paths.len());
    if session.dry_run {
        for p in paths {
            cli_println!("Marked: {p}");
        }
        return 0;
    }
    use tauri::Listener;
    let progress = app.listen("denoise-queue-progress", |event| {
        cli_println!("Denoise progress: {}", event.payload());
    });
    let job_id = app.state::<AppState>().denoise_jobs.create();
    let run = run(job_id, paths, session.options, app.clone());
    tokio::pin!(run);
    let result = tokio::select! {
        result = &mut run => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() { app.state::<AppState>().denoise_jobs.cancel(job_id); }
            run.await
        }
    };
    app.unlisten(progress);
    match result {
        Ok(s) => {
            for e in &s.errors {
                cli_eprintln!("Denoise error: {e}");
            }
            cli_println!(
                "Denoise: {} completed, {} skipped, {} failed{}",
                s.completed,
                s.skipped,
                s.failed,
                if s.cancelled { ", cancelled" } else { "" }
            );
            if s.cancelled {
                130
            } else if s.failed > 0 {
                1
            } else {
                0
            }
        }
        Err(e) => {
            cli_eprintln!("Denoise failed: {e}");
            if e == DENOISE_CANCELLED { 130 } else { 1 }
        }
    }
}

#[tauri::command]
pub async fn denoise_queue_paths(folder: String, recursive: bool) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || discover(&folder, recursive))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        DynamicImage::new_rgb8(8, 6).save(&source).unwrap();
        let path = source.to_string_lossy().into_owned();
        set_denoise_mark(vec![path.clone()], true).unwrap();
        let (_, sidecar) = parse_virtual_path(&path);
        mutate(&sidecar, |v| {
            v["adjustments"] = json!({"exposure":0.5,"crop":{"x":1,"y":1,"width":4,"height":3}});
            v["rating"] = 4.into();
            Ok(())
        })
        .unwrap();
        (dir, path)
    }
    fn output(_: &Value) -> Result<DenoisedOutput, String> {
        Ok(DenoisedOutput::Image(DynamicImage::new_rgb8(8, 6)))
    }
    fn ident() -> Value {
        json!({"method":"bm3d","pipeline":PIPELINE,"strength":15})
    }

    #[test]
    fn interrupted_five_photo_queue_skips_finished_and_resumes_without_rewriting() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for n in 0..5 {
            let source = dir.path().join(format!("photo{n}.png"));
            DynamicImage::new_rgb8(8, 6).save(&source).unwrap();
            paths.push(source.to_string_lossy().into_owned());
        }
        set_denoise_mark(paths.clone(), true).unwrap();
        let job = DenoiseJob::testing();
        let mut hashes = Vec::new();
        for path in &paths[..2] {
            let (out, skipped) = run_item(path, &ident(), &job, output).unwrap();
            assert!(!skipped);
            hashes.push(hash_file(Path::new(&out)).unwrap());
        }
        job.cancel_for_test();
        assert_eq!(
            run_item(&paths[2], &ident(), &job, output).unwrap_err(),
            DENOISE_CANCELLED
        );
        let job = DenoiseJob::testing();
        let mut calls = 0;
        for (n, path) in paths.iter().enumerate() {
            let (out, skipped) = run_item(path, &ident(), &job, |item_identity| {
                calls += 1;
                output(item_identity)
            })
            .unwrap();
            assert_eq!(skipped, n < 2);
            if n < 2 {
                assert_eq!(hash_file(Path::new(&out)).unwrap(), hashes[n]);
            }
        }
        assert_eq!(calls, 3);
        assert_eq!(
            discover(dir.path().to_str().unwrap(), false).unwrap().len(),
            5
        );
    }

    #[test]
    fn completion_preserves_latest_edits_and_queue_survives_stale_editor_write() {
        let (_dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let stale = fs::read(&sidecar).unwrap();
        let (out, _) = run_item(&path, &ident(), &DenoiseJob::testing(), output).unwrap();
        crate::file_management::write_file_atomically(&sidecar, stale).unwrap();
        assert_eq!(
            queue(&strict_json(&sidecar).unwrap()).unwrap()["completed"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        mutate(&sidecar, |v| {
            v["adjustments"]["exposure"] = 1.25.into();
            Ok(())
        })
        .unwrap();
        assert!(
            run_item(&path, &ident(), &DenoiseJob::testing(), |_| panic!(
                "must skip"
            ))
            .unwrap()
            .1
        );
        let (_, dest) = parse_virtual_path(&out);
        let saved = strict_json(&dest).unwrap();
        assert_eq!(saved["adjustments"]["exposure"], 1.25);
        assert_eq!(saved["rating"], 4);
        assert!(saved.get("denoise").is_none());
        mutate(&dest, |v| {
            v["adjustments"]["exposure"] = 3.into();
            Ok(())
        })
        .unwrap();
        assert!(
            run_item(&path, &ident(), &DenoiseJob::testing(), output)
                .unwrap_err()
                .contains("independent edits")
        );
        assert_eq!(strict_json(&dest).unwrap()["adjustments"]["exposure"], 3);
    }

    #[test]
    fn recovery_relinks_published_output_after_source_receipt_was_not_committed() {
        let (_dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let (out, _) = run_item(&path, &ident(), &DenoiseJob::testing(), output).unwrap();
        mutate(&sidecar, |v| {
            v["denoise"]["completed"] = json!([]);
            Ok(())
        })
        .unwrap();
        let (recovered, skipped) = run_item(&path, &ident(), &DenoiseJob::testing(), |_| {
            panic!("must recover")
        })
        .unwrap();
        assert_eq!(recovered, out);
        assert!(skipped);
        assert_eq!(
            queue(&strict_json(&sidecar).unwrap()).unwrap()["completed"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fs::write(&out, b"tampered").unwrap();
        assert!(
            run_item(&path, &ident(), &DenoiseJob::testing(), output)
                .unwrap_err()
                .contains("validation")
        );
        assert_eq!(fs::read(out).unwrap(), b"tampered");
    }

    #[test]
    fn options_source_changes_missing_outputs_and_concurrent_claims_are_not_skipped() {
        let (_dir, path) = fixture();
        let job = DenoiseJob::testing();
        let (first, _) = run_item(&path, &ident(), &job, output).unwrap();
        let mut changed = ident();
        changed["strength"] = 30.into();
        let (second, skip) = run_item(&path, &changed, &job, output).unwrap();
        assert!(!skip);
        assert_ne!(first, second);
        fs::remove_file(&second).unwrap();
        assert!(!run_item(&path, &changed, &job, output).unwrap().1);
        fs::write(&path, b"changed source").unwrap();
        assert!(!run_item(&path, &ident(), &job, output).unwrap().1);
        let source = fs::canonicalize(&path).unwrap();
        let name = format!(
            ".{}.denoise-lock",
            source.file_name().unwrap().to_str().unwrap()
        );
        let claim = File::open(source.with_file_name(name)).unwrap();
        claim.lock().unwrap();
        assert!(
            run_item(&path, &ident(), &job, |_| panic!("must not run"))
                .unwrap_err()
                .contains("another queue")
        );
    }

    #[test]
    fn corrupt_sidecar_and_changing_source_do_not_publish_or_erase_edits() {
        let (dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let error = run_item(&path, &ident(), &DenoiseJob::testing(), |item_identity| {
            fs::write(&path, b"changed").unwrap();
            output(item_identity)
        })
        .unwrap_err();
        assert!(error.contains("Source changed"));
        assert!(!fs::read_dir(dir.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("_Denoised_")
        }));
        fs::write(&sidecar, b"broken json").unwrap();
        assert!(set_denoise_mark(vec![path.clone()], false).is_err());
        assert_eq!(fs::read(&sidecar).unwrap(), b"broken json");
        let malformed = json!({"version":1,"rating":4,"adjustments":["invalid"]}).to_string();
        fs::write(&sidecar, &malformed).unwrap();
        assert!(set_denoise_mark(vec![path], false).is_err());
        assert_eq!(fs::read_to_string(sidecar).unwrap(), malformed);
    }

    #[test]
    fn queue_preset_framing_and_stale_saves_share_one_destination_transaction() {
        let (_dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let (out, _) = run_item(&path, &ident(), &DenoiseJob::testing(), output).unwrap();
        let (_, destination) = parse_virtual_path(&out);
        let stale = strict_json(&sidecar).unwrap();
        let start = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                start.wait();
                for revision in 0..12 {
                    crate::file_management::update_applied_preset(
                        &sidecar,
                        json!({"id":"source", "revision":revision}),
                    )
                    .unwrap();
                }
            });
            scope.spawn(|| {
                start.wait();
                for revision in 0..12 {
                    crate::file_management::update_export_framing(&sidecar, |_| {
                        Ok(json!({"instagram":{"revision":revision}}))
                    })
                    .unwrap();
                }
            });
            scope.spawn(|| {
                start.wait();
                for revision in 0..12 {
                    set_denoise_mark(vec![path.clone()], revision % 2 == 1).unwrap();
                }
            });
            start.wait();
            for _ in 0..12 {
                crate::file_management::write_file_atomically(
                    &sidecar,
                    serde_json::to_vec(&stale).unwrap(),
                )
                .unwrap();
            }
        });
        let saved = strict_json(&sidecar).unwrap();
        assert_eq!(saved["appliedPreset"]["revision"], 11);
        assert_eq!(saved["exportFraming"]["instagram"]["revision"], 11);
        assert_eq!(saved["denoise"]["marked"], true);
        assert_eq!(saved["denoise"]["completed"].as_array().unwrap().len(), 1);
        assert_eq!(saved["adjustments"], stale["adjustments"]);

        // A derived photo's framing/preset belong to that destination. Queue
        // resynchronization may update source edits but cannot undo those choices.
        crate::file_management::update_applied_preset(&destination, Value::Null).unwrap();
        crate::file_management::update_export_framing(&destination, |_| Ok(Value::Null)).unwrap();
        set_denoise_mark(vec![out.clone()], true).unwrap();
        mutate(&sidecar, |v| {
            v["adjustments"]["exposure"] = 1.25.into();
            Ok(())
        })
        .unwrap();
        assert!(
            run_item(&path, &ident(), &DenoiseJob::testing(), |_| panic!(
                "already denoised"
            ))
            .unwrap()
            .1
        );
        let target = strict_json(&destination).unwrap();
        assert_eq!(target["adjustments"]["exposure"], 1.25);
        assert_eq!(target.get("appliedPreset"), Some(&Value::Null));
        assert_eq!(target.get("exportFraming"), Some(&Value::Null));
        assert_eq!(target["denoise"]["marked"], true);
        assert!(
            target["denoise"]["completed"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        crate::file_management::write_file_atomically(
            &destination,
            serde_json::to_vec(&saved).unwrap(),
        )
        .unwrap();
        let target = strict_json(&destination).unwrap();
        assert_eq!(target.get("appliedPreset"), Some(&Value::Null));
        assert_eq!(target.get("exportFraming"), Some(&Value::Null));
        assert!(
            target["denoise"]["completed"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let ordinary = destination.with_file_name("legacy.RRDATA");
        let bytes = b"{ \"adjustments\": null, \"rating\": 2 }\n";
        crate::file_management::write_file_atomically(&ordinary, bytes).unwrap();
        assert_eq!(fs::read(&ordinary).unwrap(), bytes);
        crate::file_management::write_file_atomically(&ordinary, b"older non-JSON sidecar")
            .unwrap();
        assert_eq!(fs::read(ordinary).unwrap(), b"older non-JSON sidecar");
    }

    #[test]
    fn legacy_save_and_batch_do_not_inherit_the_source_queue_ownership() {
        let (_dir, path) = fixture();
        let identity = ident();
        let (_, source_sidecar) = parse_virtual_path(&path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source_sidecar, fs::Permissions::from_mode(0o600)).unwrap();
        }
        run_item(&path, &identity, &DenoiseJob::testing(), output).unwrap();
        let results = run_batch(&DenoiseJob::testing(), std::slice::from_ref(&path), |_| {
            output(&identity)
        });
        assert_eq!(results.len(), 1);
        let (_, dest) = parse_virtual_path(&results[0]);
        assert!(strict_json(&dest).unwrap().get("denoise").is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            discover(Path::new(&path).parent().unwrap().to_str().unwrap(), false).unwrap(),
            vec![path.clone()]
        );
        set_denoise_mark(results, true).unwrap();
        mutate(&source_sidecar, |v| {
            v["adjustments"]["exposure"] = 1.25.into();
            Ok(())
        })
        .unwrap();
        copy_denoised_sidecar(&source_sidecar, &dest).unwrap();
        let target = strict_json(&dest).unwrap();
        assert_eq!(target["adjustments"]["exposure"], 1.25);
        let ownership = queue(&target).unwrap();
        assert_eq!(ownership["marked"], true);
        assert_eq!(ownership["completed"].as_array().unwrap().len(), 0);
        assert_eq!(
            queue(&strict_json(&source_sidecar).unwrap()).unwrap()["completed"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn nind_queue_baseline_matches_source_defaults_with_actual_exposure_and_highlight_options() {
        let mut settings = crate::app_settings::AppSettings {
            enable_baseline_exposure: true,
            baseline_exposure: 1.,
            ..Default::default()
        };
        let shot = crate::as_shot::AsShot {
            white_balance: crate::white_balance::WhiteBalance::reference(),
            exposure: crate::baseline_exposure::RawExposure {
                scene_referred: true,
                dng_baseline: 0.58,
                highlight_preservation: 2.,
                camera_baseline: Some(0.35),
            },
        };
        for compensate in [false, true] {
            settings.compensate_highlight_preservation = compensate;
            let render = crate::image_processing::resolve_render_options(&settings, true);
            for source in [
                Value::Null,
                json!({}),
                json!({"baselineExposure":false,"exposure":0}),
                json!({"baselineExposure":true}),
                json!({"baselineExposure":true,"cameraExposureCompensation":true}),
                json!({"baselineExposure":true,"cameraExposureCompensation":false}),
            ] {
                let prepared = nind_raw_defaults(&source);
                let expected = crate::image_processing::get_all_adjustments_from_json(
                    &source, true, shot, render,
                );
                let actual = crate::image_processing::get_all_adjustments_from_json(
                    &prepared, true, shot, render,
                );
                assert_eq!(bytemuck::bytes_of(&actual), bytemuck::bytes_of(&expected));
            }
            let legacy = crate::image_processing::get_all_adjustments_from_json(
                &json!({}),
                true,
                shot,
                render,
            );
            let fresh = crate::image_processing::get_all_adjustments_from_json(
                &nind_raw_defaults(&Value::Null),
                true,
                shot,
                render,
            );
            assert!((fresh.global.exposure - legacy.global.exposure).abs() > 0.5);
        }
        let mut metadata =
            serde_json::to_value(crate::image_processing::ImageMetadata::default()).unwrap();
        metadata["adjustments"] = json!({"baselineExposure":false});
        let identity = json!({"method":"nind","sourceBaselineEligible":true});
        assert!(
            ensure_rendered_eligible(&metadata, Path::new("photo.ARW"), &identity)
                .unwrap_err()
                .contains("Source baseline setting changed")
        );
    }

    #[test]
    fn nind_queue_keys_and_publication_follow_the_selected_copy_camera_policy() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.ARW");
        fs::write(&raw, b"unchanged RAW fixture").unwrap();
        let selected = format!("{}?vc=selected", raw.display());
        let (_, sidecar) = parse_virtual_path(&selected);
        set_denoise_mark(vec![selected.clone()], true).unwrap();
        let identity = json!({"method":"nind","pipeline":PIPELINE});
        let mut outputs = Vec::new();
        for camera in [false, true] {
            mutate(&sidecar, |metadata| {
                metadata["adjustments"] = json!({
                    "baselineExposure":true,"cameraExposureCompensation":camera
                });
                Ok(())
            })
            .unwrap();
            let (path, skipped) =
                run_item(&selected, &identity, &DenoiseJob::testing(), |frozen| {
                    assert_eq!(
                        frozen["sourceExposurePolicy"]["cameraExposureCompensation"],
                        camera
                    );
                    output(frozen)
                })
                .unwrap();
            assert!(!skipped);
            assert!(
                run_item(&selected, &identity, &DenoiseJob::testing(), |_| panic!(
                    "must reuse identical policy"
                ))
                .unwrap()
                .1
            );
            outputs.push(path);
        }
        assert_ne!(outputs[0], outputs[1]);
        assert_eq!(fs::read(&raw).unwrap(), b"unchanged RAW fixture");
        let error = run_item(
            &selected,
            &json!({"method":"nind","pipeline":"new-attempt"}),
            &DenoiseJob::testing(),
            |frozen| {
                mutate(&sidecar, |metadata| {
                    metadata["adjustments"]["cameraExposureCompensation"] = false.into();
                    Ok(())
                })?;
                output(frozen)
            },
        )
        .unwrap_err();
        assert!(error.contains("Source exposure policy changed"));
    }

    #[test]
    fn queue_strength_uses_the_existing_unit_interval_contract() {
        for method in ["nind", "bm3d"] {
            let options = Options {
                method: method.into(),
                strength: 50.,
                sharpen: false,
            };
            assert_eq!(options.internal().unwrap().intensity, 0.5);
        }
        assert_eq!(Options::default().internal().unwrap().intensity, 1.0);
    }

    #[test]
    fn rendered_raw_refuses_agx_wb_offsets_and_crops_while_sensor_dng_retains_them() {
        let source = Path::new("photo.ARW");
        let mut metadata =
            serde_json::to_value(crate::image_processing::ImageMetadata::default()).unwrap();
        for adjustment in [
            json!({"toneMapper":"agx"}),
            json!({"whiteBalance":{"temperature":4300,"tint":12}}),
            json!({"temperature":10,"tint":-5}),
            json!({"crop":{"unit":"%","x":0,"y":0,"width":80,"height":80}}),
        ] {
            metadata["adjustments"] = adjustment;
            assert!(
                ensure_rendered_eligible(&metadata, source, &ident())
                    .unwrap_err()
                    .contains("Edited photo")
            );
            assert!(ensure_rendered_eligible(&metadata, source, &json!({"method":"fast"})).is_ok());
        }
        metadata["adjustments"] = crate::image_processing::default_adjustments();
        assert!(ensure_rendered_eligible(&metadata, source, &ident()).is_ok());
        assert!(
            ensure_rendered_eligible(
                &metadata,
                source,
                &json!({"method":"nind","settings":{"toneOverride":true,"nonRawTone":"agx"}})
            )
            .is_err()
        );
    }

    #[test]
    fn unedited_raw_projects_only_neutral_nonraw_defaults_onto_tiff() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.ARW");
        fs::write(&raw, b"fixture").unwrap();
        let (_, sidecar) = parse_virtual_path(&raw.to_string_lossy());
        set_denoise_mark(vec![raw.to_string_lossy().into_owned()], true).unwrap();
        mutate(&sidecar, |v| {
            v["adjustments"] = crate::image_processing::default_adjustments();
            v["exif"] = json!({"Orientation":"8"});
            Ok(())
        })
        .unwrap();
        let output = dir.path().join("photo.tiff");
        save_result_edits(&sidecar, &raw, &output, &ident(), None).unwrap();
        let (_, dest) = parse_virtual_path(&output.to_string_lossy());
        let metadata = strict_json(&dest).unwrap();
        assert_eq!(
            metadata["adjustments"],
            json!({"toneMapper":"basic","baselineExposure":false})
        );
        assert_eq!(metadata["exif"]["Orientation"], "1");
        let all = crate::image_processing::get_all_adjustments_from_json(
            &metadata["adjustments"],
            false,
            crate::as_shot::AsShot::reference(),
            crate::image_processing::RenderOptions::default(),
        );
        assert_eq!(all.global.tonemapper_mode, 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn waiting_output_sync_checks_user_edits_after_acquiring_the_destination_lock() {
        use std::os::unix::fs::MetadataExt;
        let (_dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let ident = ident();
        let (out, _) = run_item(&path, &ident, &DenoiseJob::testing(), output).unwrap();
        let (_, dest) = parse_virtual_path(&out);
        let receipt: Receipt = serde_json::from_value(
            queue(&strict_json(&sidecar).unwrap()).unwrap()["completed"][0].clone(),
        )
        .unwrap();
        mutate(&sidecar, |v| {
            v["adjustments"]["exposure"] = 1.25.into();
            Ok(())
        })
        .unwrap();
        let guard = crate::file_management::lock_sidecar_as_external_writer(&dest).unwrap();
        let inode = guard.metadata().unwrap().ino();
        let source = PathBuf::from(path);
        let output = PathBuf::from(out);
        let worker = std::thread::spawn(move || {
            save_result_edits(&sidecar, &source, &output, &ident, Some(&receipt))
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut blocked = false;
        while std::time::Instant::now() < deadline {
            let locks = fs::read_to_string("/proc/locks").unwrap();
            if locks
                .lines()
                .any(|line| line.contains("->") && line.contains(&format!(":{inode} ")))
            {
                blocked = true;
                break;
            }
            std::thread::yield_now();
        }
        let mut user = strict_json(&dest).unwrap();
        user["adjustments"]["exposure"] = 3.into();
        crate::file_management::write_file_atomically_unmerged(
            &dest,
            serde_json::to_vec(&user).unwrap(),
        )
        .unwrap();
        drop(guard);
        let outcome = worker.join().unwrap();
        assert!(blocked, "queue did not block on the held destination lock");
        assert!(outcome.unwrap_err().contains("independent edits"));
        assert_eq!(strict_json(&dest).unwrap()["adjustments"]["exposure"], 3);
    }

    #[test]
    fn processing_identity_excludes_private_and_ui_settings() {
        let mut settings = crate::app_settings::AppSettings::default();
        let options = Options {
            method: "nind".into(),
            ..Default::default()
        };
        let original = processing_identity(&settings, &options);
        settings.last_root_path = Some("PRIVATE_PATH_DO_NOT_RECORD".into());
        settings.terminal_settings = Some(json!({"token":"PRIVATE_SECRET_DO_NOT_RECORD"}));
        settings.theme = Some("light".into());
        assert_eq!(processing_identity(&settings, &options), original);
        assert!(!original.to_string().contains("PRIVATE"));
        settings.baseline_exposure += 0.1;
        assert_ne!(processing_identity(&settings, &options), original);
        assert_eq!(
            processing_identity(&settings, &Options::default()),
            json!({})
        );
    }

    #[test]
    fn strict_cli_arguments_never_fall_through_to_gui() {
        for args in [
            vec![],
            vec!["--marked"],
            vec!["--marked", "folder", "--method", "oops"],
            vec!["--marked", "folder", "--strength", "NaN"],
            vec!["--marked", "folder", "--shrapen"],
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(parse_args(&args).is_err());
        }
        let args = [
            "--marked",
            "folder",
            "--method",
            "fast",
            "--sharpen",
            "--dry-run",
        ]
        .map(str::to_owned);
        let args = parse_args(&args).unwrap();
        assert!(args.dry_run && args.options.sharpen);
        assert_eq!(args.options.method, "fast");
    }
}
