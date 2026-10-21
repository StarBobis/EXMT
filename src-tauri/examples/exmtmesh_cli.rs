//! Export one mesh to `.exmtmesh` with the Rust writer.
//!
//! `cargo run --example exmtmesh_cli -- <bundle> <mesh> <out.exmtmesh>`

use exmt_core::skeleton::View;
use exmt_core::{asset, typetree};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [bundle_path, mesh_name, out] = &args[..] else {
        eprintln!("usage: exmtmesh_cli <bundle> <mesh> <out.exmtmesh>");
        std::process::exit(2);
    };
    let bundle = asset::open(bundle_path).expect("open bundle");
    let (_, mesh) = bundle.mesh_object(mesh_name).expect("mesh");
    let (_, tree) = {
        let mut found = None;
        for (_, obj) in &bundle.serialized.objects {
            let Some(ty) = bundle.serialized.type_of(obj) else {
                continue;
            };
            if ty.class_id != asset::CLASS_MESH {
                continue;
            }
            let tree = typetree::read_value(&bundle.serialized, obj).expect("tree");
            if tree.get("m_Name").and_then(|v| v.as_str()) == Some(mesh_name.as_str()) {
                found = Some(tree);
                break;
            }
        }
        (0, found.expect("tree"))
    };
    let view = View {
        id: "test".into(),
        count: 0,
        named: 0,
        bones: Vec::new(),
        local: Vec::new(),
    };
    let bytes = exmt_modbox_lib::interchange::build(&mesh, &tree, "test", &view);
    std::fs::write(out, bytes).expect("write");
    println!("wrote {out}");
}
