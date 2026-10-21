//! Print animation binding counts for one clip (differential helper).

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [bundle_path, clip_name] = &args[..] else {
        eprintln!("usage: anim_cli <bundle> <clip>");
        std::process::exit(2);
    };
    let bundle = exmt_core::asset::open(Path::new(bundle_path)).expect("bundle");
    for (_, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else { continue };
        if ty.class_id != 74 {
            continue;
        }
        let tree = exmt_core::typetree::read_value(&bundle.serialized, obj).expect("tree");
        if tree.get("m_Name").and_then(|v| v.as_str()) != Some(clip_name.as_str()) {
            continue;
        }
        let clip = exmt_core::anim::from_value(&tree).expect("clip");
        let transform: Vec<_> = clip.bindings.iter().filter(|b| b.type_id == 4).collect();
        println!("bindings={} transform={} paths={}", clip.bindings.len(), transform.len(),
            transform.iter().map(|b| b.path).collect::<std::collections::HashSet<_>>().len());
        for b in transform.iter().take(5) {
            println!("  path={} attr={} base={} count={}", b.path, b.attribute, b.base, b.count);
        }
        return;
    }
    eprintln!("clip not found");
    std::process::exit(1);
}
