//! Differential helper for `pairings::pair`.
//!
//! `cargo run --example pair_cli -- <names.json> <skins.json> <out.json>`

use std::collections::BTreeMap;

use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: pair_cli <names.json> <skins.json> <out.json> [texture-order.json]");
        std::process::exit(2);
    }
    let (names_path, skins_path, out) = (&args[0], &args[1], &args[2]);
    let order: Vec<String> = args
        .get(3)
        .map(|path| serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap())
        .unwrap_or_default();
    let names: Value = serde_json::from_str(&std::fs::read_to_string(names_path).unwrap()).unwrap();
    let textures: BTreeMap<String, Vec<String>> =
        serde_json::from_value(names["textures"].clone()).unwrap();
    let skins: Value = serde_json::from_str(&std::fs::read_to_string(skins_path).unwrap()).unwrap();
    let mut rows = Vec::new();
    for skin in skins["skins"].as_array().unwrap() {
        let parts: BTreeMap<String, String> =
            serde_json::from_value(skin["parts"].clone()).unwrap_or_default();
        let mesh_names: Vec<String> = parts.values().cloned().collect();
        let groups = exmt_modbox_lib::pairings::pair(&mesh_names, &textures, None, &order);
        let maps: Vec<Value> = mesh_names
            .iter()
            .filter_map(|mesh| {
                let names = groups.get(mesh)?;
                let texture = exmt_modbox_lib::pairings::diffuse_of(names)?;
                let bundle = textures.get(&texture)?.first()?.clone();
                Some(json!({"mesh": mesh, "texture": texture, "bundle": bundle}))
            })
            .collect();
        rows.push(json!({
            "key": skin["id"],
            "groups": groups,
            "maps": maps,
        }));
    }
    std::fs::write(out, serde_json::to_string(&rows).unwrap()).unwrap();
    println!("{}", rows.len());
}
