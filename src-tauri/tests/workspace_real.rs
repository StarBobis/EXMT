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
    // Textures are the original payload in a DDS wrapper plus a JPEG preview:
    // the DDS must parse, cover the full mip chain and match the row's metadata.
    let textures = manifest["textures"].as_array().expect("textures");
    assert!(!textures.is_empty(), "no textures exported");
    for row in textures {
        let file = row["file"].as_str().expect("texture file");
        assert!(file.ends_with(".dds"), "{file} is not a DDS");
        let bytes = std::fs::read(out.join(file)).expect("read dds");
        let dds = exmt_core::dds::parse(&bytes).expect("parse dds");
        assert_eq!(dds.width as u64, row["width"].as_u64().unwrap());
        assert_eq!(dds.height as u64, row["height"].as_u64().unwrap());
        assert_eq!(dds.mip_count as u64, row["mips"].as_u64().unwrap());
        assert_eq!(
            dds.unity_format().unwrap() as u64,
            row["format"].as_u64().unwrap()
        );
        let payload = dds.payload(&bytes).expect("dds payload");
        let expected = exmt_core::texture::expected_bytes(
            dds.unity_format().unwrap(),
            dds.width as usize,
            dds.height as usize,
            dds.mip_count as usize,
        )
        .expect("mip chain size");
        assert_eq!(payload.len(), expected, "{file}: truncated mip chain");
        let preview = row["preview_file"].as_str().expect("preview_file");
        assert!(preview.ends_with(".jpg"), "{preview} is not a JPEG");
        let jpg = std::fs::read(out.join(preview)).expect("read jpg");
        assert_eq!(&jpg[..2], &[0xff, 0xd8], "{preview}: not a JPEG");
    }
    let anims = manifest["animations"].as_array().unwrap();
    assert_eq!(
        anims.len(),
        summary["animations"].as_u64().unwrap() as usize
    );
    for row in anims {
        let file = row["file"].as_str().expect("animation file");
        let path = out.join(file);
        assert!(path.is_file(), "{} missing", path.display());
        let clip: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(clip["format"], "exmt-anim-1");
        assert!(!clip["bones"].as_array().unwrap().is_empty());
    }
    let _ = std::fs::remove_dir_all(&out);
}
