//! GameCache analysis: the one pass that every studio page reads.
//!
//! The only input is the installed game. A prior cache is never required.
//!
//! 1. scan every bundle once for mesh/texture/clip names, bone paths and
//!    prefab hierarchies, writing `names.json`, `clips.json` and `bones.json`;
//! 2. derive skins from the mesh-name convention (`skins.json`);
//! 3. build every roster skin pack;
//! 4. write `manifest.json` **last** so a half-written cache is never accepted.
//!
//! Two things keep the pass affordable on a 66 GB install:
//!
//! * one open per bundle collects every kind (the second full pass for bones
//!   and the separate clip walk are gone);
//! * only node 0 of each bundle is decompressed ([`asset::open_serialized`]).
//!   GF2 puts streaming texture bytes in a sibling `.resS` node that the scan
//!   never reads, so the pass reads a fraction of the folder instead of all of
//!   it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use exmt_core::asset;
use exmt_core::crc32::crc32;
use exmt_core::texconv::Texconv;
use exmt_core::typetree::{self, Value};

use crate::{pairings, skinpack};

pub const FORMAT: &str = "exmt-gamecache-1";
const MESH: i32 = 43;
const TEXTURE: i32 = 28;
const CLIP: i32 = 74;
const AVATAR: i32 = 90;

/// Returned to the UI when the user presses Stop; the frontend shows this as a
/// cancellation, not as a failure.
pub const CANCELLED_MESSAGE: &str = "已停止分析";
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Ask the running analysis to stop after the bundle/skin it is on.
pub fn cancel() {
    CANCELLED.store(true, Ordering::SeqCst);
}

fn cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub hash: String,
    pub meshes: Vec<String>,
    pub textures: Vec<String>,
    pub clips: Vec<String>,
    /// Bone paths recovered from this bundle, when the scan asked for them.
    pub bones: Vec<BoneRow>,
    /// Bytes actually read from disk (node 0 only), for the progress ETA.
    pub bytes_read: u64,
}

#[derive(Debug, Default)]
pub struct Index {
    pub bundles: usize,
    pub meshes: BTreeMap<String, Vec<String>>,
    pub mesh_order: Vec<String>,
    pub textures: BTreeMap<String, Vec<String>>,
    pub texture_order: Vec<String>,
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
    #[serde(skip)]
    pub part_order: Vec<String>,
    pub texture_count: usize,
    pub clip_count: usize,
    pub mesh_count: usize,
}

/// Scan one bundle for the three indexed object kinds.
pub fn scan_bundle(path: &Path) -> Scan {
    scan_bundle_with(path, false)
}

/// Scan one bundle once for names, clips and (optionally) bone paths.
///
/// Only node 0 of the bundle is decompressed: names and prefab hierarchies live
/// in the SerializedFile, while sibling `.resS` streams can be most of the
/// file. A second full read pass for bones is what made the old analysis read
/// the whole install twice.
pub fn scan_bundle_with(path: &Path, with_bones: bool) -> Scan {
    let hash = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut scan = Scan {
        hash,
        ..Scan::default()
    };
    let Ok((serialized, bytes_read)) = asset::open_serialized(path) else {
        return scan;
    };
    scan.bytes_read = bytes_read;
    for (_, obj) in &serialized.objects {
        let Some(ty) = serialized.type_of(obj) else {
            continue;
        };
        let bucket = match ty.class_id {
            MESH => &mut scan.meshes,
            TEXTURE => &mut scan.textures,
            CLIP => &mut scan.clips,
            _ => continue,
        };
        if let Some(name) = serialized.peek_name(obj) {
            bucket.push(name);
        }
    }
    if with_bones {
        scan.bones = scan_bones(&serialized);
    }
    scan
}

/// Scan `bundle_dir` in `workers` chunks, preserving file order.
pub fn scan_all(
    bundle_dir: &Path,
    limit: Option<usize>,
    workers: usize,
) -> Result<(Vec<Scan>, usize), String> {
    let files = limited_files(bundle_dir, limit)?;
    let total = files.len();
    let scans = scan_files(&files, workers, false, None)?;
    Ok((scans, total))
}

