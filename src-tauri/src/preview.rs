//! Small read-only preview commands that used to round-trip through Python.
//!
//! `thumbs` is the texture grid: decode with texconv, downscale with FANT and
//! return base64 PNGs. `skinfo` lists the meshes of one bundle for the preview
//! picker. Both are on the interactive path, so keeping them here avoids
//! starting Python (and Pillow) for a click.

use exmt_core::texconv::Texconv;
use exmt_core::{asset, skin};

/// `thumbs <bundle> <size> <texture...>` → `{"textures": [...]}`.
///
/// A broken texture becomes `{"name", "error"}` like the Python command; one
/// bad asset must not sink the whole grid.
pub fn thumbs(
    bundle: &asset::OpenedBundle,
    size: usize,
    names: &[String],
    texconv: &Texconv,
) -> Result<String, String> {
    let mut rows = Vec::with_capacity(names.len());
    for name in names {
        let row = match bundle
            .texture_object(name)
            .and_then(|(_, texture)| skin::thumb(&texture, &bundle.nodes, texconv, size))
        {
            Ok(thumb) => serde_json::json!({
                "name": thumb.name,
                "width": thumb.width,
                "height": thumb.height,
                "png": thumb.png,
            }),
            Err(e) => serde_json::json!({
                "name": name,
                "error": short(&e.to_string(), 200),
            }),
        };
        rows.push(row);
    }
    serde_json::to_string(&serde_json::json!({ "textures": rows }))
        .map_err(|e| format!("serialize thumbs: {e}"))
}

/// `skinfo <bundle>` → `{"meshes": [{name, verts, tris, bones}]}`.
pub fn skinfo(bundle: &asset::OpenedBundle) -> Result<String, String> {
    let mut rows = Vec::new();
    for mesh in bundle.meshes().map_err(|e| e.to_string())? {
        rows.push(serde_json::json!({
            "name": mesh.name,
            "verts": mesh.vertex_count,
            "tris": mesh.indices.len() / 3,
            "bones": mesh.bone_hashes.len(),
        }));
    }
    serde_json::to_string(&serde_json::json!({ "meshes": rows }))
        .map_err(|e| format!("serialize skinfo: {e}"))
}

fn short(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
