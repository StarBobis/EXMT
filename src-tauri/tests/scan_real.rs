//! Differential check for the partial bundle reader the scan uses.
//!
//! `analyze::scan_bundle_with` reads only node 0 of a bundle (the
//! SerializedFile) instead of decompressing the sibling `.resS` stream too.
//! That is what makes a full scan affordable, so the names it finds must match
//! a full `asset::open` of the same file exactly.
//!
//! Opt-in: set `EXMT_ANALYZE_BUNDLE_DIR` to the game's bundle folder. Without
//! it the test prints a skip line and passes, so a checkout with no game data
//! stays green.

use std::path::Path;

use exmt_core::asset;
use exmt_modbox_lib::analyze;

fn names_of_full_open(path: &Path) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    let bundle = asset::open(path).ok()?;
    let mut meshes = Vec::new();
    let mut textures = Vec::new();
    let mut clips = Vec::new();
    for (_, obj) in &bundle.serialized.objects {
        let Some(ty) = bundle.serialized.type_of(obj) else {
            continue;
        };
        let bucket = match ty.class_id {
            43 => &mut meshes,
            28 => &mut textures,
            74 => &mut clips,
            _ => continue,
        };
        if let Some(name) = bundle.serialized.peek_name(obj) {
            bucket.push(name);
        }
    }
    Some((meshes, textures, clips))
}

#[test]
fn partial_scan_names_match_a_full_open() {
    let Ok(bundle_dir) = std::env::var("EXMT_ANALYZE_BUNDLE_DIR") else {
        eprintln!("skip: set EXMT_ANALYZE_BUNDLE_DIR to the game's bundle folder");
        return;
    };
    let files = analyze::bundle_files(Path::new(&bundle_dir)).expect("bundle list");
    assert!(!files.is_empty(), "no .bundle files in {bundle_dir}");

    // Spread the sample over the whole folder so large bundles with a `.resS`
    // sibling and small standalone ones are both covered.
    let step = (files.len() / 24).max(1);
    let mut checked = 0usize;
    for path in files.iter().step_by(step).take(24) {
        let scan = analyze::scan_bundle_with(path, false);
        let Some((meshes, textures, clips)) = names_of_full_open(path) else {
            // The full reader rejects the file; the partial one may still read
            // node 0 (that is an improvement, not a mismatch to assert on).
            eprintln!("skip: full open failed for {}", path.display());
            continue;
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        assert_eq!(scan.meshes, meshes, "meshes differ in {name}");
        assert_eq!(scan.textures, textures, "textures differ in {name}");
        assert_eq!(scan.clips, clips, "clips differ in {name}");
        checked += 1;
    }
    assert!(checked > 0, "no bundle in {bundle_dir} could be opened");
}
