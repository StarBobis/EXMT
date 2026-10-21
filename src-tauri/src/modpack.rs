//! Rust implementation of the `modpack` core command.
//!
//! The app hands over a spec with already-encoded texture payloads and
//! `.exmtmesh` mesh files. This module opens each pristine bundle once, patches
//! the named Mesh and Texture2D objects through their type trees, rebuilds the
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
//! Mesh patches are accepted only when the file's channel layout and bone table
//! match the target asset. A different cook needs bone remapping and channel
//! fitting, which the Python core already implements; [`RunError::NeedsPython`]
//! makes the shell fall back to it before any bundle is written.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use exmt_core::asset;
use exmt_core::mesh;
use exmt_core::typetree::Value;
use exmt_core::{texture, typetree};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "exmt-mod-1";

/// Why a spec could not be packed by this module.
#[derive(Debug)]
pub enum RunError {
    /// The spec needs behaviour only the Python core has (bone remapping).
    NeedsPython(String),
    /// The spec or an input file is genuinely broken.
    Failed(String),
}

impl From<String> for RunError {
    fn from(value: String) -> Self {
        RunError::Failed(value)
    }
}

impl From<&str> for RunError {
    fn from(value: &str) -> Self {
        RunError::Failed(value.to_string())
    }
}

pub type RunResult<T> = Result<T, RunError>;

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
    #[serde(default)]
    pub meshes: Vec<MeshSpec>,
    #[serde(default)]
    pub textures: Vec<TextureSpec>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MeshSpec {
    pub bundle: String,
    pub mesh: String,
    pub file: String,
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
pub fn run(spec_path: &Path, compress: bool) -> RunResult<String> {
    let text = std::fs::read_to_string(spec_path)
        .map_err(|e| format!("read spec {}: {e}", spec_path.display()))?;
    let spec: Spec = serde_json::from_str(&text).map_err(|e| format!("parse spec: {e}"))?;

    let outdir = PathBuf::from(&spec.outdir);
    std::fs::create_dir_all(&outdir).map_err(|e| format!("create {}: {e}", outdir.display()))?;
    clear_outdir(&outdir)?;

    // Order-preserving grouping: bundles appear in spec order (meshes first,
    // then textures), matching the Python dict insertion order.
    let mut order: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for item in &spec.meshes {
        if seen.insert(item.bundle.clone()) {
            order.push(item.bundle.clone());
        }
    }
    for item in &spec.textures {
        if seen.insert(item.bundle.clone()) {
            order.push(item.bundle.clone());
        }
    }
    let mut mesh_grouped: HashMap<&str, Vec<&MeshSpec>> = HashMap::new();
    for item in &spec.meshes {
        mesh_grouped
            .entry(item.bundle.as_str())
            .or_default()
            .push(item);
    }
    let mut tex_grouped: HashMap<&str, Vec<&TextureSpec>> = HashMap::new();
    for item in &spec.textures {
        tex_grouped
            .entry(item.bundle.as_str())
            .or_default()
            .push(item);
    }

    let mut written: Vec<Written> = Vec::new();
    let mut missing: Vec<serde_json::Value> = Vec::new();
    let mut mesh_count = 0usize;

    for hash in &order {
        let meshes = mesh_grouped.get(hash.as_str()).cloned().unwrap_or_default();
        let textures = tex_grouped.get(hash.as_str()).cloned().unwrap_or_default();
        let source = spec.bundles.get(hash).cloned().unwrap_or_default();
        if source.is_empty() || !Path::new(&source).is_file() {
            missing.push(serde_json::json!({
                "hash": hash,
                "meshes": meshes.iter().map(|m| m.mesh.clone()).collect::<Vec<_>>(),
                "textures": textures.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
            }));
            continue;
        }

        let bundle = asset::open(&source).map_err(|e| format!("open {source}: {e}"))?;
        let mut replacements = BTreeMap::new();
        let mut patched_meshes: Vec<String> = Vec::new();
        let mut patched_verts = 0usize;
        let mut patched_textures: Vec<String> = Vec::new();

        for item in &meshes {
            if !Path::new(&item.file).is_file() {
                missing.push(serde_json::json!({
                    "hash": hash,
                    "mesh": item.mesh,
                    "file": item.file,
                }));
                continue;
            }
            let patch = crate::exmtmesh::load(Path::new(&item.file))
                .map_err(|e| format!("{}: {e}", item.file))?
                .into_patch();
            let (path_id, mut tree) =
                find_mesh_tree(&bundle, &item.mesh).map_err(|e| format!("{hash}: {e}"))?;
            if mesh::channels_of(&tree).map_err(|e| e.to_string())? != patch.channels
                || mesh::bone_hashes_of(&tree) != patch.bone_hashes
            {
                return Err(RunError::NeedsPython(format!(
                    "mesh {:?} is cooked differently in {hash}",
                    item.mesh
                )));
            }
            let report = mesh::apply_patch(&mut tree, &patch)
                .map_err(|e| format!("mesh {:?}: {e}", item.mesh))?;
            let obj = *bundle
                .serialized
                .object(path_id)
                .ok_or_else(|| format!("mesh {:?}: object {path_id} vanished", item.mesh))?;
            let bytes = typetree::write_value(&bundle.serialized, &obj, &tree)
                .map_err(|e| format!("mesh {:?}: {e}", item.mesh))?;
            replacements.insert(path_id, bytes);
            patched_meshes.push(patch.name.clone());
            patched_verts += report.verts;
            mesh_count += 1;
        }

        for item in &textures {
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
                )
                .into());
            }
            let want = texture::expected_bytes(tex.format, tex.width, tex.height, tex.mip_count)
                .map_err(|e| format!("texture {}: {e}", item.name))?;
            if payload.len() != want {
                return Err(format!(
                    "texture {}: payload is {} bytes but the asset expects {} ({}x{}, {} mips, format {})",
                    item.name, payload.len(), want, tex.width, tex.height, tex.mip_count, tex.format
                )
                .into());
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
            patched_textures.push(item.name.clone());
        }

        if replacements.is_empty() {
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
            meshes: patched_meshes,
            textures: patched_textures,
            verts: patched_verts,
            source,
        });
    }

    // Patch rows keep the source `.exmtmesh` (not the packed bundle) so merging
    // two Mods can replay both patch sets on a fresh copy of the bundle.
    let mut patch_rows: Vec<serde_json::Value> = Vec::new();
    let mut patch_index: HashMap<&str, usize> = HashMap::new();
    for item in &spec.meshes {
        match patch_index.get(item.file.as_str()) {
            Some(&index) => {
                let bundles = patch_rows[index]["bundles"]
                    .as_array_mut()
                    .expect("bundles");
                let hash = serde_json::Value::String(item.bundle.clone());
                if !bundles.contains(&hash) {
                    bundles.push(hash);
                }
            }
            None => {
                let (file, inside) = relative_to(&item.file, &outdir);
                patch_rows.push(serde_json::json!({
                    "mesh": item.mesh,
                    "file": file,
                    "inside": inside,
                    "bundles": [item.bundle.clone()],
                }));
                patch_index.insert(item.file.as_str(), patch_rows.len() - 1);
            }
        }
    }

    // Texture rows for the manifest, grouped by file in spec order.
    let mut tex_rows: Vec<serde_json::Value> = Vec::new();
    let mut tex_by_file: HashMap<&str, usize> = HashMap::new();
    for item in &spec.textures {
        if let Some(&index) = tex_by_file.get(item.file.as_str()) {
            let bundles = tex_rows[index]["bundles"].as_array_mut().expect("bundles");
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
            "mips": if item.mips == 0 { 1 } else { item.mips },
            "bundles": [item.bundle.clone()],
        }));
        tex_by_file.insert(item.file.as_str(), tex_rows.len() - 1);
    }

    let manifest = serde_json::json!({
        "format": FORMAT,
        "label": spec.label,
        "workspace": spec.workspace,
        "created_at": crate::skinpack::local_timestamp(),
        "game_bundles": spec.game_bundles,
        "mesh_count": mesh_count,
        "bundles": written,
        "patches": patch_rows,
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
    serde_json::to_string(&summary).map_err(|e| RunError::Failed(e.to_string()))
}

/// Find a Mesh object by `m_Name`, returning its path id and decoded tree.
fn find_mesh_tree(bundle: &asset::OpenedBundle, name: &str) -> Result<(i64, Value), String> {
    for (path_id, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        if ty.class_id != asset::CLASS_MESH {
            continue;
        }
        let tree = typetree::read_value(&bundle.serialized, obj)
            .map_err(|e| format!("decode mesh {path_id}: {e}"))?;
        if tree.get("m_Name").and_then(|v| v.as_str()) == Some(name) {
            return Ok((*path_id, tree));
        }
    }
    Err(format!("mesh {name:?} not found in bundle"))
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
