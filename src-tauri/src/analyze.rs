//! GameCache analysis: the one pass that every studio page reads.
//!
//! Pipeline (same as `core/exmt_core/analyze.py`):
//!
//! 1. scan every bundle once for mesh/texture/clip **names** (`peek_name`, no
//!    object payloads), writing `names.json` and `clips.json`;
//! 2. derive skins from the mesh-name convention (`skins.json`);
//! 3. build every roster skin pack (not ported yet);
//! 4. scan prefabs/Avatars for bone paths (`bones.json`, not ported yet);
//! 5. write `manifest.json` **last** so a half-written cache is never accepted.
//!
//! The scan is merged: Python walks the folder once for names and again (metadata
//! only) for clips; here one open per bundle collects all three kinds.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use exmt_core::asset;
use exmt_core::crc32::crc32;
use exmt_core::typetree::{self, Value};

pub const FORMAT: &str = "exmt-gamecache-1";
const MESH: i32 = 43;
const TEXTURE: i32 = 28;
const CLIP: i32 = 74;
const AVATAR: i32 = 90;

#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub hash: String,
    pub meshes: Vec<String>,
    pub textures: Vec<String>,
    pub clips: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Index {
    pub bundles: usize,
    pub meshes: BTreeMap<String, Vec<String>>,
    pub mesh_order: Vec<String>,
    pub textures: BTreeMap<String, Vec<String>>,
    pub clips: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub variant: String,
    pub bundle: String,
    pub parts: BTreeMap<String, String>,
    pub part_bundles: BTreeMap<String, Vec<String>>,
    pub texture_count: usize,
    pub clip_count: usize,
    pub mesh_count: usize,
}

/// Scan one bundle for the three indexed object kinds.
pub fn scan_bundle(path: &Path) -> Scan {
    let hash = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut scan = Scan {
        hash: hash.clone(),
        ..Scan::default()
    };
    let Ok(bundle) = asset::open(path) else {
        return scan;
    };
    for (_, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        let bucket = match ty.class_id {
            MESH => &mut scan.meshes,
            TEXTURE => &mut scan.textures,
            CLIP => &mut scan.clips,
            _ => continue,
        };
        if let Some(name) = bundle.serialized.peek_name(obj) {
            bucket.push(name);
        }
    }
    scan
}

