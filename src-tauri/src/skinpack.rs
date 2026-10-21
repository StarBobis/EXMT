//! Rust implementation of the `skinpack` core command.
//!
//! One call resolves a skin into the files the studio caches under
//! `<cache>/skins/<key>/`:
//!
//! * `meshes.json` -- submesh rows (verts/tris/bones/bundle),
//! * `geometry.json` -- base64 typed arrays for three.js,
//! * `thumbs.json` -- downscaled PNG thumbnails (base64),
//! * `groups.json` -- which textures belong to which submesh,
//! * `diffuse.json` -- diffused map per submesh (base64 PNG),
//! * `meta.json` -- `{format, version, key, built_at, cache_built_at}`.
//!
//! The JSON shapes are copied from `core/exmt_core/skinpack.py` so an existing
//! cache can be read by the same frontend, and a pack built by either
//! implementation is interchangeable. Image work runs through DirectXTex's
//! `texconv.exe`; bundles are opened at most once per hash.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use exmt_core::asset;
use exmt_core::base64;
use exmt_core::container::Node;
use exmt_core::skin;
use exmt_core::texconv::Texconv;
use exmt_core::texture::Texture;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "exmt-skinpack-2";

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub key: String,
    pub dir: String,
    pub bundle_dir: String,
    #[serde(default)]
    pub cache_built_at: String,
    #[serde(default)]
    pub meshes: Vec<MeshSpec>,
    #[serde(default)]
    pub textures: Vec<TextureSpec>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub maps: Vec<MapSpec>,
    #[serde(default)]
    pub thumb_size: Option<u32>,
    #[serde(default)]
    pub map_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct MeshSpec {
    pub mesh: String,
    #[serde(default)]
    pub bundles: Vec<String>,
    #[serde(default)]
    pub bundle: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TextureSpec {
    pub bundle: String,
    #[serde(default)]
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MapSpec {
    pub mesh: String,
    pub texture: String,
    pub bundle: String,
}

#[derive(Debug, Serialize)]
struct MeshRow {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    verts: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tris: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bones: Option<usize>,
    bundle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Debug, Serialize)]
struct GeometryRow {
    name: String,
    count: u32,
    positions: String,
    normals: Option<String>,
    uvs: Option<String>,
    indices: String,
    bounds: Bounds,
    bundle: String,
}

#[derive(Debug, Serialize)]
struct ThumbRow {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    png: Option<String>,
    bundle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct Summary {
    format: &'static str,
    key: String,
    dir: String,
    meshes: usize,
    geometry: usize,
    textures: usize,
    maps: usize,
    seconds: f64,
}

/// One image on its way to PNG: decoded RGBA waiting for the batch encoder,
/// already-encoded PNG (texconv fallback), or a failure to report in the row.
enum Pending {
    Image { index: usize, width: u32, height: u32 },
    Png { png: Vec<u8>, width: u32, height: u32 },
    Failed(String),
}

/// Decode one texture for the cache: native + resize when possible, otherwise
/// the texconv path (unsupported formats and odd payloads).
fn decode_pending(
    texture: &Texture,
    nodes: &[Node],
    texconv: &Texconv,
    limit: usize,
    images: &mut Vec<(u32, u32, Vec<u8>)>,
) -> Pending {
    if let Some((width, height, rgba)) = texture.decode_rgba_resized(nodes, Some(limit)) {
        let index = images.len();
        images.push((width, height, rgba));
        return Pending::Image { index, width, height };
    }
    match texture.decode_png(nodes, texconv, Some(limit)) {
        Ok((width, height, png)) => Pending::Png { png, width, height },
        Err(e) => Pending::Failed(e.to_string()),
    }
}

fn resolve_pending<'a>(
    pending: &'a Pending,
    pngs: &'a [Result<Vec<u8>, String>],
) -> Result<(u32, u32, &'a [u8]), String> {
    match pending {
        Pending::Image { index, width, height } => match pngs.get(*index) {
            Some(Ok(png)) => Ok((*width, *height, png)),
            Some(Err(e)) => Err(e.clone()),
            None => Err("missing encoded image".into()),
        },
        Pending::Png { png, width, height } => Ok((*width, *height, png)),
        Pending::Failed(e) => Err(e.clone()),
    }
}

/// Bundles opened so far, keyed by hash.
struct Cache {
    dir: PathBuf,
    opened: HashMap<String, asset::OpenedBundle>,
}

impl Cache {
    fn new(dir: &Path) -> Self {
        Cache {
            dir: dir.to_path_buf(),
            opened: HashMap::new(),
        }
    }

    fn get(&mut self, hash: &str) -> Result<&asset::OpenedBundle, String> {
        if !self.opened.contains_key(hash) {
            let path = self.dir.join(format!("{hash}.bundle"));
            let opened = asset::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            self.opened.insert(hash.to_string(), opened);
        }
        Ok(self.opened.get(hash).expect("just inserted"))
    }
}

/// Run one skinpack spec and return the summary JSON line the CLI prints.
pub fn run(spec_path: &Path, texconv: &Texconv) -> Result<String, String> {
    let text = std::fs::read_to_string(spec_path)
        .map_err(|e| format!("read spec {}: {e}", spec_path.display()))?;
    let spec: Spec = serde_json::from_str(&text).map_err(|e| format!("parse spec: {e}"))?;
    build(&spec, texconv)
}

/// Build one skin pack from an in-memory spec (used by `analyze`).
pub fn build(spec: &Spec, texconv: &Texconv) -> Result<String, String> {
    let started = Instant::now();

    let out_dir = Path::new(&spec.dir);
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let mut cache = Cache::new(Path::new(&spec.bundle_dir));

    // --- meshes + geometry -------------------------------------------------
    let mut mesh_rows = Vec::new();
    let mut geometry_rows = Vec::new();
    for item in &spec.meshes {
        let mut candidates = item.bundles.clone();
        if candidates.is_empty() {
            if let Some(bundle) = &item.bundle {
                candidates.push(bundle.clone());
            }
        }
        let mut done = false;
        let mut last_error = String::new();
        for hash in &candidates {
            let found = cache.get(hash).and_then(|bundle| {
                let mesh = bundle.mesh(&item.mesh).map_err(|e| e.to_string())?;
                let geometry = skin::geometry(&mesh).map_err(|e| e.to_string())?;
                Ok((mesh, geometry))
            });
            match found {
                Ok((mesh, geometry)) => {
                    geometry_rows.push(GeometryRow {
                        name: geometry.name,
                        count: geometry.count,
                        positions: geometry.positions,
                        normals: geometry.normals,
                        uvs: geometry.uvs,
                        indices: geometry.indices,
                        bounds: Bounds {
                            min: geometry.bounds_min,
                            max: geometry.bounds_max,
                        },
                        bundle: hash.clone(),
                    });
                    mesh_rows.push(MeshRow {
                        name: mesh.name,
                        verts: Some(mesh.vertex_count),
                        tris: Some(mesh.indices.len() / 3),
                        bones: Some(mesh.bone_hashes.len()),
                        bundle: hash.clone(),
                        error: None,
                    });
                    done = true;
                    break;
                }
                Err(e) => last_error = e,
            }
        }
        if !done {
            mesh_rows.push(MeshRow {
                name: item.mesh.clone(),
                verts: None,
                tris: None,
                bones: None,
                bundle: candidates.first().cloned().unwrap_or_default(),
                error: Some(last_error.chars().take(200).collect()),
            });
        }
    }

    // --- image work ---------------------------------------------------------
    // Thumbnails and diffuse maps are decoded natively, downscaled in-process,
    // and converted to PNG in one texconv call for the whole skin. Decoding
    // each texture through its own texconv process was the pack phase's cost:
    // start-up plus a block decode at full resolution, once per image.
    let thumb_size = spec.thumb_size.unwrap_or(160) as usize;
    let map_size = spec.map_size.unwrap_or(1024) as usize;
    let mut images: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    // (name, bundle, source width, source height, decoded image)
    let mut thumbs: Vec<(String, String, u32, u32, Pending)> = Vec::new();
    for group in &spec.textures {
        match cache.get(&group.bundle) {
            Ok(bundle) => {
                for name in &group.names {
                    let (source, pending) = match bundle.texture(name) {
                        Ok(texture) => (
                            (texture.width as u32, texture.height as u32),
                            decode_pending(&texture, &bundle.nodes, texconv, thumb_size, &mut images),
                        ),
                        Err(e) => ((0, 0), Pending::Failed(e.to_string())),
                    };
                    thumbs.push((name.clone(), group.bundle.clone(), source.0, source.1, pending));
                }
            }
            Err(e) => {
                for name in &group.names {
                    thumbs.push((
                        name.clone(),
                        group.bundle.clone(),
                        0,
                        0,
                        Pending::Failed(e.clone()),
                    ));
                }
            }
        }
    }

    let mut map_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut map_items: Vec<Pending> = Vec::new();
    let mut map_choices: Vec<usize> = Vec::with_capacity(spec.maps.len());
    for map in &spec.maps {
        let key = (map.bundle.clone(), map.texture.clone());
        let slot = match map_slots.get(&key) {
            Some(slot) => *slot,
            None => {
                let pending = match cache.get(&map.bundle) {
                    Ok(bundle) => match bundle.texture(&map.texture) {
                        Ok(texture) => {
                            decode_pending(&texture, &bundle.nodes, texconv, map_size, &mut images)
                        }
                        Err(e) => Pending::Failed(e.to_string()),
                    },
                    Err(e) => Pending::Failed(e),
                };
                let slot = map_items.len();
                map_items.push(pending);
                map_slots.insert(key, slot);
                slot
            }
        };
        map_choices.push(slot);
    }

    let pngs: Vec<Result<Vec<u8>, String>> = texconv
        .rgba_to_png(&images)
        .into_iter()
        .map(|row| row.map_err(|e| e.to_string()))
        .collect();

    let mut thumb_rows = Vec::with_capacity(thumbs.len());
    for (name, bundle, source_width, source_height, pending) in thumbs {
        thumb_rows.push(match resolve_pending(&pending, &pngs) {
            Ok((_, _, png)) => ThumbRow {
                name,
                // The row carries the *source* size, not the thumbnail's: the
                // model page labels a texture with its original resolution.
                width: Some(source_width),
                height: Some(source_height),
                png: Some(base64::encode(png)),
                bundle,
                error: None,
            },
            Err(e) => ThumbRow {
                name,
                width: None,
                height: None,
                png: None,
                bundle,
                error: Some(e.chars().take(200).collect()),
            },
        });
    }
    thumb_rows.sort_by(|a, b| a.name.cmp(&b.name));

    let map_pngs: Vec<Option<String>> = map_items
        .iter()
        .map(|pending| {
            resolve_pending(pending, &pngs)
                .ok()
                .map(|(_, _, png)| base64::encode(png))
        })
        .collect();
    let mut diffuse = BTreeMap::new();
    for (index, map) in spec.maps.iter().enumerate() {
        if let Some(png) = &map_pngs[map_choices[index]] {
            diffuse.insert(map.mesh.clone(), png.clone());
        }
    }

    // --- files -------------------------------------------------------------
    write_json(&out_dir.join("meshes.json"), &mesh_rows)?;
    write_json(
        &out_dir.join("geometry.json"),
        &serde_json::json!({ "meshes": geometry_rows }),
    )?;
    write_json(&out_dir.join("thumbs.json"), &thumb_rows)?;
    write_json(&out_dir.join("groups.json"), &spec.groups)?;
    write_json(&out_dir.join("diffuse.json"), &diffuse)?;
    write_json(
        &out_dir.join("meta.json"),
        &serde_json::json!({
            "format": FORMAT,
            "version": 3,
            "key": spec.key,
            "built_at": local_timestamp(),
            "cache_built_at": spec.cache_built_at,
        }),
    )?;

    let summary = Summary {
        format: FORMAT,
        key: spec.key.clone(),
        dir: spec.dir.clone(),
        meshes: mesh_rows.len(),
        geometry: geometry_rows.len(),
        textures: thumb_rows.len(),
        maps: diffuse.len(),
        seconds: (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    };
    serde_json::to_string(&summary).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text =
        serde_json::to_string(value).map_err(|e| format!("serialize {}: {e}", path.display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

/// Local wall-clock time as `YYYY-MM-DDTHH:MM:SS`, matching `time.strftime`.
pub fn local_timestamp() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(out: *mut SystemTime);
        }
        let mut now = SystemTime::default();
        unsafe { GetLocalTime(&mut now) };
        return format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            now.year, now.month, now.day, now.hour, now.minute, now.second
        );
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{secs}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> Texconv {
        Texconv::new("texconv-not-needed-here.exe")
    }

    #[test]
    fn an_empty_spec_writes_every_cache_file() {
        let dir = std::env::temp_dir().join(format!("exmt-skinpack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let spec = dir.join("spec.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &spec,
            serde_json::json!({
                "key": "test",
                "dir": dir.join("out").to_string_lossy(),
                "bundle_dir": dir.to_string_lossy(),
                "cache_built_at": "2026-01-01T00:00:00",
                "meshes": [],
                "textures": [],
                "maps": [],
                "groups": {"m": ["t"]},
                "thumb_size": 160,
                "map_size": 1024
            })
            .to_string(),
        )
        .unwrap();

        let line = run(&spec, &tool()).unwrap();
        let summary: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(summary["format"], FORMAT);
        assert_eq!(summary["meshes"], 0);
        for name in [
            "meshes.json",
            "geometry.json",
            "thumbs.json",
            "groups.json",
            "diffuse.json",
            "meta.json",
        ] {
            assert!(dir.join("out").join(name).is_file(), "{name} missing");
        }
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("out/meta.json")).unwrap())
                .unwrap();
        assert_eq!(meta["format"], FORMAT);
        assert_eq!(meta["cache_built_at"], "2026-01-01T00:00:00");
        assert_eq!(meta["version"], 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_spec_is_an_error() {
        assert!(run(Path::new("nope.json"), &tool()).is_err());
    }
}

