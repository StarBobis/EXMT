//! `workspace` export for the Blender add-on.
//!
//! Writes one folder per workspace:
//!
//! ```text
//! meshes/<mesh>.exmtmesh   geometry + full typetree + skeleton view
//! textures/<name>.png      decoded (max 2048) textures
//! animations/*.exmtanim    reference clips (not part of a Mod)
//! skeleton.json            merged rig for the whole skin
//! workspace.json           what the add-on reads
//! spec.json                the input, for packing later
//! ```
//!
//! This is the Rust replacement for `preview.make_workspace`: meshes, textures,
//! the merged skeleton and reference animation clips all run here. The Blender
//! add-on reads the same folder layout from either implementation.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use exmt_core::anim;
use exmt_core::asset;
use exmt_core::asset::OpenedBundle;
use exmt_core::crc32::crc32;
use exmt_core::mesh::{self, Mesh};
use exmt_core::skeleton::{self, View};
use exmt_core::texconv::Texconv;
use exmt_core::typetree::{self, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Spec {
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub game_bundles: String,
    pub bundles: BTreeMap<String, String>,
    #[serde(default)]
    pub mesh_bundles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub texture_bundles: BTreeMap<String, Vec<String>>,
    pub meshes: Vec<MeshItem>,
    #[serde(default)]
    pub textures: Vec<TextureItem>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub animations: Vec<AnimItem>,
    #[serde(default)]
    pub skeleton_meshes: Vec<MeshItem>,
    #[serde(default)]
    pub bone_names: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MeshItem {
    pub bundle: String,
    pub mesh: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TextureItem {
    pub bundle: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AnimItem {
    pub bundle: String,
    pub clip: String,
}

pub fn run(outdir: &Path, spec_path: &Path, texconv: &Texconv) -> Result<String, String> {
    let text = std::fs::read_to_string(spec_path)
        .map_err(|e| format!("read spec {}: {e}", spec_path.display()))?;
    let spec: Spec = serde_json::from_str(&text).map_err(|e| format!("parse spec: {e}"))?;

    let mesh_dir = outdir.join("meshes");
    let tex_dir = outdir.join("textures");
    let anim_dir = outdir.join("animations");
    for stale in [&mesh_dir, &tex_dir, &anim_dir] {
        let _ = std::fs::remove_dir_all(stale);
    }
    for dir in [&mesh_dir, &tex_dir] {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }

    let mut bundles: HashMap<String, OpenedBundle> = HashMap::new();
    let load = |hash: &str, bundles: &mut HashMap<String, OpenedBundle>| -> Result<(), String> {
        if bundles.contains_key(hash) {
            return Ok(());
        }
        let path = spec
            .bundles
            .get(hash)
            .ok_or_else(|| format!("bundle {hash} has no path in the spec"))?;
        let opened = asset::open(path).map_err(|e| format!("open {path}: {e}"))?;
        bundles.insert(hash.to_string(), opened);
        Ok(())
    };

    // Exported meshes, in spec order.
    let mut loaded: Vec<(String, Mesh, Value)> = Vec::with_capacity(spec.meshes.len());
    for item in &spec.meshes {
        load(&item.bundle, &mut bundles)?;
        let bundle = bundles.get(&item.bundle).expect("loaded above");
        let (_, mesh, tree) = mesh_with_tree(bundle, &item.mesh)?;
        loaded.push((item.bundle.clone(), mesh, tree));
    }

    // The rig takes the union of every skin mesh; extras are loaded only when
    // the spec asks for them (animations), matching the Python packer.
    let mut skeleton_tables: Vec<(String, Vec<u32>, Vec<f32>)> = Vec::new();
    for (_, mesh, _) in &loaded {
        skeleton_tables.push((
            mesh.name.clone(),
            mesh.bone_hashes.clone(),
            mesh.bindpose.clone(),
        ));
    }
    for item in &spec.skeleton_meshes {
        if loaded
            .iter()
            .any(|(hash, m, _)| hash == &item.bundle && m.name == item.mesh)
        {
            continue;
        }
        load(&item.bundle, &mut bundles)?;
        let bundle = bundles.get(&item.bundle).expect("loaded above");
        if let Ok((_, mesh, _)) = mesh_with_tree(bundle, &item.mesh) {
            skeleton_tables.push((mesh.name.clone(), mesh.bone_hashes, mesh.bindpose));
        }
    }

    let paths = load_bone_paths(&spec.bone_names);
    let mut skeleton = skeleton::merge(&skeleton_tables, &paths, 0.01)
        .map_err(|e| format!("merge skeleton: {e}"))?;
    skeleton.id = if spec.id.is_empty() {
        spec.name.clone()
    } else {
        spec.id.clone()
    };

    let mut written_meshes = Vec::new();
    let mut mesh_bytes = 0u64;
    for (index, (hash, mesh, tree)) in loaded.iter().enumerate() {
        let view: View = skeleton.view_for(&mesh.name);
        let bytes = crate::interchange::build(mesh, tree, hash, &view);
        let fname = format!("{}.exmtmesh", safe(&mesh.name));
        let path = mesh_dir.join(&fname);
        std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        mesh_bytes += bytes.len() as u64;
        written_meshes.push(serde_json::json!({
            "index": index,
            "alias": alias(&mesh.name),
            "file": format!("meshes/{fname}"),
            "mesh": mesh.name,
            "bundle": hash,
            "verts": mesh.vertex_count,
            "bones": view.local.len(),
        }));
    }

    let skeleton_json = serde_json::json!({
        "format": skeleton::FORMAT,
        "id": skeleton.id,
        "count": skeleton.bones.len(),
        "named": skeleton.named,
        "conflicts": skeleton.conflicts.iter().map(|c| serde_json::json!({
            "hash": c.hash, "mesh": c.mesh, "deviation": c.deviation,
        })).collect::<Vec<_>>(),
        "bones": skeleton.bones.iter().map(|b| serde_json::json!({
            "hash": b.hash, "name": b.name, "parent": b.parent, "rest": b.rest, "meshes": b.meshes,
        })).collect::<Vec<_>>(),
        "meshes": skeleton.local,
    });
    write_pretty(&outdir.join("skeleton.json"), &skeleton_json)?;

    let mut written_textures = Vec::new();
    let mut failed = Vec::new();
    for item in &spec.textures {
        let result = (|| -> Result<serde_json::Value, String> {
            load(&item.bundle, &mut bundles)?;
            let bundle = bundles.get(&item.bundle).expect("loaded above");
            let (_, texture) = bundle
                .texture_object(&item.name)
                .map_err(|e| e.to_string())?;
            let (width, height, png) = texture
                .decode_png(&bundle.nodes, texconv, Some(2048))
                .map_err(|e| e.to_string())?;
            let fname = format!("{}.png", safe(&item.name));
            let path = tex_dir.join(&fname);
            std::fs::write(&path, &png).map_err(|e| format!("write {}: {e}", path.display()))?;
            Ok(serde_json::json!({
                "file": format!("textures/{fname}"),
                "name": item.name,
                "bundle": item.bundle,
                "width": width,
                "height": height,
            }))
        })();
        match result {
            Ok(row) => written_textures.push(row),
            Err(e) => failed.push(serde_json::json!({
                "name": item.name,
                "error": short(&e, 200),
            })),
        }
    }

    let mut written_anims = Vec::new();
    let mut failed_anims = Vec::new();
    let skeleton_hashes: std::collections::HashSet<u32> =
        skeleton.bones.iter().map(|b| b.hash).collect();
    let mut hierarchies: HashMap<String, (HashMap<u32, u32>, HashMap<u32, String>)> =
        HashMap::new();
    if !spec.animations.is_empty() {
        std::fs::create_dir_all(&anim_dir)
            .map_err(|e| format!("create {}: {e}", anim_dir.display()))?;
    }
    for item in &spec.animations {
        let result = (|| -> Result<serde_json::Value, String> {
            load(&item.bundle, &mut bundles)?;
            let bundle = bundles.get(&item.bundle).expect("loaded above");
            if !hierarchies.contains_key(&item.bundle) {
                hierarchies.insert(item.bundle.clone(), hierarchy(bundle));
            }
            let (parents, prefab_paths) = hierarchies.get(&item.bundle).expect("inserted above");
            let clip = load_clip(bundle, &item.clip)?;
            let mut merged = paths.clone();
            for (hash, path) in prefab_paths {
                merged.insert(*hash, path.clone());
            }
            let tracks = clip.tracks(None, &merged);
            let payload = anim_json(&clip, &tracks, parents);
            let fname = format!("{}.exmtanim", safe(&clip.name));
            let path = anim_dir.join(&fname);
            let text = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
            std::fs::write(&path, &text).map_err(|e| format!("write {}: {e}", path.display()))?;
            let matched = tracks
                .rows
                .iter()
                .filter(|row| skeleton_hashes.contains(&row.hash))
                .count();
            Ok(serde_json::json!({
                "file": format!("animations/{fname}"),
                "clip": clip.name,
                "bundle": item.bundle,
                "seconds": round32(clip.length(), 3),
                "samples": tracks.times.len(),
                "loop": clip.looped,
                "bones": tracks.rows.len(),
                "matched": matched,
                "bytes": text.len(),
            }))
        })();
        match result {
            Ok(row) => written_anims.push(row),
            Err(e) => failed_anims.push(serde_json::json!({
                "clip": item.clip,
                "error": short(&e, 200),
            })),
        }
    }

    let manifest = serde_json::json!({
        "format": "exmt-workspace-1",
        "workspace_id": if spec.workspace_id.is_empty() { spec.id.clone() } else { spec.workspace_id.clone() },
        "label": spec.label,
        "created_at": if spec.created_at.is_empty() { crate::skinpack::local_timestamp() } else { spec.created_at.clone() },
        "entry": { "id": spec.id, "name": spec.name, "display": spec.display },
        "game_bundles": spec.game_bundles,
        "bundles": spec.bundles,
        "mesh_bundles": spec.mesh_bundles,
        "texture_bundles": spec.texture_bundles,
        "skeleton": {
            "file": "skeleton.json",
            "bones": skeleton.bones.len(),
            "named": skeleton.named,
            "conflicts": skeleton.conflicts.len(),
        },
        "meshes": written_meshes,
        "textures": written_textures,
        "groups": spec.groups,
        "failed_textures": failed,
        "animations": written_anims,
        "failed_animations": failed_anims,
    });
    write_pretty(&outdir.join("workspace.json"), &manifest)?;
    let spec_copy = outdir.join("spec.json");
    if !spec_copy.exists() {
        write_pretty(&spec_copy, &serde_json::to_value(&spec).unwrap_or_default())?;
    }

    let summary = serde_json::json!({
        "dir": outdir.display().to_string(),
        "meshes": loaded.len(),
        "textures": written_textures.len(),
        "failed_textures": failed.len(),
        "animations": written_anims.len(),
        "failed_animations": failed_anims.len(),
        "bones": skeleton.bones.len(),
        "named_bones": skeleton.named,
        "bytes": mesh_bytes,
    });
    serde_json::to_string(&summary).map_err(|e| e.to_string())
}

fn load_clip(bundle: &OpenedBundle, name: &str) -> Result<anim::Clip, String> {
    // A bundle can contain several clips with the same name; Python's
    // `open_clips` dict keeps the last one, so later matches overwrite.
    let mut found = None;
    for (path_id, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        if ty.class_id != 74 {
            continue;
        }
        let tree = typetree::read_value(&bundle.serialized, obj)
            .map_err(|e| format!("decode clip {path_id}: {e}"))?;
        if tree.get("m_Name").and_then(|v| v.as_str()) == Some(name) {
            let clip = anim::from_value(&tree).map_err(|e| format!("{name}: {e}"))?;
            found = Some(clip);
        }
    }
    found.ok_or_else(|| format!("clip {name:?} not found"))
}

/// `({path hash: parent hash}, {path hash: path})` from the bundle's prefab.
///
/// The clip's bindings only carry `crc32(transform path)`; the hierarchy is in
/// the Transform/GameObject objects that ship in the same bundle. Paths are
/// registered under every suffix because the clip is Animator-relative while the
/// prefab starts at the model object.
fn hierarchy(bundle: &OpenedBundle) -> (HashMap<u32, u32>, HashMap<u32, String>) {
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut father: HashMap<i64, i64> = HashMap::new();
    let mut owner: HashMap<i64, i64> = HashMap::new();
    for (path_id, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        let kind = match ty.class_id {
            1 => "gameobject",
            4 => "transform",
            _ => continue,
        };
        let Ok(tree) = typetree::read_value(&bundle.serialized, obj) else {
            continue;
        };
        if kind == "gameobject" {
            names.insert(
                *path_id,
                tree.get("m_Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
        } else {
            let father_id = tree
                .get("m_Father")
                .and_then(|v| v.get("m_PathID"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let owner_id = tree
                .get("m_GameObject")
                .and_then(|v| v.get("m_PathID"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            father.insert(*path_id, father_id);
            owner.insert(*path_id, owner_id);
        }
    }

    let mut path_of: HashMap<i64, String> = HashMap::new();
    for &tid in father.keys() {
        let mut parts: Vec<String> = Vec::new();
        let mut cur = tid;
        let mut guard = 0;
        while cur != 0 && guard < 64 {
            if let Some(owner_id) = owner.get(&cur) {
                if let Some(name) = names.get(owner_id) {
                    if !name.is_empty() {
                        parts.push(name.clone());
                    }
                }
            }
            let next = father.get(&cur).copied().unwrap_or(0);
            if next == 0 {
                break;
            }
            cur = next;
            guard += 1;
        }
        parts.reverse();
        if !parts.is_empty() {
            path_of.insert(tid, parts.join("/"));
        }
    }

    let mut parents: HashMap<u32, u32> = HashMap::new();
    let mut paths: HashMap<u32, String> = HashMap::new();
    let mut depth: HashMap<u32, usize> = HashMap::new();
    for (&tid, path) in &path_of {
        let parent_path = father
            .get(&tid)
            .and_then(|id| path_of.get(id))
            .cloned()
            .unwrap_or_default();
        let parent_parts: Vec<&str> = if parent_path.is_empty() {
            Vec::new()
        } else {
            parent_path.split('/').collect()
        };
        let parts: Vec<&str> = path.split('/').collect();
        for start in 0..parts.len() {
            let suffix = parts[start..].join("/");
            if suffix.is_empty() {
                continue;
            }
            let parent_suffix = if parent_parts.len() > start {
                parent_parts[start..].join("/")
            } else {
                String::new()
            };
            let hash = crc32(suffix.as_bytes());
            let candidate = if parent_suffix.is_empty() {
                0
            } else {
                crc32(parent_suffix.as_bytes())
            };
            let previous = parents.get(&hash).copied();
            let should_replace = match previous {
                None => true,
                Some(0) if candidate != 0 => true,
                Some(prev) => {
                    start < depth.get(&hash).copied().unwrap_or(usize::MAX)
                        && (candidate != 0) >= (prev != 0)
                }
            };
            if should_replace {
                parents.insert(hash, candidate);
                depth.insert(hash, start);
            }
            paths.entry(hash).or_insert(suffix);
        }
    }
    (parents, paths)
}

fn anim_json(
    clip: &anim::Clip,
    tracks: &anim::Tracks,
    parents: &HashMap<u32, u32>,
) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = tracks
        .rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "path": row.path,
                "hash": row.hash,
                "position": axes(&row.position),
                "rotation": axes(&row.rotation),
                "scale": axes(&row.scale),
                "euler": axes(&row.euler),
                "parent": parents.get(&row.hash).copied().unwrap_or(0),
            })
        })
        .collect();
    serde_json::json!({
        "format": anim::FORMAT,
        "name": clip.name,
        "source_clip": clip.name,
        "sample_rate": tracks.rate,
        "start": clip.start,
        "stop": clip.stop,
        "length": clip.length(),
        "loop": clip.looped,
        "events": clip.events.iter().map(|e| serde_json::json!({
            "time": e.time, "name": e.name,
        })).collect::<Vec<_>>(),
        "times": tracks.times.iter().map(|t| round64(*t, 6)).collect::<Vec<_>>(),
        "bones": rows,
    })
}

fn axes(rows: &Option<Vec<Vec<f64>>>) -> serde_json::Value {
    match rows {
        Some(rows) => serde_json::Value::Array(
            rows.iter()
                .map(|axis| {
                    serde_json::Value::Array(
                        axis.iter()
                            .map(|value| serde_json::json!(round64(*value, 5)))
                            .collect(),
                    )
                })
                .collect(),
        ),
        None => serde_json::Value::Null,
    }
}

fn round32(value: f32, places: i32) -> f64 {
    round64(value as f64, places)
}

fn round64(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

fn mesh_with_tree(bundle: &OpenedBundle, name: &str) -> Result<(i64, Mesh, Value), String> {
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
            let mesh = mesh::from_value_with_nodes(&tree, &bundle.nodes)
                .map_err(|e| format!("{name}: {e}"))?;
            return Ok((*path_id, mesh, tree));
        }
    }
    Err(format!("mesh {name:?} not found"))
}

fn load_bone_paths(path: &str) -> HashMap<u32, String> {
    if path.is_empty() {
        return HashMap::new();
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return HashMap::new();
    };
    let Some(map) = value.get("paths").and_then(|v| v.as_object()) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (key, value) in map {
        if let (Ok(hash), Some(name)) = (key.parse::<u32>(), value.as_str()) {
            out.insert(hash, name.to_string());
        }
    }
    out
}

fn write_pretty(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn safe(name: &str) -> String {
    let text: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    if text.is_empty() { "item".into() } else { text }
}

fn alias(name: &str) -> String {
    let Some(rest) = name.strip_prefix("c_") else {
        return name.to_string();
    };
    let Some(position) = rest.find('_') else {
        return name.to_string();
    };
    let mut tail = &rest[position + 1..];
    if let Some(stripped) = tail.strip_prefix("slg_") {
        tail = stripped;
    }
    if tail.is_empty() {
        name.to_string()
    } else {
        tail.to_string()
    }
}

fn short(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names_match_the_python_rule() {
        assert_eq!(
            safe("c_VepleySSR01_slg_body_lod0"),
            "c_VepleySSR01_slg_body_lod0"
        );
        assert_eq!(safe("a/b:c"), "a_b_c");
        assert_eq!(safe(""), "item");
    }

    #[test]
    fn aliases_strip_the_character_prefix() {
        assert_eq!(alias("c_AlvaSSR0102_slg_cloth1_lod0"), "cloth1_lod0");
        assert_eq!(
            alias("cw_GrozaSR0102_WL_slg_lod0"),
            "cw_GrozaSR0102_WL_slg_lod0"
        );
        assert_eq!(alias("plain"), "plain");
    }
}