/// Scan `bundle_dir` in `workers` chunks, preserving file order.
pub fn scan_all(
    bundle_dir: &Path,
    limit: Option<usize>,
    workers: usize,
) -> Result<(Vec<Scan>, usize), String> {
    let mut files = bundle_files(bundle_dir)?;
    if let Some(limit) = limit {
        if limit > 0 {
            files.truncate(limit);
        }
    }
    if files.is_empty() {
        return Err(format!("no .bundle files in {}", bundle_dir.display()));
    }
    let total = files.len();
    let workers = workers.max(1).min(16).min(total);
    let chunk = (total + workers - 1) / workers;
    let scans: Vec<Scan> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in files.chunks(chunk) {
            handles.push(scope.spawn(move || {
                part.iter()
                    .map(|path| scan_bundle(path))
                    .collect::<Vec<_>>()
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("scan worker"))
            .collect()
    });
    Ok((scans, total))
}

pub fn bundle_files(bundle_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        std::fs::read_dir(bundle_dir).map_err(|e| format!("read {}: {e}", bundle_dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bundle"))
        .collect();
    files.sort();
    Ok(files)
}

pub fn merge(scans: &[Scan]) -> Index {
    let mut index = Index {
        bundles: scans.len(),
        ..Index::default()
    };
    for scan in scans {
        for name in &scan.meshes {
            if !index.meshes.contains_key(name) {
                index.mesh_order.push(name.clone());
            }
            push_bundle(&mut index.meshes, name, &scan.hash);
        }
        for name in &scan.textures {
            push_bundle(&mut index.textures, name, &scan.hash);
        }
        for name in &scan.clips {
            push_bundle(&mut index.clips, name, &scan.hash);
        }
    }
    index
}

fn push_bundle(map: &mut BTreeMap<String, Vec<String>>, name: &str, hash: &str) {
    let bucket = map.entry(name.to_string()).or_default();
    if !bucket.iter().any(|h| h == hash) {
        bucket.push(hash.to_string());
    }
}

/// `names.json` payload.
pub fn names_json(index: &Index, bundle_dir: &Path, built_at: &str) -> serde_json::Value {
    serde_json::json!({
        "format": "exmt-meshindex-2",
        "built_at": built_at,
        "game_bundles": bundle_dir.display().to_string(),
        "bundles": index.bundles,
        "mesh_count": index.meshes.len(),
        "texture_count": index.textures.len(),
        "meshes": index.meshes,
        "textures": index.textures,
    })
}

/// `clips.json` payload.
pub fn clips_json(index: &Index, bundle_dir: &Path, built_at: &str) -> serde_json::Value {
    let mut anim_bundles: Vec<String> = index
        .clips
        .values()
        .flatten()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    anim_bundles.sort();
    serde_json::json!({
        "format": "exmt-animindex-1",
        "built_at": built_at,
        "bundle_dir": bundle_dir.display().to_string(),
        "bundles": index.bundles,
        "clip_count": index.clips.len(),
        "anim_bundles": anim_bundles,
        "clips": index.clips,
    })
}

/// Derive the roster from mesh names.
///
/// Only the best LOD of each part is kept: lowest index, and on a tie the full
/// mesh over the mobile (`lodm`) copy. The suffix after the LOD token stays part
/// of the part name (`_Dorm` is a different outfit), and emitter markers are
/// skipped because they carry no skinning.
pub fn skins_of(index: &Index) -> Vec<Skin> {
    struct Working {
        skin: Skin,
        lods: HashMap<String, (i32, i32)>,
        bundles: Vec<String>,
    }
    let mut skins: BTreeMap<String, Working> = BTreeMap::new();
    for mesh in &index.mesh_order {
        let bundles = &index.meshes[mesh];
        let Some((prefix, name, rest)) = split_mesh(mesh) else {
            continue;
        };
        let (part, rank) = match lod_of(&rest) {
            Some((head, mobile, number, tail)) => (
                format!("{head}{tail}").to_lowercase(),
                (number, if mobile { 1 } else { 0 }),
            ),
            None => (rest.to_lowercase(), (0, 0)),
        };
        if part.contains("emitter") {
            continue;
        }
        let key = format!("c_{name}");
        let working = skins.entry(key.clone()).or_insert_with(|| Working {
            skin: Skin {
                id: key,
                name: name.clone(),
                variant: if name.to_lowercase().ends_with("dorm") {
                    "dorm".into()
                } else {
                    "slg".into()
                },
                bundle: String::new(),
                parts: BTreeMap::new(),
                part_bundles: BTreeMap::new(),
                texture_count: 0,
                clip_count: 0,
                mesh_count: 0,
            },
            lods: HashMap::new(),
            bundles: Vec::new(),
        });
        if let Some(previous) = working.lods.get(&part) {
            if *previous <= rank {
                continue;
            }
        }
        working.lods.insert(part.clone(), rank);
        working.skin.parts.insert(part.clone(), mesh.clone());
        working.skin.part_bundles.insert(part, bundles.clone());
        for hash in bundles {
            if !working.bundles.contains(hash) {
                working.bundles.push(hash.clone());
            }
        }
        let _ = prefix;
    }

    let mut rows: Vec<Skin> = Vec::new();
    for (_, mut working) in skins {
        let prefix = format!("{}_", working.skin.id);
        let weapon = format!("cw_{}_", working.skin.name);
        working.skin.texture_count = index
            .textures
            .keys()
            .filter(|name| name.starts_with(&prefix) || name.starts_with(&weapon))
            .count();
        working.skin.clip_count = index
            .clips
            .keys()
            .filter(|name| name.starts_with(&prefix) || name.starts_with(&weapon))
            .count();
        working.skin.bundle = most_common(&working.bundles);
        working.skin.mesh_count = working.skin.parts.len();
        rows.push(working.skin);
    }
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    rows
}

fn most_common(values: &[String]) -> String {
    let mut best = String::new();
    let mut best_count = 0usize;
    for value in values {
        let count = values.iter().filter(|other| *other == value).count();
        if count > best_count {
            best = value.clone();
            best_count = count;
        }
    }
    best
}

/// `c_Name_rest` / `cw_Name_rest` without pulling in a regex engine.
fn split_mesh(name: &str) -> Option<(&'static str, String, String)> {
    let (prefix, rest) = if let Some(rest) = name.strip_prefix("c_") {
        ("c", rest)
    } else if let Some(rest) = name.strip_prefix("cw_") {
        ("cw", rest)
    } else {
        return None;
    };
    let cut = rest.find('_')?;
    if cut == 0 {
        return None;
    }
    let (character, tail) = rest.split_at(cut);
    if !character.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let tail = &tail[1..];
    if tail.is_empty() {
        return None;
    }
    Some((prefix, character.to_string(), tail.to_string()))
}

/// Match `^(.*?)_lod(m?)(\d+)(.*)$` case-insensitively.
fn lod_of(rest: &str) -> Option<(String, bool, i32, String)> {
    let lower = rest.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(found) = lower[search..].find("_lod") {
        let at = search + found;
        let mut pos = at + 4;
        let mobile = lower[pos..].starts_with('m');
        if mobile {
            pos += 1;
        }
        let digits_start = pos;
        while pos < lower.len() && lower.as_bytes()[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos > digits_start {
            let number: i32 = lower[digits_start..pos].parse().unwrap_or(0);
            return Some((
                rest[..at].to_string(),
                mobile,
                number,
                rest[pos..].to_string(),
            ));
        }
        search = at + 1;
    }
    None
}

/// Write both indexes into `<cache>/GameCache`, atomically.
pub fn build_indexes(
    bundle_dir: &Path,
    out_dir: &Path,
    limit: Option<usize>,
    workers: usize,
    built_at: &str,
) -> Result<(Index, f64), String> {
    let started = Instant::now();
    let (scans, total) = scan_all(bundle_dir, limit, workers)?;
    let mut index = merge(&scans);
    index.bundles = total;
    write_json(
        &out_dir.join("names.json"),
        &names_json(&index, bundle_dir, built_at),
    )?;
    write_json(
        &out_dir.join("clips.json"),
        &clips_json(&index, bundle_dir, built_at),
    )?;
    Ok((index, started.elapsed().as_secs_f64()))
}

#[derive(Debug, Clone)]
struct BoneRow {
    hash: u32,
    path: String,
    priority: i32,
}

/// Recover `crc32(transform path) -> path` from Avatars and prefab hierarchies.
pub fn build_bones(
    bundle_dir: &Path,
    out_dir: &Path,
    limit: Option<usize>,
    workers: usize,
    built_at: &str,
) -> Result<(usize, usize), String> {
    let mut files = bundle_files(bundle_dir)?;
    if let Some(limit) = limit {
        if limit > 0 {
            files.truncate(limit);
        }
    }
    let total = files.len();
    let workers = workers.max(1).min(16).min(total.max(1));
    let chunk = (total + workers - 1) / workers.max(1);
    let scans: Vec<Vec<BoneRow>> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in files.chunks(chunk.max(1)) {
            handles.push(scope.spawn(move || {
                part.iter()
                    .map(|path| scan_bones_bundle(path))
                    .collect::<Vec<_>>()
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("bone worker"))
            .collect()
    });

    let mut paths: HashMap<u32, (String, i32)> = HashMap::new();
    for rows in scans {
        for row in rows {
            match paths.get(&row.hash) {
                None => {
                    paths.insert(row.hash, (row.path, row.priority));
                }
                Some((path, priority)) if path != &row.path && row.priority > *priority => {
                    paths.insert(row.hash, (row.path, row.priority));
                }
                _ => {}
            }
        }
    }
    let sorted: BTreeMap<String, String> = paths
        .iter()
        .map(|(hash, (path, _))| (hash.to_string(), path.clone()))
        .collect();
    let payload = serde_json::json!({
        "format": "exmt-bonenames-1",
        "built_at": built_at,
        "bundle_dir": bundle_dir.display().to_string(),
        "bundles": total,
        "names": sorted.len(),
        "paths": sorted,
    });
    write_json(&out_dir.join("bones.json"), &payload)?;
    Ok((total, sorted.len()))
}

fn scan_bones_bundle(path: &Path) -> Vec<BoneRow> {
    let Ok(bundle) = asset::open(path) else {
        return Vec::new();
    };
    let mut out: HashMap<u32, (String, i32)> = HashMap::new();
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut father: HashMap<i64, i64> = HashMap::new();
    let mut owner: HashMap<i64, i64> = HashMap::new();
    for (path_id, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        match ty.class_id {
            AVATAR => {
                let Ok(tree) = typetree::read_value(&bundle.serialized, obj) else {
                    continue;
                };
                let Some(rows) = tree.get("m_TOS").and_then(|v| v.as_array()) else {
                    continue;
                };
                for row in rows {
                    if let Some((hash, text)) = pair_value(row) {
                        if !text.is_empty() {
                            out.insert(hash, (text, 2));
                        }
                    }
                }
            }
            1 => {
                let Ok(tree) = typetree::read_value(&bundle.serialized, obj) else {
                    continue;
                };
                names.insert(
                    *path_id,
                    tree.get("m_Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                );
            }
            4 => {
                let Ok(tree) = typetree::read_value(&bundle.serialized, obj) else {
                    continue;
                };
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
            _ => {}
        }
    }

    for &tid in father.keys() {
        let mut parts: Vec<String> = Vec::new();
        let mut cur = tid;
        let mut guard = 0;
        while cur != 0 && father.contains_key(&cur) && guard < 64 {
            if let Some(name) = owner.get(&cur).and_then(|id| names.get(id)) {
                if !name.is_empty() {
                    parts.push(name.clone());
                }
            }
            cur = father[&cur];
            guard += 1;
        }
        if parts.is_empty() {
            continue;
        }
        parts.reverse();
        add_bone_path(&mut out, &parts.join("/"), 1);
    }

    out.into_iter()
        .map(|(hash, (path, priority))| BoneRow {
            hash,
            path,
            priority,
        })
        .collect()
}

fn add_bone_path(out: &mut HashMap<u32, (String, i32)>, path: &str, priority: i32) {
    let parts: Vec<&str> = path.split('/').collect();
    for start in 0..parts.len() {
        let suffix = parts[start..].join("/");
        if suffix.is_empty() {
            continue;
        }
        let hash = crc32(suffix.as_bytes());
        match out.get(&hash) {
            None => {
                out.insert(hash, (suffix, priority));
            }
            Some((_, previous)) if priority > *previous => {
                out.insert(hash, (suffix, priority));
            }
            _ => {}
        }
    }
}

/// A typetree `pair` may arrive as `Pair`, a two-item array, or a struct.
fn pair_value(value: &Value) -> Option<(u32, String)> {
    match value {
        Value::Pair(first, second) => Some((first.as_u32()?, second.as_str()?.to_string())),
        Value::Array(items) if items.len() >= 2 => {
            Some((items[0].as_u32()?, items[1].as_str()?.to_string()))
        }
        Value::Struct(fields) => {
            let first = fields.iter().find(|(k, _)| k == "first" || k == "key")?;
            let second = fields.iter().find(|(k, _)| k == "second" || k == "value")?;
            Some((first.1.as_u32()?, second.1.as_str()?.to_string()))
        }
        _ => None,
    }
}

pub fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(meshes: &[(&str, &str)], textures: &[&str], clips: &[&str]) -> Index {
        let mut index = Index::default();
        for (name, hash) in meshes {
            if !index.meshes.contains_key(*name) {
                index.mesh_order.push((*name).to_string());
            }
            index
                .meshes
                .entry((*name).into())
                .or_default()
                .push((*hash).into());
        }
        for name in textures {
            index
                .textures
                .entry((*name).into())
                .or_default()
                .push("t".into());
        }
        for name in clips {
            index
                .clips
                .entry((*name).into())
                .or_default()
                .push("a".into());
        }
        index
    }

    #[test]
    fn splits_mesh_names_and_lod_tokens() {
        assert_eq!(
            split_mesh("c_AlvaSSR0102_slg_body_lod0").unwrap().1,
            "AlvaSSR0102"
        );
        assert_eq!(split_mesh("cw_Groza_WL_lod0").unwrap().0, "cw");
        assert!(split_mesh("B_body_d").is_none());
        let (part, mobile, n, tail) = lod_of("slg_cloth_lod0_Dorm").unwrap();
        assert_eq!(
            (part.as_str(), mobile, n, tail.as_str()),
            ("slg_cloth", false, 0, "_Dorm")
        );
        assert!(lod_of("slg_body").is_none());
    }

    #[test]
    fn skins_keep_the_lowest_lod_and_skip_emitters() {
        let index = index(
            &[
                ("c_A_slg_body_lod0", "b1"),
                ("c_A_slg_body_lod1", "b1"),
                ("c_A_slg_body_lodm0", "b1"),
                ("c_A_slg_hair_lod0", "b2"),
                ("c_A_slg_emitter_lod0", "b2"),
            ],
            &["c_A_slg_body_d", "c_A_slg_hair_d", "cw_A_WL_d"],
            &["c_A_slg_idle_N"],
        );
        let rows = skins_of(&index);
        assert_eq!(rows.len(), 1);
        let skin = &rows[0];
        assert_eq!(skin.id, "c_A");
        assert_eq!(skin.parts["slg_body"], "c_A_slg_body_lod0");
        assert_eq!(skin.parts.len(), 2);
        assert_eq!(skin.texture_count, 3);
        assert_eq!(skin.clip_count, 1);
        assert_eq!(skin.bundle, "b1");
    }

    #[test]
    fn dorm_skin_variant_and_suffix_are_preserved() {
        let index = index(&[("c_AgentDorm_slg_body_lod0_Dorm", "b")], &[], &[]);
        let rows = skins_of(&index);
        assert_eq!(rows[0].variant, "dorm");
        assert_eq!(
            rows[0].parts["slg_body_dorm"],
            "c_AgentDorm_slg_body_lod0_Dorm"
        );
    }
}
