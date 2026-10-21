//! Rust implementation of the `modpack` core command.
//!
//! The app hands over a spec with a list of already-encoded texture payloads
//! and, optionally, mesh patch files. This module covers the **texture half**:
//! it opens each pristine bundle once, rewrites the named Texture2D objects
//! through their type trees (inline pixels, clear the stream), rebuilds the
//! SerializedFile and the UnityFS container, and writes:
//!
//! ```text
//! <outdir>/<hash>.bundle
//! <outdir>/mod.json
//! ```
//!
//! `mod.json` reproduces `core/exmt_core/modpack.py`'s shape so the installer
//! does not care which implementation produced it.
//!
//! Specs that contain mesh patches are refused here and fall back to the Python
//! core: `.exmtmesh` is an NPZ archive carrying a full typetree, and porting
//! that reader is a separate step. Refusing loudly is better than silently
//! shipping a mod without its meshes.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use exmt_core::asset;
use exmt_core::{texture, typetree};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "exmt-mod-1";

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub outdir: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub game_bundles: String,
    #[serde(default)]
    pub bundles: BTreeMap<String, String>,
    /// Mesh patch rows. Any entry here means "Python core must handle it".
    #[serde(default)]
    pub meshes: Vec<serde_json::Value>,
    #[serde(default)]
    pub textures: Vec<TextureSpec>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TextureSpec {
    pub bundle: String,
    pub name: String,
    pub file: String,
    #[serde(default)]
    pub format: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub mips: i32,
}

#[derive(Debug, Serialize)]
struct Written {
    hash: String,
    file: String,
    bytes: usize,
    source_bytes: u64,
    meshes: Vec<String>,
    textures: Vec<String>,
    verts: usize,
    source: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    dir: String,
    bundles: usize,
    meshes: usize,
    textures: usize,
    missing: usize,
    bytes: usize,
    files: Vec<String>,
}

/// Run one modpack spec and return the summary JSON line the CLI prints.
pub fn run(spec_path: &Path, compress: bool) -> Result<String, String> {
    let text = std::fs::read_to_string(spec_path)
        .map_err(|e| format!("read spec {}: {e}", spec_path.display()))?;
    let spec: Spec = serde_json::from_str(&text).map_err(|e| format!("parse spec: {e}"))?;
    if !spec.meshes.is_empty() {
        return Err("this spec contains mesh patches; the Python core handles those".into());
    }

    let outdir = PathBuf::from(&spec.outdir);
    std::fs::create_dir_all(&outdir).map_err(|e| format!("create {}: {e}", outdir.display()))?;
    clear_outdir(&outdir)?;

    // Order-preserving grouping: bundles appear in spec order, like the
    // Python dict insertion order.
    let mut order: Vec<String> = Vec::new();
    let mut grouped: HashMap<String, Vec<&TextureSpec>> = HashMap::new();
    for item in &spec.textures {
        if !grouped.contains_key(&item.bundle) {
            order.push(item.bundle.clone());
        }
        grouped.entry(item.bundle.clone()).or_default().push(item);
    }

    let mut written: Vec<Written> = Vec::new();
    let mut missing: Vec<serde_json::Value> = Vec::new();

    for hash in &order {
        let rows = grouped.get(hash).expect("grouped above");
        let source = spec.bundles.get(hash).cloned().unwrap_or_default();
        if source.is_empty() || !Path::new(&source).is_file() {
            missing.push(serde_json::json!({
                "hash": hash,
                "meshes": [],
                "textures": rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            }));
            continue;
        }

        let bundle = asset::open(&source).map_err(|e| format!("open {source}: {e}"))?;
        let mut replacements = BTreeMap::new();
        let mut patched = Vec::new();
        for item in rows {
            if !Path::new(&item.file).is_file() {
                missing.push(serde_json::json!({
                    "hash": hash,
                    "texture": item.name,
                    "file": item.file,
                }));
                continue;
            }
            let payload =
                std::fs::read(&item.file).map_err(|e| format!("read {}: {e}", item.file))?;
            let (path_id, tex) = bundle
                .texture_object(&item.name)
                .map_err(|e| format!("{hash}: {e}"))?;
            if item.format != 0 && item.format != tex.format {
                return Err(format!(
                    "texture {}: encoded as {} but the asset is {}",
                    item.name, item.format, tex.format
                ));
            }
            let want = texture::expected_bytes(tex.format, tex.width, tex.height, tex.mip_count)
                .map_err(|e| format!("texture {}: {e}", item.name))?;
            if payload.len() != want {
                return Err(format!(
                    "texture {}: payload is {} bytes but the asset expects {} ({}x{}, {} mips, format {})",
                    item.name,
                    payload.len(),
                    want,
                    tex.width,
                    tex.height,
                    tex.mip_count,
                    tex.format
                ));
            }
            let obj = *bundle
                .serialized
                .object(path_id)
                .ok_or_else(|| format!("texture {}: object {path_id} vanished", item.name))?;
            let mut tree = typetree::read_value(&bundle.serialized, &obj)
                .map_err(|e| format!("texture {}: {e}", item.name))?;
            texture::store_image(&mut tree, &payload)
                .map_err(|e| format!("texture {}: {e}", item.name))?;
            let bytes = typetree::write_value(&bundle.serialized, &obj, &tree)
                .map_err(|e| format!("texture {}: {e}", item.name))?;
            replacements.insert(path_id, bytes);
            patched.push(item.name.clone());
        }
        if patched.is_empty() {
            continue;
        }

        let saved = bundle
            .save_patched(&replacements, compress)
            .map_err(|e| format!("rebuild {hash}: {e}"))?;
        let file = format!("{hash}.bundle");
        std::fs::write(outdir.join(&file), &saved).map_err(|e| format!("write {file}: {e}"))?;
        written.push(Written {
            hash: hash.clone(),
            file,
            bytes: saved.len(),
            source_bytes: std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0),
            meshes: Vec::new(),
            textures: patched,
            verts: 0,
            source,
        });
    }

    // Texture rows for the manifest, grouped by file in spec order.
    let mut tex_rows: Vec<serde_json::Value> = Vec::new();
    let mut tex_by_file: HashMap<String, usize> = HashMap::new();
    for item in &spec.textures {
        if let Some(&index) = tex_by_file.get(&item.file) {
            let row = &mut tex_rows[index];
            let bundles = row["bundles"].as_array_mut().expect("bundles array");
            let hash = serde_json::Value::String(item.bundle.clone());
            if !bundles.contains(&hash) {
                bundles.push(hash);
            }
            continue;
        }
        let (file, inside) = relative_to(&item.file, &outdir);
        tex_rows.push(serde_json::json!({
            "name": item.name,
            "file": file,
            "inside": inside,
            "format": item.format,
            "width": item.width,
            "height": item.height,
            "mips": item.mips,
            "bundles": [item.bundle.clone()],
        }));
        tex_by_file.insert(item.file.clone(), tex_rows.len() - 1);
    }

    let mesh_count = 0usize;
    let manifest = serde_json::json!({
        "format": FORMAT,
        "label": spec.label,
        "workspace": spec.workspace,
        "created_at": crate::skinpack::local_timestamp(),
        "game_bundles": spec.game_bundles,
        "mesh_count": mesh_count,
        "bundles": written,
        "patches": [],
        "textures": tex_rows,
        "missing": missing,
    });
    let target = outdir.join("mod.json");
    let tmp = outdir.join("mod.json.tmp");
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("write mod.json: {e}"))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("rename mod.json: {e}"))?;

    let files: Vec<String> = written.iter().map(|w| w.file.clone()).collect();
    let summary = Summary {
        dir: spec.outdir,
        bundles: written.len(),
        meshes: mesh_count,
        textures: tex_rows.len(),
        missing: missing.len(),
        bytes: written.iter().map(|w| w.bytes).sum(),
        files,
    };
    serde_json::to_string(&summary).map_err(|e| e.to_string())
}

