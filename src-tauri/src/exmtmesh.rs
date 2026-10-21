//! Reader for the `.exmtmesh` interchange file (NPZ + meta JSON).
//!
//! The Blender add-on writes an `.exmtmesh` per edited mesh: NumPy arrays for
//! the geometry plus a `meta` entry holding the mesh name, channel layout and
//! (optionally) the asset's full typetree. This module turns that into a
//! [`MeshPatch`] the core can write into a target Mesh tree.
//!
//! Only the arrays the game needs are read. The embedded typetree is not used:
//! when the target bundle stores the same cook, its own tree is the better
//! source of truth, and when it does not, the core refuses the patch so the
//! Python core can do the bone remapping.

use std::path::Path;

use exmt_core::mesh::{Channel, MeshPatch};
use exmt_core::npy::Npy;
use exmt_core::zip::Archive;
use serde_json::Value;

pub const FORMAT: &str = "exmt-mesh-2";

/// Arrays the interchange format may carry, mapped to their channel slot.
const ARRAYS: [(&str, usize); 8] = [
    ("positions", 0),
    ("normals", 1),
    ("tangents", 2),
    ("colors", 3),
    ("uv0", 4),
    ("uv1", 5),
    ("weights", 12),
    ("bones", 13),
];

#[derive(Debug, Clone)]
pub struct ExmtMesh {
    pub name: String,
    pub vertex_count: usize,
    pub index_format: i32,
    pub channels: [Channel; 14],
    pub arrays: [Option<Vec<f32>>; 14],
    pub indices: Vec<u32>,
    /// Index count per submesh, in file order.
    pub submesh_count: Vec<u32>,
    pub bone_hashes: Vec<u32>,
}

impl ExmtMesh {
    pub fn into_patch(self) -> MeshPatch {
        MeshPatch {
            name: self.name,
            vertex_count: self.vertex_count,
            channels: self.channels,
            attributes: self.arrays,
            indices: self.indices,
            index_format: self.index_format,
            submesh_count: self.submesh_count,
            bone_hashes: self.bone_hashes,
        }
    }
}

pub fn load(path: &Path) -> Result<ExmtMesh, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    load_bytes(&data).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_bytes(data: &[u8]) -> Result<ExmtMesh, String> {
    let archive = Archive::new(data).map_err(|e| format!("open NPZ: {e}"))?;
    let meta_bytes = archive
        .read("meta.npy", 64 << 20)
        .map_err(|e| format!("meta.npy: {e}"))?;
    let meta_text = Npy::parse(&meta_bytes)
        .map_err(|e| format!("parse meta.npy: {e}"))?
        .as_string()
        .map_err(|e| format!("decode meta: {e}"))?;
    let info: Value =
        serde_json::from_str(&meta_text).map_err(|e| format!("parse meta JSON: {e}"))?;

    let format = info.get("format").and_then(|v| v.as_str()).unwrap_or("");
    if format != FORMAT {
        return Err(format!("unsupported format {format:?} (expected {FORMAT})"));
    }
    let vertex_count = info
        .get("vertex_count")
        .and_then(|v| v.as_u64())
        .ok_or("meta has no vertex_count")? as usize;
    let index_format = info
        .get("index_format")
        .and_then(|v| v.as_i64())
        .ok_or("meta has no index_format")? as i32;
    if index_format != 0 && index_format != 1 {
        return Err(format!("invalid index_format {index_format}"));
    }
    let channels = read_channels(&info)?;

    let mut arrays: [Option<Vec<f32>>; 14] = Default::default();
    for (key, attr) in ARRAYS {
        let dim = channels[attr].dimension as usize;
        if dim == 0 || archive.get(&format!("{key}.npy")).is_none() {
            continue;
        }
        let npy = read_npy(&archive, &format!("{key}.npy"))?;
        arrays[attr] = Some(read_rows(&npy, vertex_count, dim)?);
    }

    if archive.get("indices.npy").is_none() {
        return Err("missing indices.npy".into());
    }
    // `interchange.save` always stores indices as uint32, whatever the mesh's
    // index format is; narrow only after reading, and reject values that would
    // be silently truncated.
    let wide = read_npy(&archive, "indices.npy")?
        .as_u32()
        .map_err(|e| format!("indices: {e}"))?;
    let indices = if index_format == 0 {
        wide.into_iter()
            .map(|v| {
                u16::try_from(v)
                    .map(u32::from)
                    .map_err(|_| format!("index {v} does not fit the 16-bit index buffer"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        wide
    };

    let submesh_count = read_npy(&archive, "submesh_count.npy")?
        .as_u32()
        .map_err(|e| format!("submesh_count: {e}"))?;
    let bone_hashes = if archive.get("bone_hashes.npy").is_some() {
        read_npy(&archive, "bone_hashes.npy")?
            .as_u32()
            .map_err(|e| format!("bone_hashes: {e}"))?
    } else {
        Vec::new()
    };

    Ok(ExmtMesh {
        name: info
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        vertex_count,
        index_format,
        channels,
        arrays,
        indices,
        submesh_count,
        bone_hashes,
    })
}

fn read_npy(archive: &Archive<'_>, name: &str) -> Result<Npy, String> {
    let bytes = archive
        .read(name, 256 << 20)
        .map_err(|e| format!("{name}: {e}"))?;
    Npy::parse(&bytes).map_err(|e| format!("{name}: {e}"))
}

/// Read a 2-D array, keeping the first `dim` columns of each row.
fn read_rows(npy: &Npy, rows: usize, dim: usize) -> Result<Vec<f32>, String> {
    if npy.shape.len() != 2 || npy.shape[0] != rows {
        return Err(format!("expected a ({rows}, N) array, got {:?}", npy.shape));
    }
    let cols = npy.shape[1];
    if cols < dim {
        return Err(format!("array has {cols} columns, the channel needs {dim}"));
    }
    let flat: Vec<f32> = if npy.descr.contains("f4") {
        npy.as_f32().map_err(|e| e.to_string())?
    } else if npy.descr.contains("u4") {
        npy.as_u32()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|v| v as f32)
            .collect()
    } else {
        return Err(format!("unsupported dtype {}", npy.descr));
    };
    let mut out = Vec::with_capacity(rows * dim);
    for row in 0..rows {
        out.extend_from_slice(&flat[row * cols..row * cols + dim]);
    }
    Ok(out)
}

fn read_channels(info: &Value) -> Result<[Channel; 14], String> {
    let list = info
        .get("channels")
        .and_then(|v| v.as_array())
        .ok_or("meta has no channels")?;
    let mut channels = [Channel::UNUSED; 14];
    for (i, ch) in list.iter().enumerate().take(14) {
        let field = |name: &str| ch.get(name).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        channels[i] = Channel {
            stream: field("stream"),
            offset: field("offset"),
            format: field("format"),
            dimension: field("dimension"),
        };
    }
    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_npz_file_is_rejected() {
        assert!(load_bytes(b"not an npz").is_err());
    }

    #[test]
    fn channels_are_padded_to_fourteen() {
        let info = serde_json::json!({"channels": [{"stream": 0, "offset": 0, "format": 0, "dimension": 3}]});
        let channels = read_channels(&info).unwrap();
        assert_eq!(channels[0].dimension, 3);
        assert_eq!(channels[13], Channel::UNUSED);
    }
}
