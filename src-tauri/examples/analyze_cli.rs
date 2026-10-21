//! Differential driver for the first analyze stages.
//!
//! `cargo run --example analyze_cli -- <bundle_dir> <cache_dir> <limit> <workers>`
//! writes `GameCache/{names,clips,skins}.json` (no packs/bones yet).

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: analyze_cli <bundle_dir> <cache_dir> <limit> <workers> [bones]");
        std::process::exit(2);
    }
    let (bundle_dir, cache_dir, limit, workers) = (&args[0], &args[1], &args[2], &args[3]);
    let out = Path::new(cache_dir).join("GameCache");
    let limit: Option<usize> = match limit.parse::<usize>() {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    };
    let workers: usize = workers.parse().unwrap_or(6);
    let built_at = exmt_modbox_lib::skinpack::local_timestamp();
    let with_bones = args.get(4).map(|v| v == "bones").unwrap_or(false);
    let outcome = exmt_modbox_lib::analyze::build_indexes(
        Path::new(bundle_dir),
        &out,
        limit,
        workers,
        with_bones,
        &built_at,
        None,
    )
    .expect("indexes");
    let index = outcome.index;
    let skins = exmt_modbox_lib::analyze::skins_of(&index);
    let bones = outcome.bones.len();
    let seconds = outcome.seconds;
    let skin_json = serde_json::json!({
        "format": "exmt-skins-1",
        "built_at": built_at,
        "skins": skins,
    });
    exmt_modbox_lib::analyze::write_json(&out.join("skins.json"), &skin_json).expect("skins");
    println!(
        "{}",
        serde_json::json!({
            "bundles": index.bundles,
            "meshes": index.meshes.len(),
            "textures": index.textures.len(),
            "clips": index.clips.len(),
            "skins": skins.len(),
            "bones": bones,
            "seconds": (seconds * 10.0).round() / 10.0,
        })
    );
}
