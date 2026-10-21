//! End-to-end harness for the Rust `modpack` implementation.
//!
//! Point `EXMT_MODPACK_SPEC` at a texture-only spec whose `bundles` point at
//! real pristine bundles. The test runs the same code path the Tauri command
//! uses and asserts the summary plus a written bundle.

use std::path::Path;

use exmt_modbox_lib::modpack;

#[test]
fn runs_a_texture_spec_when_given() {
    let Ok(spec) = std::env::var("EXMT_MODPACK_SPEC") else {
        eprintln!("skip: set EXMT_MODPACK_SPEC to a generated modpack spec");
        return;
    };
    let line = modpack::run(Path::new(&spec), true).expect("modpack");
    let summary: serde_json::Value = serde_json::from_str(&line).expect("summary json");
    assert_eq!(summary["bundles"].as_u64(), Some(1), "{line}");
    let files = summary["files"].as_array().expect("files");
    assert_eq!(files.len(), 1);
    let outdir = Path::new(summary["dir"].as_str().expect("dir"));
    let bundle = outdir.join(files[0].as_str().unwrap());
    assert!(bundle.is_file(), "{} missing", bundle.display());
    assert!(outdir.join("mod.json").is_file());
}
