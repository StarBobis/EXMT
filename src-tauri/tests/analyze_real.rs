//! End-to-end harness for the Rust GameCache analysis.
//!
//! Set `EXMT_ANALYZE_BUNDLE_DIR` to the game's bundle folder; the test scans a
//! five-bundle slice (no packs) into a temporary cache.

use std::path::Path;

#[test]
fn scans_a_real_bundle_slice() {
    let Ok(bundle_dir) = std::env::var("EXMT_ANALYZE_BUNDLE_DIR") else {
        eprintln!("skip: set EXMT_ANALYZE_BUNDLE_DIR to the game's bundle folder");
        return;
    };
    let Some(tool) = exmt_core::texconv::Texconv::locate() else {
        eprintln!("skip: texconv.exe not available");
        return;
    };
    let cache = std::env::temp_dir().join(format!("exmt-analyze-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let options = exmt_modbox_lib::analyze::Options {
        workers: 2,
        limit: Some(5),
        with_packs: false,
        with_bones: true,
    };
    let line = exmt_modbox_lib::analyze::run(Path::new(&bundle_dir), &cache, &options, &tool)
        .expect("analyze");
    let manifest: serde_json::Value = serde_json::from_str(&line).expect("manifest");
    assert_eq!(manifest["format"], "exmt-gamecache-1");
    assert!(manifest["counts"]["textures"].as_u64().unwrap() > 0);
    assert!(manifest["counts"]["bones"].as_u64().unwrap() > 0);
    let names: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache.join("GameCache/names.json")).unwrap())
            .unwrap();
    assert_eq!(names["format"], "exmt-meshindex-2");
    assert!(cache.join("GameCache/skins.json").is_file());
    assert!(cache.join("GameCache/bones.json").is_file());
    assert!(cache.join("GameCache/manifest.json").is_file());
    let _ = std::fs::remove_dir_all(&cache);
}