pub fn limited_files(bundle_dir: &Path, limit: Option<usize>) -> Result<Vec<PathBuf>, String> {
    let mut files = bundle_files(bundle_dir)?;
    if let Some(limit) = limit {
        if limit > 0 {
            files.truncate(limit);
        }
    }
    if files.is_empty() {
        return Err(format!("no .bundle files in {}", bundle_dir.display()));
    }
    Ok(files)
}

fn scan_files(
    files: &[PathBuf],
    workers: usize,
    with_bones: bool,
    tracker: Option<&ScanTracker<'_>>,
) -> Result<Vec<Scan>, String> {
    let total = files.len();
    let workers = workers.max(1).min(16).min(total.max(1));
    let chunk = (total + workers - 1) / workers.max(1);
    let scans: Vec<Scan> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in files.chunks(chunk.max(1)) {
            handles.push(scope.spawn(move || {
                let mut rows = Vec::with_capacity(part.len());
                for path in part {
                    if cancelled() {
                        break;
                    }
                    if let Some(tracker) = tracker {
                        tracker.start_file(path);
                    }
                    let scan = scan_bundle_with(path, with_bones);
                    if let Some(tracker) = tracker {
                        tracker.finish_file(scan.bytes_read);
                    }
                    rows.push(scan);
                }
                rows
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("scan worker"))
            .collect()
    });
    if cancelled() {
        return Err(CANCELLED_MESSAGE.into());
    }
    Ok(scans)
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
            if !index.textures.contains_key(name) {
                index.texture_order.push(name.clone());
            }
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
        "game_bundles": display_dir(bundle_dir),
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
        "bundle_dir": display_dir(bundle_dir),
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
                part_order: Vec::new(),
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
        if !working.skin.parts.contains_key(&part) {
            working.skin.part_order.push(part.clone());
        }
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

/// Everything one scan pass produces.
pub struct ScanOutcome {
    pub index: Index,
    /// `crc32(bone path) -> path`, empty when bones were not requested.
    pub bones: BTreeMap<String, String>,
    pub seconds: f64,
}

/// Scan every bundle once and write `names.json` / `clips.json` /
/// `bones.json` into `<cache>/GameCache`.
///
/// One pass collects names, clips and (optionally) bone paths, because opening
/// a bundle is what costs I/O: the old code read the whole install a second
/// time for bones.
pub fn build_indexes(
    bundle_dir: &Path,
    out_dir: &Path,
    limit: Option<usize>,
    workers: usize,
    with_bones: bool,
    built_at: &str,
    progress: Option<&Progress>,
) -> Result<ScanOutcome, String> {
    let started = Instant::now();
    let files = limited_files(bundle_dir, limit)?;
    let total = files.len();
    let tracker = progress.map(|progress| ScanTracker::new(&files, "scan", progress));
    let scans = scan_files(&files, workers, with_bones, tracker.as_ref())?;
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

    let mut paths: HashMap<u32, (String, i32)> = HashMap::new();
    if with_bones {
        for scan in &scans {
            for row in &scan.bones {
                match paths.get(&row.hash) {
                    None => {
                        paths.insert(row.hash, (row.path.clone(), row.priority));
                    }
                    Some((path, priority)) if path != &row.path && row.priority > *priority => {
                        paths.insert(row.hash, (row.path.clone(), row.priority));
                    }
                    _ => {}
                }
            }
        }
    }
    let bones: BTreeMap<String, String> = paths
        .iter()
        .map(|(hash, (path, _))| (hash.to_string(), path.clone()))
        .collect();
    if with_bones {
        let payload = serde_json::json!({
            "format": "exmt-bonenames-1",
            "built_at": built_at,
            "bundle_dir": display_dir(bundle_dir),
            "bundles": total,
            "names": bones.len(),
            "paths": bones,
        });
        write_json(&out_dir.join("bones.json"), &payload)?;
    }
    Ok(ScanOutcome {
        index,
        bones,
        seconds: started.elapsed().as_secs_f64(),
    })
}

#[derive(Debug, Clone)]
pub struct BoneRow {
    hash: u32,
    path: String,
    priority: i32,
}

/// Recover `crc32(transform path) -> path` from Avatars and prefab hierarchies.
fn scan_bones(serialized: &exmt_core::serialized::SerializedFile) -> Vec<BoneRow> {
    let mut out: HashMap<u32, (String, i32)> = HashMap::new();
    let mut names: HashMap<i64, String> = HashMap::new();
    let mut father: HashMap<i64, i64> = HashMap::new();
    let mut owner: HashMap<i64, i64> = HashMap::new();
    for (path_id, obj) in &serialized.objects {
        let Some(ty) = serialized.type_of(obj) else {
            continue;
        };
        match ty.class_id {
            AVATAR => {
                let Ok(tree) = typetree::read_value(serialized, obj) else {
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
                // `m_Name` is not the first GameObject field (m_Component and
                // m_Layer come before it), so this needs the full decode.
                let Ok(tree) = typetree::read_value(serialized, obj) else {
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
                let Ok(tree) = typetree::read_value(serialized, obj) else {
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

/// Absolute path for the cache files, without Windows' `\\?\` verbatim prefix.
///
/// `canonicalize()` returns a `\\?\`-prefixed path. The UI builds the bundle
/// folder from the user's install path, so a verbatim prefix in `manifest.json`
/// never compares equal -- a perfectly good cache then reads as
/// "game cache is elsewhere", which is exactly the failure that made a
/// finished analysis look like a failed one.
pub fn display_dir(path: &Path) -> String {
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy().replace('/', "\\");
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_string(),
        None => text,
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


#[derive(Debug, Clone)]
pub struct Options {
    pub workers: usize,
    pub limit: Option<usize>,
    pub with_bones: bool,
    pub with_packs: bool,
}

pub struct Progress {
    path: PathBuf,
    phases: Vec<serde_json::Value>,
    started: Instant,
    files_total: AtomicUsize,
}

impl Progress {
    fn new(path: PathBuf) -> Self {
        let progress = Progress {
            path,
            phases: Vec::new(),
            started: Instant::now(),
            files_total: AtomicUsize::new(0),
        };
        progress.write("starting", None, None);
        progress
    }

    fn start(&self, phase: &str) {
        self.write("running", Some(phase), None);
    }

    fn set_files_total(&self, total: usize) {
        self.files_total.store(total, Ordering::Relaxed);
    }

    fn done(&mut self, phase: &str, seconds: f64, detail: serde_json::Value) {
        let mut row = serde_json::json!({"phase": phase, "seconds": (seconds * 10.0).round() / 10.0});
        if let (Some(row), Some(extra)) = (row.as_object_mut(), detail.as_object()) {
            for (key, value) in extra {
                row.insert(key.clone(), value.clone());
            }
        }
        self.phases.push(row);
        self.write("running", None, None);
    }

    fn finish(&self, error: Option<&str>) {
        self.write(if error.is_some() { "error" } else { "done" }, None, error);
    }

    /// Mid-phase progress with the concrete file/skin being worked on.
    fn live(
        &self,
        phase: &str,
        done: usize,
        total: usize,
        current: &str,
        eta_seconds: Option<f64>,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        let payload = serde_json::json!({
            "state": "running",
            "phase": phase,
            "phases": self.phases,
            "seconds": (self.started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "files_total": self.files_total.load(Ordering::Relaxed),
            "live": {
                "phase": phase,
                "done": done,
                "total": total,
                "current": current,
                "eta_seconds": eta_seconds.map(|v| (v * 10.0).round() / 10.0),
                "bytes_done": bytes_done,
                "bytes_total": bytes_total,
                "seconds": (self.started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            },
        });
        // A monitor-style write: workers race rarely and a torn read is ignored
        // by the UI until the next tick, so a direct write is cheaper than a
        // temp-file rename per bundle.
        let _ = std::fs::write(&self.path, serde_json::to_string(&payload).unwrap_or_default());
    }

    fn write(&self, state: &str, phase: Option<&str>, error: Option<&str>) {
        let payload = serde_json::json!({
            "state": state,
            "phase": phase,
            "phases": self.phases,
            "seconds": (self.started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "files_total": self.files_total.load(Ordering::Relaxed),
            "error": error,
        });
        let _ = write_json(&self.path, &payload);
    }
}

/// Shared counters for one file-scanning phase (`names` / `bones`).
struct ScanTracker<'a> {
    progress: &'a Progress,
    phase: &'static str,
    total: usize,
    bytes_total: u64,
    done: AtomicUsize,
    bytes_done: AtomicU64,
    current: Mutex<String>,
    started: Instant,
    write_lock: Mutex<()>,
}

impl<'a> ScanTracker<'a> {
    fn new(files: &[PathBuf], phase: &'static str, progress: &'a Progress) -> Self {
        // The scan reads only node 0, so the files' sizes say little about the
        // remaining work. The ETA therefore advances by file count, and the
        // byte counters report what was actually read.
        ScanTracker {
            progress,
            phase,
            total: files.len(),
            bytes_total: 0,
            done: AtomicUsize::new(0),
            bytes_done: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            started: Instant::now(),
            write_lock: Mutex::new(()),
        }
    }

    fn start_file(&self, path: &Path) {
        let first = self.done.load(Ordering::Relaxed) == 0;
        if let Ok(mut current) = self.current.lock() {
            *current = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
        }
        if first {
            self.publish();
        }
    }

    fn finish_file(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        if done % 4 == 0 || done == self.total {
            self.publish();
        }
    }

    fn publish(&self) {
        let Ok(_guard) = self.write_lock.try_lock() else { return };
        let done = self.done.load(Ordering::Relaxed);
        let bytes = self.bytes_done.load(Ordering::Relaxed);
        let elapsed = self.started.elapsed().as_secs_f64();
        let eta = if bytes > 0 && self.bytes_total > bytes {
            Some(elapsed * (self.bytes_total - bytes) as f64 / bytes as f64)
        } else if done > 0 && done < self.total {
            Some(elapsed * (self.total - done) as f64 / done as f64)
        } else {
            None
        };
        let current = self.current.lock().map(|v| v.clone()).unwrap_or_default();
        self.progress.live(self.phase, done, self.total, &current, eta, bytes, self.bytes_total);
    }
}

fn is_roster(skin: &Skin) -> bool {
    !skin.parts.is_empty()
        && (skin.parts.keys().any(|part| part.starts_with("slg")) || skin.variant == "dorm")
}

fn pack_spec(
    out_dir: &Path,
    bundle_dir: &Path,
    index: &Index,
    catalog: &pairings::Catalog,
    skin: &Skin,
    built_at: &str,
) -> skinpack::Spec {
    let prefix = format!("{}_", skin.id);
    let mut meshes = Vec::new();
    let mut mesh_names = Vec::new();
    let order: Vec<&String> = if skin.part_order.is_empty() {
        skin.parts.keys().collect()
    } else {
        skin.part_order.iter().collect()
    };
    for part in order {
        let Some(mesh) = skin.parts.get(part) else { continue };
        mesh_names.push(mesh.clone());
        meshes.push(skinpack::MeshSpec {
            mesh: mesh.clone(),
            bundles: skin.part_bundles.get(part).cloned().unwrap_or_default(),
            bundle: None,
        });
    }
    let groups = catalog.pair(&mesh_names);

    let mut wanted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for names in groups.values() {
        wanted.extend(names.iter().cloned());
    }
    for name in index.textures.keys() {
        if name.starts_with(&prefix) {
            wanted.insert(name.clone());
        }
    }
    let mut by_bundle: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in wanted {
        if let Some(bundle) = index.textures.get(&name).and_then(|rows| rows.first()) {
            by_bundle.entry(bundle.clone()).or_default().push(name);
        }
    }
    let textures = by_bundle
        .into_iter()
        .map(|(bundle, names)| skinpack::TextureSpec { bundle, names })
        .collect();

    let mut maps = Vec::new();
    for mesh in &mesh_names {
        let Some(names) = groups.get(mesh) else { continue };
        let Some(texture) = pairings::diffuse_of(names) else { continue };
        let Some(bundle) = index.textures.get(&texture).and_then(|rows| rows.first()) else { continue };
        maps.push(skinpack::MapSpec { mesh: mesh.clone(), texture, bundle: bundle.clone() });
    }

    skinpack::Spec {
        key: skin.id.clone(),
        dir: out_dir.join("skins").join(&skin.id).display().to_string(),
        bundle_dir: display_dir(bundle_dir),
        cache_built_at: built_at.to_string(),
        meshes,
        textures,
        groups,
        maps,
        thumb_size: Some(160),
        map_size: Some(1024),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_packs(
    out_dir: &Path,
    bundle_dir: &Path,
    index: &Index,
    catalog: &pairings::Catalog,
    skins: &[Skin],
    texconv: &Texconv,
    workers: usize,
    limit: Option<usize>,
    built_at: &str,
    progress: &mut Progress,
) -> usize {
    let mut roster: Vec<&Skin> = skins.iter().filter(|skin| is_roster(skin)).collect();
    if let Some(limit) = limit {
        roster.truncate(limit.max(1));
    }
    if roster.is_empty() {
        return 0;
    }
    progress.start("packs");
    // Spec construction is part of the phase, not free: pairing every skin used
    // to re-derive the texture catalogue and went unmeasured.
    let started = Instant::now();
    let specs: Vec<skinpack::Spec> = roster
        .iter()
        .map(|skin| pack_spec(out_dir, bundle_dir, index, catalog, skin, built_at))
        .collect();
    let total = specs.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let ok = std::sync::atomic::AtomicUsize::new(0);
    let total_started = progress.started;
    let thread_count = workers.clamp(1, 8).min(total);
    let chunk = (total + thread_count - 1) / thread_count;
    let progress_path = progress.path.clone();
    let phases = progress.phases.clone();
    std::thread::scope(|scope| {
        for part in specs.chunks(chunk) {
            let done = &done;
            let ok = &ok;
            let progress_path = &progress_path;
            let phases = &phases;
            scope.spawn(move || {
                for spec in part {
                    if cancelled() {
                        break;
                    }
                    let started_count = done.load(std::sync::atomic::Ordering::Relaxed);
                    if started_count == 0 {
                        let payload = serde_json::json!({
                            "state": "running",
                            "phase": "packs",
                            "phases": phases,
                            "seconds": (total_started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                            "live": {
                                "phase": "packs",
                                "done": 0,
                                "total": total,
                                "current": spec.key,
                                "ok": ok.load(std::sync::atomic::Ordering::Relaxed),
                                "seconds": (total_started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                            },
                        });
                        let _ = std::fs::write(progress_path, serde_json::to_string(&payload).unwrap_or_default());
                    }
                    if skinpack::build(spec, texconv).is_ok() {
                        ok.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    let finished = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if finished == 1 || finished % 5 == 0 || finished == total {
                        let elapsed = total_started.elapsed().as_secs_f64();
                        let phase_elapsed = started.elapsed().as_secs_f64();
                        let eta = if finished > 0 && finished < total {
                            Some(phase_elapsed * (total - finished) as f64 / finished as f64)
                        } else {
                            None
                        };
                        let payload = serde_json::json!({
                            "state": "running",
                            "phase": "packs",
                            "phases": phases,
                            "seconds": (elapsed * 10.0).round() / 10.0,
                            "live": {
                                "phase": "packs",
                                "done": finished,
                                "total": total,
                                "current": spec.key,
                                "eta_seconds": eta.map(|v| (v * 10.0).round() / 10.0),
                                "ok": ok.load(std::sync::atomic::Ordering::Relaxed),
                                "seconds": (elapsed * 10.0).round() / 10.0,
                            },
                        });
                        let _ = std::fs::write(progress_path, serde_json::to_string(&payload).unwrap_or_default());
                    }
                }
            });
        }
    });
    let ok = ok.load(std::sync::atomic::Ordering::Relaxed);
    progress.done("packs", started.elapsed().as_secs_f64(), serde_json::json!({
        "packs": ok, "skins": total,
    }));
    ok
}

/// Run the whole GameCache analysis and return the manifest JSON line.
pub fn run(
    bundle_dir: &Path,
    cache_root: &Path,
    options: &Options,
    texconv: &Texconv,
) -> Result<String, String> {
    CANCELLED.store(false, Ordering::SeqCst);
    let result = run_inner(bundle_dir, cache_root, options, texconv);
    if let Err(error) = &result {
        let _ = std::fs::remove_dir_all(cache_root.join("GameCache.new"));
        let progress_path = cache_root.join("GameCache.progress.json");
        let _ = write_json(&progress_path, &serde_json::json!({
            "state": "error",
            "phase": null,
            "phases": [],
            "seconds": 0.0,
            "error": error,
        }));
    }
    result
}

fn run_inner(
    bundle_dir: &Path,
    cache_root: &Path,
    options: &Options,
    texconv: &Texconv,
) -> Result<String, String> {
    if !bundle_dir.is_dir() {
        return Err(format!("game asset directory does not exist: {}", bundle_dir.display()));
    }
    let bundles = bundle_files(bundle_dir)?;
    if bundles.is_empty() {
        return Err(format!("no .bundle files in {}", bundle_dir.display()));
    }

    let final_dir = cache_root.join("GameCache");
    // Build into a sibling first: Stop or a crash must leave the working cache
    // untouched. Only a complete manifest is swapped into `GameCache`.
    let work_dir = cache_root.join("GameCache.new");
    // Leftovers from an interrupted run must not block a fresh analysis; the
    // only input this needs is the game install itself.
    for stale in ["GameCache.new", "GameCache.old"] {
        let _ = std::fs::remove_dir_all(cache_root.join(stale));
    }
    std::fs::create_dir_all(&work_dir)
        .map_err(|e| format!("create {}: {e}", work_dir.display()))?;

    let built_at = skinpack::local_timestamp();
    let total = Instant::now();
    let mut progress = Progress::new(cache_root.join("GameCache.progress.json"));
    progress.set_files_total(bundles.len());

    progress.start("scan");
    let ScanOutcome { index, bones, seconds: scan_seconds } = build_indexes(
        bundle_dir,
        &work_dir,
        options.limit,
        options.workers,
        options.with_bones,
        &built_at,
        Some(&progress),
    )?;
    progress.done("scan", scan_seconds, serde_json::json!({
        "scanned": index.bundles,
        "meshes": index.meshes.len(),
        "textures": index.textures.len(),
        "clips": index.clips.len(),
        "bones": bones.len(),
    }));

    progress.start("skins");
    let skins_started = Instant::now();
    let skins = skins_of(&index);
    write_json(&work_dir.join("skins.json"), &serde_json::json!({
        "format": "exmt-skins-1",
        "built_at": built_at,
        "skins": skins,
    }))?;
    progress.done(
        "skins",
        skins_started.elapsed().as_secs_f64(),
        serde_json::json!({"skins": skins.len()}),
    );

    let mut pack_count = 0usize;
    if options.with_packs {
        let catalog = pairings::Catalog::new(&index.textures, &index.texture_order);
        pack_count = build_packs(
            &work_dir,
            bundle_dir,
            &index,
            &catalog,
            &skins,
            texconv,
            options.workers,
            options.limit,
            &built_at,
            &mut progress,
        );
        if cancelled() {
            return Err(CANCELLED_MESSAGE.into());
        }
    }

    let bone_count = bones.len();
    let manifest = serde_json::json!({
        "format": FORMAT,
        "built_at": built_at,
        "bundle_dir": display_dir(bundle_dir),
        "cache_dir": display_dir(&final_dir),
        "bundles": bundles.len(),
        "limited_to": options.limit,
        "workers": options.workers,
        "counts": {
            "meshes": index.meshes.len(),
            "textures": index.textures.len(),
            "clips": index.clips.len(),
            "skins": skins.len(),
            "packs": pack_count,
            "bones": bone_count,
        },
        "seconds": (total.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    });
    write_json(&work_dir.join("manifest.json"), &manifest)?;

    // Swap only after the manifest is on disk: this is the point where the new
    // cache becomes the one the UI reads. Stop cannot reach this branch.
    let old_dir = cache_root.join("GameCache.old");
    let _ = std::fs::remove_dir_all(&old_dir);
    if final_dir.is_dir() {
        std::fs::rename(&final_dir, &old_dir)
            .map_err(|e| format!("move old cache: {e}"))?;
    }
    if let Err(error) = std::fs::rename(&work_dir, &final_dir) {
        if old_dir.is_dir() {
            let _ = std::fs::rename(&old_dir, &final_dir);
        }
        return Err(format!("install new cache: {error}"));
    }
    let _ = std::fs::remove_dir_all(&old_dir);
    progress.finish(None);
    serde_json::to_string(&manifest).map_err(|e| e.to_string())
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
    fn cache_paths_have_no_verbatim_prefix() {
        // canonicalize() returns `\\?\C:\...`; the UI compares the bundle folder
        // it built from the install path, so the prefix must never be written.
        let dir = std::env::temp_dir();
        let shown = display_dir(&dir);
        assert!(!shown.starts_with(r"\\?\"), "still verbatim: {shown}");
        assert!(!shown.contains('/'), "separators not unified: {shown}");
        assert_eq!(display_dir(&dir.join("nope")).ends_with("nope"), true);
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


