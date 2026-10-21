//! Real-data contract for the texture layer, opt-in by environment variable.
//!
//! Point `EXMT_TEXTURE_ASSETS` at a Unity `SerializedFile` that carries
//! Texture2D objects (a shipped `.assets` from any Unity 2017–2022 game works;
//! it does not have to be GF2). The test then checks every texture it can
//! decode against the invariants the Python core relies on:
//!
//! * the payload length equals the declared mip chain (`expected_bytes`),
//! * mip 0 decodes to exactly `width * height * 4` bytes,
//! * storing that image back (top-down), re-serializing the object and decoding
//!   it again reproduces the same pixels.
//!
//! The streamed half of a texture is read from the sibling `.resS` file named
//! by `m_StreamData.path`, which is how the pieces sit on disk before packing.
//!
//! Without the variable the test prints a skip line and returns: a clean
//! checkout has no game assets, and that must not be a failure.

use std::path::{Path, PathBuf};

use exmt_core::container::Node;
use exmt_core::serialized::SerializedFile;
use exmt_core::texture::{self, Texture};
use exmt_core::typetree::{self, Value};

fn sample() -> Option<PathBuf> {
    match std::env::var("EXMT_TEXTURE_ASSETS") {
        Ok(path) if Path::new(&path).exists() => Some(PathBuf::from(path)),
        _ => {
            eprintln!("skip: set EXMT_TEXTURE_ASSETS to a SerializedFile with Texture2D objects");
            None
        }
    }
}

fn image_field(tree: &Value) -> Option<Vec<u8>> {
    ["image data", "m_ImageData", "image_data"]
        .iter()
        .find_map(|name| {
            tree.get(name)
                .and_then(|v| v.as_bytes().map(|b| b.to_vec()))
        })
}

#[test]
fn every_texture_round_trips_through_its_type_tree() {
    let Some(path) = sample() else { return };
    let raw = std::fs::read(&path).expect("read assets");
    let sf = SerializedFile::parse(&raw).expect("parse SerializedFile");
    let dir = path.parent().unwrap_or(Path::new("."));

    let mut nodes: Vec<Node> = vec![Node {
        path: path.file_name().unwrap().to_string_lossy().into_owned(),
        data: raw.clone(),
        flags: 0,
    }];
    let mut checked = 0usize;
    let mut formats = std::collections::BTreeMap::new();

    for (path_id, obj) in &sf.objects {
        let Some(ty) = sf.type_of(obj) else { continue };
        if ty.class_id != 28 {
            continue;
        }
        let tree = typetree::read_value(&sf, obj).unwrap_or_else(|e| panic!("read {path_id}: {e}"));
        let tex = Texture::from_value(&tree).unwrap_or_else(|e| panic!("texture {path_id}: {e}"));
        if tex.width == 0 || tex.height == 0 {
            continue;
        }
        let Some(format) = texture::format(tex.format) else {
            continue;
        };
        if format.bc.is_none() && tex.format != 4 {
            continue;
        }

        // Attach the stream file the texture points at, once.
        if let Some(stream) = &tex.stream {
            let name = stream
                .path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&stream.path);
            if !nodes.iter().any(|n| n.path == name) {
                let stream_path = dir.join(name);
                assert!(
                    stream_path.exists(),
                    "texture {path_id} streams from {name}, which is not next to {}",
                    path.display()
                );
                nodes.push(Node {
                    path: name.to_string(),
                    data: std::fs::read(&stream_path).expect("read stream"),
                    flags: 0,
                });
            }
        }

        let payload = tex
            .payload(&nodes)
            .unwrap_or_else(|e| panic!("payload {path_id}: {e}"));
        assert_eq!(
            payload.len(),
            tex.expected_bytes().expect("layout"),
            "texture {path_id} ({} {}x{} format {}) payload does not match its mip chain",
            tex.name,
            tex.width,
            tex.height,
            tex.format
        );

        let before = tex
            .decode_rgba(&nodes)
            .unwrap_or_else(|e| panic!("decode {path_id} {}: {e}", tex.name));
        assert_eq!(before.len(), tex.width * tex.height * 4);

        // The stored bytes are bottom-up; flipping gives the top-down form an
        // encoder would hand us, and storing it again must be lossless.
        let top_down =
            texture::flip_vertical(&payload, tex.format, tex.width, tex.height, tex.mip_count)
                .expect("flip");
        let mut edited = tree.clone();
        let stored = texture::store_image(&mut edited, &top_down).expect("store");
        assert!(
            stored.stream.is_none(),
            "store_image must inline the payload"
        );
        let bytes = typetree::write_value(&sf, obj, &edited).expect("serialize edited object");
        let back = typetree::decode_with_type(&sf, ty, &bytes).expect("decode edited object");
        assert_eq!(
            image_field(&back).expect("image data"),
            image_field(&edited).expect("image data"),
            "image data changed across serialization"
        );
        let again = Texture::from_value(&back).expect("re-parse");
        assert_eq!(again.stream, None);
        let after = again.decode_rgba(&[]).expect("decode edited");
        assert_eq!(
            before, after,
            "texture {path_id} {} changed across a round trip",
            tex.name
        );

        *formats.entry(format.name).or_insert(0usize) += 1;
        checked += 1;
    }

    assert!(
        checked > 0,
        "no decodable textures found in {}",
        path.display()
    );
    eprintln!("checked {checked} textures: {formats:?}");
}
