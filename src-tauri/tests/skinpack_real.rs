//! End-to-end harness for the Rust `skinpack` implementation.
//!
//! Point `EXMT_SKINPACK_SPEC` at a spec JSON whose `bundle_dir` holds real
//! `<hash>.bundle` files. The test runs the same code path the Tauri command
//! uses, so a run against the Python core's spec is a true differential.
//!
//! It is opt-in on purpose: a checkout has no game bundles, and that must not
//! fail the suite.

use std::path::Path;

use exmt_core::texconv::Texconv;

#[test]
fn runs_a_real_spec_when_given() {
    let Ok(spec) = std::env::var("EXMT_SKINPACK_SPEC") else {
        eprintln!("skip: set EXMT_SKINPACK_SPEC to a generated skinpack spec");
        return;
    };
    let tool = match Texconv::locate() {
        Some(tool) => tool,
        None => {
            eprintln!("skip: texconv.exe not available");
            return;
        }
    };
    let line = exmt_modbox_lib::skinpack::run(Path::new(&spec), &tool).expect("skinpack");
    let summary: serde_json::Value = serde_json::from_str(&line).expect("summary json");
    assert_eq!(summary["format"], "exmt-skinpack-2");
    assert!(summary["textures"].as_u64().unwrap() > 0);
}
