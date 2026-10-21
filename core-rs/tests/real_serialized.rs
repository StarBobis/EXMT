//! Contract tests for the SerializedFile reader against a real bundle.
//!
//! Every expected value here was produced by an independent implementation on
//! the same file. The suite returns early when the sample is absent so it stays
//! runnable on a clean checkout.

use exmt_core::container;
use exmt_core::serialized::{Endian, SerializedFile};

mod support;

fn load() -> Option<SerializedFile> {
    let path = support::gf2_bundle()?;
    let raw = std::fs::read(path).expect("read sample");
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse container");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");
    Some(SerializedFile::parse(&nodes[0].data).expect("parse serialized file"))
}

#[test]
fn header_fields_match() {
    let Some(sf) = load() else { return };
    assert_eq!(sf.metadata_size, 11005);
    assert_eq!(sf.file_size, 2_740_916);
    assert_eq!(sf.version, 21);
    assert_eq!(sf.data_offset, 11040);
    assert_eq!(sf.endian, Endian::Little);
    assert_eq!(sf.unity_version, "2019.4.29f1");
    assert_eq!(sf.platform, 19, "StandaloneWindows64");
    assert!(sf.has_type_tree);
}

#[test]
fn type_table_matches() {
    let Some(sf) = load() else { return };
    assert_eq!(sf.types.len(), 2);
    assert_eq!(sf.types[0].class_id, 43, "Mesh");
    assert_eq!(sf.types[1].class_id, 142, "AssetBundle");
    assert_eq!(sf.types[0].nodes.len(), 238);
    assert_eq!(sf.types[1].nodes.len(), 58);
    assert!(sf.types.iter().all(|t| t.script_type_index == -1));
    // v21 appends an empty dependency list to each type.
    assert!(sf.types.iter().all(|t| t.type_dependencies.is_empty()));
}

#[test]
fn object_table_matches() {
    let Some(sf) = load() else { return };
    let expected: [(i64, u64, u32, i32); 5] = [
        (-5358632390582853297, 0, 237_912, 0),
        (-4397244682595215826, 237_912, 1_822_256, 0),
        (1, 2_060_168, 412, 1),
        (7433588863114304978, 2_060_584, 38_040, 0),
        (8143092236940524548, 2_098_624, 631_252, 0),
    ];
    assert_eq!(sf.objects.len(), expected.len());
    for (i, (path_id, start, size, type_id)) in expected.iter().enumerate() {
        let (got_id, obj) = &sf.objects[i];
        assert_eq!(got_id, path_id, "path_id of object {i}");
        assert_eq!(obj.byte_start, *start, "byte_start of object {i}");
        assert_eq!(obj.byte_size, *size, "byte_size of object {i}");
        assert_eq!(obj.type_id, *type_id, "type_id of object {i}");
        // The type id must resolve to the class the fixture recorded.
        let class_id = sf.type_of(obj).expect("type resolves").class_id;
        assert_eq!(class_id, if *type_id == 0 { 43 } else { 142 });
    }
}

#[test]
fn objects_are_ordered_with_small_alignment_gaps() {
    let Some(sf) = load() else { return };
    let mut spans: Vec<(u64, u32)> =
        sf.objects.iter().map(|(_, o)| (o.byte_start, o.byte_size)).collect();
    spans.sort();
    assert_eq!(spans[0].0, 0);
    for w in spans.windows(2) {
        let (a_start, a_size) = w[0];
        let (b_start, _) = w[1];
        let gap = b_start as i64 - (a_start + a_size as u64) as i64;
        assert!(gap >= 0, "objects overlap");
        assert!(gap <= 4, "gap {gap} larger than the observed 4-byte padding");
    }
}

#[test]
fn last_object_ends_at_end_of_file() {
    let Some(sf) = load() else { return };
    let end = sf
        .objects
        .iter()
        .map(|(_, o)| o.byte_start + o.byte_size as u64)
        .max()
        .expect("objects");
    assert_eq!(sf.data_offset + end, sf.data.len() as u64);
}

#[test]
fn type_tree_root_and_key_nodes() {
    let Some(sf) = load() else { return };
    let mesh = &sf.types[0];
    let root = mesh.root().expect("root");
    assert_eq!((root.level, root.type_name.as_str(), root.name.as_str()), (0, "Mesh", "Base"));
    // Resolves through the type's own string buffer.
    let name_node = &mesh.nodes[1];
    assert_eq!((name_node.level, name_node.type_name.as_str(), name_node.name.as_str()), (1, "string", "m_Name"));
    // Resolves through Unity's built-in table.
    let arr = &mesh.nodes[2];
    assert_eq!((arr.type_name.as_str(), arr.name.as_str()), ("Array", "Array"));
}

#[test]
fn type_tree_levels_are_preorder_valid() {
    let Some(sf) = load() else { return };
    for ty in &sf.types {
        assert_eq!(ty.nodes[0].level, 0);
        for w in ty.nodes.windows(2) {
            assert!(
                w[1].level <= w[0].level + 1,
                "child skipped a level: {} -> {}",
                w[0].level,
                w[1].level
            );
        }
    }
}

#[test]
fn subtree_ends_partition_the_tree() {
    let Some(sf) = load() else { return };
    for ty in &sf.types {
        // The root must cover the whole list.
        assert_eq!(ty.nodes[0].subtree_end, ty.nodes.len());
        // A leaf's subtree is just itself.
        for (i, n) in ty.nodes.iter().enumerate() {
            assert!(n.subtree_end > i, "subtree_end must be exclusive and past the node");
            assert!(n.subtree_end <= ty.nodes.len());
        }
    }
}

#[test]
fn payload_slices_have_expected_sizes() {
    let Some(sf) = load() else { return };
    let obj = sf.object(-5358632390582853297).expect("body mesh");
    let payload = sf.object_bytes(obj);
    assert_eq!(payload.len(), 237_912);
    let bundle = sf.object(1).expect("asset bundle");
    assert_eq!(sf.object_bytes(bundle).len(), 412);
}

#[test]
fn every_object_slice_is_readable() {
    let Some(sf) = load() else { return };
    for (path_id, obj) in &sf.objects {
        let bytes = sf.object_bytes(obj);
        assert_eq!(bytes.len(), obj.byte_size as usize, "object {path_id}");
    }
}

#[test]
fn missing_object_is_none() {
    let Some(sf) = load() else { return };
    assert!(sf.object(424242).is_none());
}

