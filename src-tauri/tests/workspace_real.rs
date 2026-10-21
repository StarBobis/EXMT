//! End-to-end harness for the Rust `workspace` command.
//!
//! Set `EXMT_WORKSPACE_SPEC` to a spec with `animations: []`; the test writes
//! into a temporary directory and checks the files the Blender add-on reads.

use std::path::Path;

#[test]
fn runs_a_real_workspace_spec() {
    let Ok(spec) = std::env::var("EXMT_WORKSPACE_SPEC") else {
        eprintln!("skip: set EXMT_WORKSPACE_SPEC to a generated workspace spec");
        return;
    };
    let Some(tool) = exmt_core::texconv::Texconv::locate() else {
        eprintln!("skip: texconv.exe not available");
        return;
    };
    let out = std::env::temp_dir().join(format!("exmt-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let line = exmt_modbox_lib::workspace::run(&out, Path::new(&spec), &tool).expect("workspace");
    let summary: serde_json::Value = serde_json::from_str(&line).expect("summary");
    assert!(summary["meshes"].as_u64().unwrap() > 0);
    assert!(summary["bones"].as_u64().unwrap() > 0);
    assert!(out.join("workspace.json").is_file());
    assert!(out.join("skeleton.json").is_file());
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("workspace.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["format"], "exmt-workspace-1");
    assert_eq!(
        manifest["meshes"].as_array().unwrap().len(),
        summary["meshes"].as_u64().unwrap() as usize
    );
    let first = manifest["meshes"][0]["file"].as_str().unwrap();
    assert!(out.join(first).is_file());
    let _ = std::fs::remove_dir_all(&out);
}
