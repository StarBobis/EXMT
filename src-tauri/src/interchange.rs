//! Write the Blender interchange file (`.exmtmesh`, NPZ) from a decoded mesh.
//!
//! The shape follows `core/exmt_core/interchange.py`:
//!
//! * typed arrays for positions/normals/tangents/colors/uv0/uv1/weights/bones,
//!   indices (always uint32), submesh first/count, bindpose and bone hashes;
//! * the mesh's full typetree as JSON, with the big byte fields moved to
//!   `raw_stream` / `raw_index` / `raw_baked_*` and nulled in the JSON;
//! * a `meta` entry with name, channels, AABB, submeshes, skeleton view and the
//!   typetree.
//!
//! This is the write side only; `exmtmesh.rs` reads it back.

use std::collections::BTreeMap;

use exmt_core::mesh::Mesh;
use exmt_core::npz::Npz;
use exmt_core::skeleton::View;
use exmt_core::typetree::Value;

const RAW_FIELDS: [(&str, &str); 4] = [
    ("m_DataSize", "raw_stream"),
    ("m_IndexBuffer", "raw_index"),
    ("m_BakedConvexCollisionMesh", "raw_baked_convex"),
    ("m_BakedTriangleCollisionMesh", "raw_baked_tri"),
];

/// Build one `.exmtmesh` from a mesh and its decoded tree.
pub fn build(mesh: &Mesh, tree: &Value, bundle: &str, skeleton: &View) -> Vec<u8> {
    let mut raw = BTreeMap::new();
    let tree_json = value_to_json(tree, None, &mut raw);
    let aabb = tree
        .get("m_LocalAABB")
        .map(|v| value_to_json(v, None, &mut BTreeMap::new()))
        .unwrap_or(serde_json::Value::Null);
    let submeshes = tree
        .get("m_SubMeshes")
        .map(|v| value_to_json(v, None, &mut BTreeMap::new()))
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    let mut info = serde_json::json!({
        "format": "exmt-mesh-2",
        "name": mesh.name,
        "vertex_count": mesh.vertex_count,
        "index_format": mesh.index_format,
        "channels": mesh.channels.iter().map(|c| serde_json::json!({
            "stream": c.stream, "offset": c.offset, "format": c.format, "dimension": c.dimension,
        })).collect::<Vec<_>>(),
        "aabb": aabb,
        "submeshes": submeshes,
        "has_bindpose": !mesh.bindpose.is_empty(),
        "bundle": bundle,
        "skeleton": skeleton_json(skeleton),
    });
    if let serde_json::Value::Object(map) = &mut info {
        map.insert("tree".into(), tree_json);
    }

    let mut npz = Npz::new();
    for (attr, key) in [
        (0usize, "positions"),
        (1, "normals"),
        (2, "tangents"),
        (3, "colors"),
        (4, "uv0"),
        (5, "uv1"),
        (12, "weights"),
        (13, "bones"),
    ] {
        let dim = mesh.channels[attr].dimension as usize;
        let values = mesh.streams.attributes[attr].as_deref();
        if dim > 0 {
            if let Some(values) = values {
                if attr == 13 {
                    // `interchange.save` writes bone indices as uint32 even
                    // though the in-memory vertex encoder works in floats.
                    let ints: Vec<u32> = values.iter().map(|v| *v as u32).collect();
                    npz.add_u32(key, &[mesh.vertex_count, dim], &ints);
                } else {
                    npz.add_f32(key, &[mesh.vertex_count, dim], values);
                }
            }
        }
    }
    // Python's writer always emits weights/bones, zero-filled when the channel
    // is unused; Blender's reader keys off the channel table, so this is inert
    // but keeps the file shape identical.
    for (attr, key) in [(12usize, "weights"), (13, "bones")] {
        if mesh.channels[attr].dimension == 0 {
            if attr == 13 {
                let zeros = vec![0u32; mesh.vertex_count * 4];
                npz.add_u32(key, &[mesh.vertex_count, 4], &zeros);
            } else {
                let zeros = vec![0.0f32; mesh.vertex_count * 4];
                npz.add_f32(key, &[mesh.vertex_count, 4], &zeros);
            }
        }
    }
    npz.add_u32("indices", &[mesh.indices.len()], &mesh.indices);
    let width: u32 = if mesh.index_format == 0 { 2 } else { 4 };
    let first: Vec<u32> = tree
        .get("m_SubMeshes")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (row.get("firstByte").and_then(|v| v.as_u32()).unwrap_or(0) / width) as u32
                })
                .collect()
        })
        .unwrap_or_default();
    let counts: Vec<u32> = tree
        .get("m_SubMeshes")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| row.get("indexCount").and_then(|v| v.as_u32()).unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    npz.add_u32("submesh_first", &[first.len()], &first);
    npz.add_u32("submesh_count", &[counts.len()], &counts);
    if !mesh.bindpose.is_empty() {
        let bones = mesh.bindpose.len() / 16;
        npz.add_f32("bindpose", &[bones, 4, 4], &mesh.bindpose);
    }
    npz.add_u32("bone_hashes", &[mesh.bone_hashes.len()], &mesh.bone_hashes);
    for (key, bytes) in raw {
        if !bytes.is_empty() {
            npz.add_u8(&key, &bytes);
        }
    }
    let meta = serde_json::to_string(&info).unwrap_or_else(|_| "{}".into());
    npz.add_string("meta", &meta);
    npz.write()
}

