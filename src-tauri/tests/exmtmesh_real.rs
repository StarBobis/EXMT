//! Contract test for the `.exmtmesh` reader.
//!
//! Set `EXMT_EXMTMESH` to a file written by `interchange.save` (the Blender
//! add-on's format). The test checks the invariants every exported mesh must
//! satisfy; the exact geometry is the game's, not a fixture.

use std::path::Path;

use exmt_modbox_lib::exmtmesh;

#[test]
fn reads_a_python_written_mesh() {
    let Ok(path) = std::env::var("EXMT_EXMTMESH") else {
        eprintln!("skip: set EXMT_EXMTMESH to an .exmtmesh file");
        return;
    };
    let mesh = exmtmesh::load(Path::new(&path)).expect("load .exmtmesh");
    assert!(!mesh.name.is_empty(), "mesh name");
    assert!(mesh.vertex_count > 0, "vertex count");
    assert!(!mesh.indices.is_empty(), "indices");
    assert_eq!(
        mesh.submesh_count.iter().map(|c| *c as usize).sum::<usize>(),
        mesh.indices.len(),
        "submeshes cover the index buffer"
    );
    assert_eq!(mesh.channels[0].dimension, 3, "positions are float3");
    let positions = mesh.arrays[0].as_deref().expect("positions");
    assert_eq!(positions.len(), mesh.vertex_count * 3);
    for (attr, values) in mesh.arrays.iter().enumerate() {
        if let Some(values) = values {
            let dim = mesh.channels[attr].dimension as usize;
            assert!(dim > 0, "array {attr} present on an unused channel");
            assert_eq!(values.len(), mesh.vertex_count * dim, "attribute {attr}");
        }
    }

    let patch = mesh.into_patch();
    assert_eq!(patch.attributes[0].as_deref().map(|v| v.len()), Some(patch.vertex_count * 3));
}