/// Python's `_clear`: remove previous bundles and the manifest, nothing else.
fn clear_outdir(outdir: &Path) -> Result<(), String> {
    let entries =
        std::fs::read_dir(outdir).map_err(|e| format!("list {}: {e}", outdir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let remove = name.ends_with(".bundle") || name == "mod.json" || name == "mod.json.tmp";
        if remove {
            std::fs::remove_file(entry.path())
                .map_err(|e| format!("remove {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Lexical relative path with forward slashes, matching `os.path.relpath`
/// for the "inside outdir" case. Outside files stay absolute.
fn relative_to(file: &str, outdir: &Path) -> (String, bool) {
    let path = Path::new(file);
    if let Ok(rel) = path.strip_prefix(outdir) {
        let text = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !text.is_empty() {
            return (text, true);
        }
    }
    (file.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_spec_is_refused() {
        let dir = std::env::temp_dir().join(format!("exmt-modpack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = dir.join("spec.json");
        std::fs::write(
            &spec,
            serde_json::json!({
                "outdir": dir.to_string_lossy(),
                "bundles": {},
                "meshes": [{"bundle": "h", "mesh": "m", "file": "x"}],
                "textures": []
            })
            .to_string(),
        )
        .unwrap();
        assert!(run(&spec, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_texture_spec_writes_a_manifest() {
        let dir = std::env::temp_dir().join(format!("exmt-modpack-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = dir.join("spec.json");
        std::fs::write(
            &spec,
            serde_json::json!({
                "outdir": dir.to_string_lossy(),
                "label": "t",
                "bundles": {},
                "meshes": [],
                "textures": []
            })
            .to_string(),
        )
        .unwrap();
        let line = run(&spec, true).unwrap();
        let summary: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(summary["bundles"], 0);
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("mod.json")).unwrap()).unwrap();
        assert_eq!(manifest["format"], FORMAT);
        assert_eq!(manifest["mesh_count"], 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        let out = Path::new(r"C:\cache\merge");
        assert_eq!(
            relative_to(r"C:\cache\merge\tex\a.dds", out),
            ("tex/a.dds".to_string(), true)
        );
        assert_eq!(
            relative_to(r"D:\somewhere\a.dds", out),
            (r"D:\somewhere\a.dds".to_string(), false)
        );
    }
}
