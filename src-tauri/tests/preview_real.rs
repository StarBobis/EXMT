//! Contract test for the preview commands (`thumbs`, `skinfo`).
//!
//! Set `EXMT_PREVIEW_BUNDLE` to a bundle with at least one texture. The test
//! checks the JSON shape the frontend expects and that texconv actually produced
//! a PNG (base64 signature), not a raw error row.

use std::path::Path;

use exmt_core::asset;
use exmt_core::texconv::Texconv;
use exmt_modbox_lib::preview;

#[test]
fn thumbs_returns_png_rows() {
    let Ok(path) = std::env::var("EXMT_PREVIEW_BUNDLE") else {
        eprintln!("skip: set EXMT_PREVIEW_BUNDLE to a bundle with textures");
        return;
    };
    let Some(tool) = Texconv::locate() else {
        eprintln!("skip: texconv.exe not available");
        return;
    };
    let bundle = asset::open(Path::new(&path)).expect("open bundle");
    let name = "CharacterOverlayTex".to_string();
    let text = preview::thumbs(&bundle, 160, &[name.clone()], &tool).expect("thumbs");
    let json: serde_json::Value = serde_json::from_str(&text).expect("json");
    let row = &json["textures"][0];
    assert_eq!(row["name"], name);
    assert!(row.get("error").is_none(), "thumbs failed: {row}");
    let png = row["png"].as_str().expect("png");
    assert!(
        png.starts_with("iVBORw0KGgo"),
        "not a PNG: {}",
        &png[..16.min(png.len())]
    );
    assert!(row["width"].as_u64().unwrap() > 0);
    assert!(row["height"].as_u64().unwrap() > 0);
}

#[test]
fn skinfo_lists_meshes_when_present() {
    // Streamed meshes (vertex data in `.resS`) cannot be decoded here; point
    // this at a bundle whose meshes are inline, e.g. the mesh fixture.
    let Ok(path) = std::env::var("EXMT_PREVIEW_MESH_BUNDLE") else {
        eprintln!("skip: set EXMT_PREVIEW_MESH_BUNDLE to a bundle with inline meshes");
        return;
    };
    let bundle = asset::open(Path::new(&path)).expect("open bundle");
    let text = preview::skinfo(&bundle).expect("skinfo");
    let json: serde_json::Value = serde_json::from_str(&text).expect("json");
    let rows = json["meshes"].as_array().expect("meshes");
    assert!(!rows.is_empty(), "expected at least one mesh");
    for row in rows {
        assert!(row["name"].is_string());
        assert!(row["verts"].as_u64().unwrap() > 0);
        assert!(row["tris"].as_u64().is_some());
    }
    eprintln!("skinfo: {} meshes", rows.len());
}
