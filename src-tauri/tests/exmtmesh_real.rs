//! Contract test for the `.exmtmesh` reader.
//!
//! Set `EXMT_EXMTMESH` to a file written by `interchange.save` (the Blender
//! add-on's format). The fixture used during development was produced by
//! Python/NumPy with a 3-vertex triangle, float32 positions/normals/UVs and
//! uint32 indices.

use std::path::Path;

use exmt_modbox_lib::exmtmesh;

#[test]
fn reads_the_python_written_file() {
    let Ok(path) = std::env::var("EXMT_EXMTMESH") else {
        eprintln!("skip: set EXMT_EXMTMESH to an .exmtmesh file");
        return;
    };
    let mesh = exmtmesh::load(Path::new(&path)).expect("load .exmtmesh");
    assert_eq!(mesh.name, "tri");
    assert_eq!(mesh.vertex_count, 3);
    assert_eq!(mesh.index_format, 1);
    assert_eq!(mesh.channels[0].dimension, 3);
    assert_eq!(mesh.channels[4].offset, 24);
    assert_eq!(mesh.channels[13].format, 10);
    assert_eq!(
        mesh.arrays[0].as_deref(),
        Some(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0][..])
    );
    assert_eq!(mesh.indices, vec![0, 1, 2]);
    assert_eq!(mesh.submesh_count, vec![3]);
    assert!(mesh.bone_hashes.is_empty());

    let patch = mesh.into_patch();
    assert_eq!(patch.name, "tri");
    assert_eq!(
        patch.attributes[4].as_deref(),
        Some(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0][..])
    );
}