fn skeleton_json(view: &View) -> serde_json::Value {
    serde_json::json!({
        "id": view.id,
        "count": view.count,
        "named": view.named,
        "bones": view.bones.iter().map(|b| serde_json::json!({
            "hash": b.hash, "name": b.name, "parent": b.parent, "rest": b.rest,
        })).collect::<Vec<_>>(),
        "local": view.local,
    })
}

/// Convert a typetree value into the JSON shape UnityPy uses, moving the big
/// byte blobs into `raw` and replacing them with null.
fn value_to_json(
    value: &Value,
    field: Option<&str>,
    raw: &mut BTreeMap<String, Vec<u8>>,
) -> serde_json::Value {
    if let Some(name) = field {
        if let Some((_, key)) = RAW_FIELDS.iter().find(|(field, _)| *field == name) {
            if let Some(bytes) = byte_blob(value) {
                if !bytes.is_empty() {
                    raw.insert((*key).to_string(), bytes);
                    return serde_json::Value::Null;
                }
            }
        }
    }
    match value {
        Value::Bool(v) => serde_json::Value::Bool(*v),
        Value::I8(v) => (*v).into(),
        Value::U8(v) => (*v).into(),
        Value::I16(v) => (*v).into(),
        Value::U16(v) => (*v).into(),
        Value::I32(v) => (*v).into(),
        Value::U32(v) => (*v).into(),
        Value::I64(v) => (*v).into(),
        Value::U64(v) => (*v).into(),
        Value::F32(v) => float(*v as f64),
        Value::F64(v) => float(*v),
        Value::Str(v) => serde_json::Value::String(v.clone()),
        Value::Bytes(_) => serde_json::Value::Null,
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|v| value_to_json(v, None, raw)).collect())
        }
        Value::Struct(fields) => {
            let mut map = serde_json::Map::new();
            for (name, value) in fields {
                map.insert(name.clone(), value_to_json(value, Some(name), raw));
            }
            serde_json::Value::Object(map)
        }
        Value::Pair(a, b) => serde_json::Value::Array(vec![
            value_to_json(a, None, raw),
            value_to_json(b, None, raw),
        ]),
    }
}

fn byte_blob(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Bytes(bytes) => Some(bytes.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::U8(byte) => out.push(*byte),
                    Value::I8(byte) => out.push(*byte as u8),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn float(value: f64) -> serde_json::Value {
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_fields_are_nulled_and_collected() {
        let tree = Value::Struct(vec![
            (
                "m_VertexData".into(),
                Value::Struct(vec![("m_DataSize".into(), Value::Bytes(vec![1, 2, 3]))]),
            ),
            (
                "m_IndexBuffer".into(),
                Value::Array(vec![Value::U8(4), Value::U8(5)]),
            ),
        ]);
        let mut raw = BTreeMap::new();
        let json = value_to_json(&tree, None, &mut raw);
        assert_eq!(json["m_VertexData"]["m_DataSize"], serde_json::Value::Null);
        assert_eq!(json["m_IndexBuffer"], serde_json::Value::Null);
        assert_eq!(raw["raw_stream"], vec![1, 2, 3]);
        assert_eq!(raw["raw_index"], vec![4, 5]);
    }

    #[test]
    fn non_finite_floats_become_null() {
        assert_eq!(float(f64::INFINITY), serde_json::Value::Null);
        assert_eq!(float(1.5), serde_json::json!(1.5));
    }
}
