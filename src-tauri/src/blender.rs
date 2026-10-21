//! Pack the intermediate job the Blender add-on stages in a workspace.
//!
//! The add-on no longer runs a Python core.  Clicking 「生成 Mod」 there only
//! writes:
//!
//! ```text
//! GeneratedMod/patches/<mesh>.exmtmesh
//! GeneratedMod/textures/<texture>.png
//! GeneratedMod/job.json
//! ```
//!
//! This module owns the Unity side of that job: it opens each target bundle,
//! encodes the replacement PNG in the asset's own format/size/mip count, then
//! hands a normal `modpack` spec to [`crate::modpack::run`].  The result is the
//! same `GeneratedMod/<hash>.bundle` + `mod.json` the installer already reads.
//!
//! Encoding is cached next to the payload (`<name>.<bundle>.bin.json`), so a
//! second click only re-encodes textures whose PNG or target metadata changed.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use exmt_core::asset;
use exmt_core::texconv::Texconv;
use serde::{Deserialize, Serialize};

/// The only job format this build understands.
pub const JOB_FORMAT: &str = "exmt-blender-job-1";

#[derive(Debug, Deserialize)]
struct Job {
    #[serde(default)]
    format: String,
    #[serde(default)]
    meshes: Vec<JobMesh>,
    #[serde(default)]
    textures: Vec<JobTexture>,
}

#[derive(Debug, Deserialize)]
struct JobMesh {
    #[serde(default)]
    bundle: String,
    mesh: String,
    file: String,
}

#[derive(Debug, Deserialize)]
struct JobTexture {
    name: String,
    file: String,
    #[serde(default)]
    bundles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    #[serde(default)]
    workspace_id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    bundles: BTreeMap<String, String>,
    #[serde(default)]
    mesh_bundles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    texture_bundles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    textures: Vec<ManifestTexture>,
}

#[derive(Debug, Default, Deserialize)]
struct ManifestTexture {
    #[serde(default)]
    name: String,
    #[serde(default)]
    bundle: String,
}

/// Cache key for one encoded texture payload.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CacheStamp {
    png_mtime: u64,
    png_size: u64,
    format: i32,
    width: usize,
    height: usize,
    mips: usize,
    texconv_mtime: u64,
}

pub fn job_path(workspace: &Path) -> PathBuf {
    workspace.join("GeneratedMod").join("job.json")
}

/// True when a job exists and `mod.json` is older than it (or missing).
///
/// `pack` uses this to stay idempotent: clicking 「加载到游戏」 twice must not
/// re-encode and re-pack unchanged Blender output.
pub fn is_stale(workspace: &Path) -> bool {
    let job = job_path(workspace);
    if !job.is_file() {
        return false;
    }
    let manifest = workspace.join("GeneratedMod").join("mod.json");
    if !manifest.is_file() {
        return true;
    }
    match (mtime(&job), mtime(&manifest)) {
        (job_at, manifest_at) => job_at > manifest_at,
    }
}

