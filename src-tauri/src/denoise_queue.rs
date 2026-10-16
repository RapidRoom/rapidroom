//! Persistent marks and crash-safe, content-addressed denoise results, shared by CLI and UI.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

const PIPELINE: &str = "rapidroom-denoise-queue-v1";

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
            intensity: if self.method == "bm3d" {
                self.strength / 100.
            } else {
                self.strength
            },
            sharpen: self.sharpen,
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
    if !value.is_object() {
        return Err(format!("Invalid sidecar object: {}", path.display()));
    }
    // Validate edit schema too; never replace unreadable edits with defaults.
    serde_json::from_value::<crate::image_processing::ImageMetadata>(value.clone())
        .map_err(|e| format!("Invalid sidecar {}: {e}", path.display()))?;
    Ok(value)
}

pub(crate) fn lock_path(path: &Path) -> std::io::Result<File> {
    let target = if path.is_symlink() {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("Missing filename"))?
        .to_string_lossy();
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(target.with_file_name(format!(".{name}.rapidroom-lock")))?;
    file.lock()?;
    Ok(file)
}

fn mutate(
    path: &Path,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<(), String> {
    crate::file_management::ensure_card_writable_for_paths(&[path.to_string_lossy().as_ref()])?;
    let _guard = lock_path(path).map_err(|e| e.to_string())?;
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

fn save_result_edits(
    source: &Path,
    output: &Path,
    previous: Option<&Receipt>,
) -> Result<String, String> {
    let mut source_edits = edits(strict_json(source)?);
    let (_, dest) = parse_virtual_path(&output.to_string_lossy());
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
    }
    crate::file_management::write_file_atomically(
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
    let job = state
        .denoise_jobs
        .start(job_id, Some(event_sink(&app)))
        .ok_or(DENOISE_CANCELLED)?;
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
                let result = run_item(path, &ident, &job, || {
                    denoise_image_with_settings(
                        &parse_virtual_path(path).0.to_string_lossy(),
                        &internal,
                        &app,
                        models.clone(),
                        &job,
                        frozen_settings.clone(),
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
    outcome
}

fn run_item(
    path: &str,
    ident: &Value,
    job: &DenoiseJob,
    denoise: impl FnOnce() -> Result<DenoisedOutput, String>,
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
    let q = queue(&strict_json(&sidecar)?)?;
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
        receipt.edits_hash = save_result_edits(&sidecar, &output, Some(&receipt))?;
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
        receipt.edits_hash = save_result_edits(&sidecar, &output, Some(&receipt))?;
        record(&sidecar, &receipt)?;
        return Ok((output.to_string_lossy().into_owned(), true));
    }
    let staged_receipt = fs::read(&provenance)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .filter(|r| r.key == key);
    job.check()?;
    let image = denoise()?;
    job.check()?;
    if hash_file(&source)? != source_hash {
        return Err("Source changed during denoise; result discarded".into());
    }
    let mut committed = None;
    publish(image, &output, raw, job, |hash| {
        let edits_hash = save_result_edits(
            &sidecar,
            &output,
            previous.as_ref().or(staged_receipt.as_ref()),
        )?;
        crate::exif_processing::write_rrexif_sidecar(&source.to_string_lossy(), &output)?;
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
    fn output() -> Result<DenoisedOutput, String> {
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
            let (out, skipped) = run_item(path, &ident(), &job, || {
                calls += 1;
                output()
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
            run_item(&path, &ident(), &DenoiseJob::testing(), || panic!(
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
        let (recovered, skipped) = run_item(&path, &ident(), &DenoiseJob::testing(), || {
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
            run_item(&path, &ident(), &job, || panic!("must not run"))
                .unwrap_err()
                .contains("another queue")
        );
    }

    #[test]
    fn corrupt_sidecar_and_changing_source_do_not_publish_or_erase_edits() {
        let (dir, path) = fixture();
        let (_, sidecar) = parse_virtual_path(&path);
        let error = run_item(&path, &ident(), &DenoiseJob::testing(), || {
            fs::write(&path, b"changed").unwrap();
            output()
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
        assert!(set_denoise_mark(vec![path], false).is_err());
        assert_eq!(fs::read(sidecar).unwrap(), b"broken json");
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
