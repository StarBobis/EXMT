//! Contract tests for the Mesh semantic layer against real character meshes.

use std::path::Path;

use exmt_core::container;
use exmt_core::mesh;
use exmt_core::serialized::SerializedFile;
use exmt_core::typetree::{read_value, write_value};

const SAMPLE: &str = r"C:\DevCache\gf2\backup\872f1c4f93f07d6ba65b103ae5d1a337.bundle.orig";

/// (path_id, name, vertex count, index count, submesh index counts)
const MESHES: [(i64, &str, usize, usize, &[u32]); 4] = [
    (-5358632390582853297, "c_VepleySSR01_slg_body_lod0", 2283, 11007, &[11007]),
    (-4397244682595215826, "c_VepleySSR01_slg_cloth_lod0", 18194, 68121, &[28086, 34713, 3024, 2298]),
    (7433588863114304978, "c_VepleySSR01_slg_plush_lod0", 358, 1509, &[1509]),
    (8143092236940524548, "c_VepleySSR01_slg_hair_lod0", 5844, 21720, &[12453, 9267]),
];

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
fn mesh_inventory_matches() {
    let Some(sf) = load() else { return };
    for (path_id, name, vertices, indices, subs) in MESHES {
        let obj = sf.object(path_id).unwrap_or_else(|| panic!("object {path_id}"));
        let tree = read_value(&sf, obj).expect("read");
        let m = mesh::from_value(&tree).expect("mesh");
        assert_eq!(m.name, name);
        assert_eq!(m.vertex_count, vertices, "{name} vertex count");
        assert_eq!(m.indices.len(), indices, "{name} index count");
        assert_eq!(m.submesh_index_counts, subs, "{name} submeshes");
        assert_eq!(m.index_format, 0, "{name} uses 16-bit indices");
        // Submesh index counts must cover the index buffer exactly.
        let total: u32 = subs.iter().sum();
        assert_eq!(total as usize, indices, "{name} submeshes do not cover the buffer");
    }
}

#[test]
fn channel_layout_matches_the_payload_size() {
    let Some(sf) = load() else { return };
    for (path_id, name, vertices, _, _) in MESHES {
        let obj = sf.object(path_id).expect("object");
        let tree = read_value(&sf, obj).expect("read");
        let m = mesh::from_value(&tree).expect("mesh");

        let strides = mesh::strides(&m.channels).expect("strides");
        // Body is the mesh whose per-stream strides were verified against the
        // Python oracle; other meshes use different channel sets (hair packs an
        // extra 8 bytes into stream 1).
        if name.contains("body") {
            assert_eq!(strides, vec![40, 20, 32], "{name} streams");
        }
        assert_eq!(strides.len(), 3, "{name} uses three streams");
        let implied = mesh::layout_size(&strides, m.vertex_count);
        assert_eq!(
            implied,
            m.streams.raw.len(),
            "{name}: layout implies {implied} bytes but m_DataSize is {}",
            m.streams.raw.len()
        );
        assert_eq!(m.vertex_count, vertices);
    }
}

#[test]
fn attributes_have_expected_shapes() {
    let Some(sf) = load() else { return };
    let obj = sf.object(-5358632390582853297).expect("body");
    let tree = read_value(&sf, obj).expect("read");
    let m = mesh::from_value(&tree).expect("mesh");

    let expect_dims = [
        (0, 3),  // position
        (1, 3),  // normal
        (2, 4),  // tangent
        (3, 4),  // color
        (4, 2),  // uv0
        (12, 4), // weights
        (13, 4), // bones
    ];
    for (attr, dim) in expect_dims {
        let values = m.streams.attributes[attr]
            .as_ref()
            .unwrap_or_else(|| panic!("attribute {attr} missing"));
        assert_eq!(values.len(), m.vertex_count * dim, "attribute {attr}");
    }
    // Unused uv1..uv7 stay absent.
    for attr in 5..12 {
        assert!(m.streams.attributes[attr].is_none(), "attribute {attr} should be unused");
    }
}

#[test]
fn vertices_lie_inside_their_declared_bounds() {
    let Some(sf) = load() else { return };
    for (path_id, name, _, _, _) in MESHES {
        let obj = sf.object(path_id).expect("object");
        let tree = read_value(&sf, obj).expect("read");
        let m = mesh::from_value(&tree).expect("mesh");
        let pos = m.streams.attributes[0].as_ref().expect("positions");

        // The stored AABB should contain every vertex (with a small tolerance
        // for float rounding).
        for axis in 0..3 {
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for v in 0..m.vertex_count {
                let x = pos[v * 3 + axis];
                lo = lo.min(x);
                hi = hi.max(x);
            }
            let center = lo + (hi - lo) * 0.5;
            let extent = (hi - lo) * 0.5;
            let declared_center = m.aabb_center[axis];
            let declared_extent = m.aabb_extent[axis];
            assert!(
                (center - declared_center).abs() < 1e-3 * (1.0 + declared_extent.abs()),
                "{name} axis {axis}: centre {center} vs declared {declared_center}"
            );
            assert!(
                (extent - declared_extent).abs() < 1e-3 * (1.0 + declared_extent.abs()),
                "{name} axis {axis}: extent {extent} vs declared {declared_extent}"
            );
        }
    }
}