/// Pack one staged Blender job and return the `modpack` summary JSON.
///
/// No job means `{"skipped":true,"reason":"no-job"}`; an up-to-date job means
/// `{"skipped":true,"reason":"up-to-date"}`.  Both are success for the UI.
pub fn pack(workspace: &Path, texconv: &Texconv) -> Result<String, String> {
    let job_path = job_path(workspace);
    if !job_path.is_file() {
        return Ok(r#"{"skipped":true,"reason":"no-job"}"#.to_string());
    }
    if !is_stale(workspace) {
        return Ok(r#"{"skipped":true,"reason":"up-to-date"}"#.to_string());
    }

    let text = std::fs::read_to_string(&job_path)
        .map_err(|e| format!("read {}: {e}", job_path.display()))?;
    let job: Job = serde_json::from_str(&text)
        .map_err(|e| format!("parse {}: {e}", job_path.display()))?;
    if !job.format.is_empty() && job.format != JOB_FORMAT {
        return Err(format!(
            "unsupported Blender job format {:?} (expected {JOB_FORMAT})",
            job.format
        ));
    }

    let manifest_path = workspace.join("workspace.json");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("parse {}: {e}", manifest_path.display()))?;

    let outdir = workspace.join("GeneratedMod");
    let encdir = outdir.join(".encode");
    std::fs::create_dir_all(&encdir).map_err(|e| format!("create {}: {e}", encdir.display()))?;

    // job.json lives in GeneratedMod, so its `patches/...` and `textures/...`
    // paths are relative to that folder, not to the workspace root.
    let mut meshes = Vec::new();
    let mut seen_meshes: HashSet<(String, String)> = HashSet::new();
    for item in &job.meshes {
        let file = resolve(&outdir, &item.file);
        if !file.is_file() {
            return Err(format!("mesh patch not found: {}", file.display()));
        }
        let bundles = if item.bundle.is_empty() {
            manifest.mesh_bundles.get(&item.mesh).cloned().unwrap_or_default()
        } else {
            vec![item.bundle.clone()]
        };
        if bundles.is_empty() {
            return Err(format!("mesh {:?} has no target bundle in job.json", item.mesh));
        }
        for bundle in bundles {
            if bundle.is_empty() || !seen_meshes.insert((bundle.clone(), item.mesh.clone())) {
                continue;
            }
            meshes.push(serde_json::json!({
                "bundle": bundle,
                "mesh": item.mesh,
                "file": file.to_string_lossy(),
            }));
        }
    }

    // Group replacement textures by target bundle: one bundle is opened at a
    // time and dropped before the next, so peak memory stays at a single
    // texture pack even when a workspace replaces dozens of textures.
    let mut by_bundle: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut seen_textures: HashSet<(String, String)> = HashSet::new();
    for item in &job.textures {
        let png_path = resolve(&outdir, &item.file);
        if !png_path.is_file() {
            return Err(format!("replacement texture not found: {}", png_path.display()));
        }
        let targets = targets_for(&manifest, item);
        if targets.is_empty() {
            skipped.push(format!("{}: no target bundle", item.name));
            continue;
        }
        for hash in targets {
            if hash.is_empty() || !seen_textures.insert((hash.clone(), item.name.clone())) {
                continue;
            }
            by_bundle
                .entry(hash)
                .or_default()
                .push((item.name.clone(), png_path.clone()));
        }
    }

    let mut textures = Vec::new();
    for (hash, items) in &by_bundle {
        let path = manifest
            .bundles
            .get(hash)
            .ok_or_else(|| format!("bundle {hash} has no path in workspace.json"))?;
        let bundle = asset::open(path).map_err(|e| format!("open {path}: {e}"))?;
        for (name, png_path) in items {
            let png = std::fs::read(png_path)
                .map_err(|e| format!("read {}: {e}", png_path.display()))?;
            let (_, texture) = bundle
                .texture_object(name)
                .map_err(|e| format!("{hash}: {e}"))?;
            let stem = format!("{}.{}", safe(name), safe(hash));
            let payload_path = encdir.join(format!("{stem}.bin"));
            let cache_path = encdir.join(format!("{stem}.bin.json"));
            let stamp = CacheStamp {
                png_mtime: mtime(png_path),
                png_size: png.len() as u64,
                format: texture.format,
                width: texture.width,
                height: texture.height,
                mips: texture.mip_count,
                texconv_mtime: mtime(texconv.path()),
            };
            if !(cache_hit(&cache_path, &stamp) && payload_path.is_file()) {
                // One texture texconv cannot wrap (RGBAHalf, BC6H, …) must not
                // sink the whole Mod: record it and pack the rest.
                let payload = match texconv.encode(
                    &png,
                    texture.format,
                    Some((texture.width as u32, texture.height as u32)),
                    texture.mip_count as u32,
                ) {
                    Ok(payload) => payload,
                    Err(e) => {
                        skipped.push(format!("{name}（{e}）"));
                        continue;
                    }
                };
                std::fs::write(&payload_path, payload)
                    .map_err(|e| format!("write {}: {e}", payload_path.display()))?;
                let cache = serde_json::to_vec(&stamp).map_err(|e| e.to_string())?;
                std::fs::write(&cache_path, cache)
                    .map_err(|e| format!("write {}: {e}", cache_path.display()))?;
            }
            textures.push(serde_json::json!({
                "bundle": hash,
                "name": name,
                "file": payload_path.to_string_lossy(),
                "format": texture.format,
                "width": texture.width,
                "height": texture.height,
                "mips": texture.mip_count,
            }));
        }
    }

    if meshes.is_empty() && textures.is_empty() {
        return Err("job.json has neither meshes nor textures".to_string());
    }

    let spec = serde_json::json!({
        "outdir": outdir.to_string_lossy(),
        "label": manifest.label,
        "workspace": manifest.workspace_id,
        "bundles": manifest.bundles,
        "meshes": meshes,
        "textures": textures,
    });
    let spec_path = encdir.join("spec.json");
    let spec_text = serde_json::to_string_pretty(&spec).map_err(|e| e.to_string())?;
    std::fs::write(&spec_path, spec_text)
        .map_err(|e| format!("write {}: {e}", spec_path.display()))?;

    let summary = crate::modpack::run(&spec_path, true)
        .map_err(|e| match e {
            crate::modpack::RunError::Failed(text) => text,
        })?;
    let mut value: serde_json::Value =
        serde_json::from_str(&summary).map_err(|e| format!("parse modpack summary: {e}"))?;
    if !skipped.is_empty() {
        value["skipped_textures"] = serde_json::json!(skipped);
    }
    Ok(value.to_string())
}

fn targets_for(manifest: &Manifest, item: &JobTexture) -> Vec<String> {
    if !item.bundles.is_empty() {
        return item.bundles.iter().filter(|h| !h.is_empty()).cloned().collect();
    }
    if let Some(list) = manifest.texture_bundles.get(&item.name) {
        let found: Vec<String> = list.iter().filter(|h| !h.is_empty()).cloned().collect();
        if !found.is_empty() {
            return found;
        }
    }
    manifest
        .textures
        .iter()
        .find(|row| row.name == item.name && !row.bundle.is_empty())
        .map(|row| vec![row.bundle.clone()])
        .unwrap_or_default()
}

/// `base` is `GeneratedMod` for job entries: job.json paths are job-relative.
fn resolve(base: &Path, file: &str) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|delta| delta.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

fn cache_hit(path: &Path, stamp: &CacheStamp) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<CacheStamp>(&text)
        .map(|cached| cached == *stamp)
        .unwrap_or(false)
}

