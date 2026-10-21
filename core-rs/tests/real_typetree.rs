//! Contract tests for the value layer: decode, re-encode, expect the same bytes.
//!
//! The central invariant is that reading a payload and writing it back without
//! edits reproduces it exactly. A second set of tests proves the writer really
//! re-serializes (payload size changes with string length, mutating a float
//! survives) rather than copying bytes around.

use std::path::Path;

use exmt_core::container;
use exmt_core::serialized::SerializedFile;
use exmt_core::typetree::{self, read_value, write_value, Value};

const SAMPLE: &str = r"C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig";

fn load() -> Option<SerializedFile> {
    let path = Path::new(SAMPLE);
    if !path.exists() {
        eprintln!("skip: sample bundle not present");
        return None;
    }
    let raw = std::fs::read(path).expect("read sample");
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse container");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");
    Some(SerializedFile::parse(&nodes[0].data).expect("parse serialized file"))
}

#[test]
fn every_object_is_byte_identical_after_round_trip() {
    let Some(sf) = load() else { return };
    for (path_id, obj) in &sf.objects {
        let value = read_value(&sf, obj).unwrap_or_else(|e| panic!("read {path_id}: {e}"));
        let out = write_value(&sf, obj, &value).unwrap_or_else(|e| panic!("write {path_id}: {e}"));
        assert_eq!(out, sf.object_bytes(obj), "payload changed for object {path_id}");
    }
}

#[test]
fn large_mesh_round_trips() {
    let Some(sf) = load() else { return };
    let obj = sf.object(-4397244682595215826).expect("cloth mesh");
    assert_eq!(obj.byte_size, 1_822_256);
    let value = read_value(&sf, obj).expect("read");
    assert_eq!(write_value(&sf, obj, &value).expect("write"), sf.object_bytes(obj));
}

#[test]
fn decoded_mesh_has_expected_shape() {
    let Some(sf) = load() else { return };
    let obj = sf.object(-5358632390582853297).expect("body mesh");
    let value = read_value(&sf, obj).expect("read");

    let fields = value.as_struct().expect("mesh is a struct");
    assert_eq!(fields.len(), 23, "mesh has 23 top-level fields");
    assert_eq!(value.get("m_Name").and_then(|v| v.as_str()), Some("c_VepleySSR01_slg_body_lod0"));

    let vd = value.get("m_VertexData").expect("m_VertexData");
    assert_eq!(vd.get("m_VertexCount").and_then(|v| v.as_i32()), Some(2283));
    let channels = vd.get("m_Channels").and_then(|v| v.as_array()).expect("channels");
    assert_eq!(channels.len(), 14);
    let data = vd.get("m_DataSize").and_then(|v| v.as_bytes()).expect("data size");
    assert_eq!(data.len(), 210_048);
    // 2283 verts * 92 bytes/vertex, padded up to a 16-byte boundary.
    assert!(data.len() >= 2283 * 92);
    assert!(data.len() - 2283 * 92 < 16);

    let subs = value.get("m_SubMeshes").and_then(|v| v.as_array()).expect("submeshes");
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].get("indexCount").and_then(|v| v.as_i32()), Some(11007));
}

#[test]
fn asset_bundle_object_shape() {
    let Some(sf) = load() else { return };
    let obj = sf.object(1).expect("asset bundle");
    let value = read_value(&sf, obj).expect("read");
    let fields = value.as_struct().expect("struct");
    assert_eq!(fields.len(), 11);
    assert!(value.get("m_AssetBundleName").is_some());
    assert!(value.get("m_Container").is_some());
}

#[test]
fn rename_resizes_payload_and_keeps_neighbours() {
    let Some(sf) = load() else { return };
    let obj = *sf.object(-5358632390582853297).expect("body mesh");
    let original = sf.object_bytes(&obj).to_vec();
    let base = read_value(&sf, &obj).expect("read");

    for newname in ["x", "", "a_much_longer_name_than_the_original_one_abcdefghij"] {
        let mut tree = base.clone();
        match tree.get_mut("m_Name") {
            Some(Value::Str(s)) => *s = newname.to_string(),
            other => panic!("m_Name is not a string: {other:?}"),
        }
        let out = write_value(&sf, &obj, &tree).expect("write");
        // Re-read through the same type tree to check the mutation landed.
        let ty = sf.type_of(&obj).expect("type");
        let back = typetree::decode_with_type(&sf, ty, &out).expect("decode back");
        assert_eq!(
            back.get("m_Name").and_then(|v| v.as_str()),
            Some(newname),
            "rename did not survive"
        );
        assert_eq!(
            back.get("m_VertexData")
                .and_then(|v| v.get("m_VertexCount"))
                .and_then(|v| v.as_i32()),
            Some(2283),
            "vertex count changed across re-layout"
        );
        // The payload must actually grow or shrink with the name.
        assert_ne!(out.len(), original.len(), "payload length did not move");
    }
}

#[test]
fn vertex_mutation_survives_with_same_length() {
    let Some(sf) = load() else { return };
    let obj = *sf.object(-5358632390582853297).expect("body mesh");
    let mut tree = read_value(&sf, &obj).expect("read");

    // Read the first vertex X, bump it by 1.0, and write it back.
    let before = {
        let vd = tree.get("m_VertexData").expect("vd");
        let raw = vd.get("m_DataSize").and_then(|v| v.as_bytes()).expect("raw");
        f32::from_bits(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    };

    {
        let vd = tree.get_mut("m_VertexData").expect("vd");
        match vd.get_mut("m_DataSize") {
            Some(Value::Bytes(raw)) => {
                let mut bytes = raw.clone();
                bytes[0..4].copy_from_slice(&(before + 1.0).to_le_bytes());
                *raw = bytes;
            }
            other => panic!("m_DataSize is not bytes: {other:?}"),
        }
    }

    let out = write_value(&sf, &obj, &tree).expect("write");
    assert_eq!(out.len(), sf.object_bytes(&obj).len(), "size must not change");

    let ty = sf.type_of(&obj).expect("type");
    let back = typetree::decode_with_type(&sf, ty, &out).expect("decode back");
    let after = {
        let vd = back.get("m_VertexData").expect("vd");
        let raw = vd.get("m_DataSize").and_then(|v| v.as_bytes()).expect("raw");
        f32::from_bits(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    };
    assert!((after - (before + 1.0)).abs() < 1e-6, "vertex mutation lost: {before} -> {after}");
}

#[test]
fn missing_field_is_rejected_by_writer() {
    let Some(sf) = load() else { return };
    let obj = *sf.object(1).expect("asset bundle");
    let tree = read_value(&sf, &obj).expect("read");
    // Drop one field; the writer must refuse rather than emit a short payload.
    let mut broken = tree.clone();
    if let Value::Struct(fields) = &mut broken {
        fields.pop();
    }
    assert!(write_value(&sf, &obj, &broken).is_err());
}