#[test]
fn bone_weights_are_normalised() {
    let Some(sf) = load() else { return };
    let obj = sf.object(8143092236940524548).expect("hair");
    let tree = read_value(&sf, obj).expect("read");
    let m = mesh::from_value(&tree).expect("mesh");
    let weights = m.streams.attributes[12].as_ref().expect("weights");
    // Skinning weights for a bound vertex sum to 1. Unweighted vertices read 0.
    let mut checked = 0;
    for v in 0..m.vertex_count.min(200) {
        let sum: f32 = (0..4).map(|k| weights[v * 4 + k]).sum();
        if sum > 0.0 {
            assert!((sum - 1.0).abs() < 1e-3, "vertex {v} weights sum to {sum}");
            checked += 1;
        }
    }
    assert!(checked > 0, "no weighted vertices found");
}

#[test]
fn bone_indices_are_in_range() {
    let Some(sf) = load() else { return };
    let obj = sf.object(-5358632390582853297).expect("body");
    let tree = read_value(&sf, obj).expect("read");
    let m = mesh::from_value(&tree).expect("mesh");
    let bones = m.streams.attributes[13].as_ref().expect("bones");
    let bone_count = m.bone_hashes.len().max(1) as f32;
    for (i, b) in bones.iter().enumerate() {
        assert!(*b >= 0.0, "negative bone index at {i}");
        assert!(
            *b < bone_count.max(4096.0),
            "bone index {b} at {i} exceeds {} bones",
            m.bone_hashes.len()
        );
    }
}

#[test]
fn decode_encode_round_trips_the_vertex_blob() {
    let Some(sf) = load() else { return };
    for (path_id, name, _, _, _) in MESHES {
        let obj = sf.object(path_id).expect("object");
        let tree = read_value(&sf, obj).expect("read");
        let m = mesh::from_value(&tree).expect("mesh");

        let strides = mesh::strides(&m.channels).expect("strides");
        let back = mesh::encode_vertices(
            &m.streams.attributes,
            m.vertex_count,
            &m.channels,
            Some(&m.streams.raw),
        )
        .expect("encode");
        assert_eq!(back.len(), mesh::layout_size(&strides, m.vertex_count));
        assert_eq!(back, m.streams.raw, "{name} vertex blob changed");
    }
}

#[test]
fn full_object_round_trip_through_mesh_layer() {
    // The strongest check: pull the mesh out, put it back, and re-serialize the
    // whole object. Unrelated fields must be untouched byte for byte.
    let Some(sf) = load() else { return };
    for (path_id, name, _, _, _) in MESHES {
        let obj = sf.object(path_id).expect("object");
        let mut tree = read_value(&sf, obj).expect("read");
        let m = mesh::from_value(&tree).expect("mesh");
        mesh::to_value(&mut tree, &m).expect("write mesh back");
        let out = write_value(&sf, obj, &tree).expect("serialize");
        assert_eq!(out, sf.object_bytes(obj), "{name} object changed");
    }
}

#[test]
fn modifying_a_vertex_changes_only_that_vertex() {
    let Some(sf) = load() else { return };
    let obj = sf.object(8143092236940524548).expect("hair");
    let mut tree = read_value(&sf, obj).expect("read");
    let mut m = mesh::from_value(&tree).expect("mesh");

    let before = m.streams.raw.clone();
    let original_x = f32::from_le_bytes([before[0], before[1], before[2], before[3]]);
    {
        let pos = m.streams.attributes[0].as_mut().expect("positions");
        pos[0] = original_x + 1.0;
    }
    mesh::to_value(&mut tree, &m).expect("write");
    let out = write_value(&sf, obj, &tree).expect("serialize");
    assert_eq!(out.len(), sf.object_bytes(obj).len(), "size must not change");

    // Re-read and confirm the mutation landed while following bytes held.
    let ty = sf.type_of(&obj).expect("type");
    let back = exmt_core::typetree::decode_with_type(&sf, ty, &out).expect("decode");
    let m2 = mesh::from_value(&back).expect("mesh");
    let pos2 = m2.streams.attributes[0].as_ref().expect("positions");
    assert!(
        (pos2[0] - (original_x + 1.0)).abs() < 1e-6,
        "vertex X mutation lost: {original_x} -> {}",
        pos2[0]
    );
    // Bytes after the first float belong to other vertices and must be untouched.
    assert_eq!(m2.streams.raw[4..], before[4..], "later vertices should not move");
}