/// Keep a texture/bundle name usable as a Windows file name.
fn safe(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.len() > 96 {
        out.truncate(96);
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names_stay_usable() {
        assert_eq!(safe("c_Vepley_slg_cloth01_d"), "c_Vepley_slg_cloth01_d");
        assert_eq!(safe("a/b:c*"), "a_b_c_");
        assert_eq!(safe(""), "unnamed");
    }

    #[test]
    fn relative_job_paths_are_under_the_workspace() {
        let workspace = Path::new(r"D:\cache\workspace\skin");
        assert_eq!(
            resolve(workspace, "patches/mesh.exmtmesh"),
            workspace.join("patches/mesh.exmtmesh")
        );
        assert_eq!(
            resolve(workspace, r"D:\elsewhere\mesh.exmtmesh"),
            Path::new(r"D:\elsewhere\mesh.exmtmesh")
        );
    }

    #[test]
    fn pack_without_job_is_a_noop() {
        let dir = std::env::temp_dir().join(format!("exmt-blender-noop-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let tool = Texconv::new("texconv-not-needed.exe");
        let summary = pack(&dir, &tool).expect("a missing job is not an error");
        assert!(summary.contains("no-job"), "{summary}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_job_is_not_stale() {
        let dir = std::env::temp_dir().join(format!("exmt-blender-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(!is_stale(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real-data run, gated on `EXMT_BLENDER_WORKSPACE`:
    ///
    /// ```text
    /// set EXMT_BLENDER_WORKSPACE=<workspace with GeneratedMod/job.json>
    /// cargo test --manifest-path src-tauri/Cargo.toml packs_a_real_workspace -- --nocapture
    /// ```
    ///
    /// It writes `<workspace>/GeneratedMod/*.bundle` and `mod.json`, so point it
    /// at a scratch workspace unless you mean to install the result.
    #[test]
    fn packs_a_real_workspace_when_asked() {
        let Ok(dir) = std::env::var("EXMT_BLENDER_WORKSPACE") else {
            eprintln!("skip: set EXMT_BLENDER_WORKSPACE to pack a real job.json");
            return;
        };
        let exe = std::env::var("EXMT_TEXCONV").unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("bin")
                .join("texconv.exe")
                .to_string_lossy()
                .into_owned()
        });
        if !Path::new(&exe).is_file() {
            eprintln!("skip: texconv.exe not found at {exe}");
            return;
        }
        let summary = pack(Path::new(&dir), &Texconv::new(exe)).expect("pack the real job");
        let value: serde_json::Value = serde_json::from_str(&summary).expect("summary json");
        eprintln!("packed: {value}");
        assert!(value.get("bundles").is_some(), "modpack summary: {value}");
    }
}
