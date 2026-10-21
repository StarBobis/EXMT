//! Real-bundle write contract, opt-in by environment variable.
//!
//! `EXMT_WRITE_BUNDLE` points at a UnityFS bundle (any game with v17–v21
//! SerializedFiles works). The tests check the two properties a write path
//! needs before it is trusted with mods:
//!
//! * rebuilding with no changes keeps **every object payload byte-identical**;
//! * patching one texture through its type tree changes that texture and
//!   nothing else, and the saved bundle reopens with the same pixels.
//!
//! The bundle is rebuilt in memory; nothing is written to the repo or the game.

use std::collections::BTreeMap;
use std::path::Path;

use exmt_core::asset;
use exmt_core::{texture, typetree};

mod support;

fn sample() -> Option<String> {
    if let Ok(path) = std::env::var("EXMT_WRITE_BUNDLE") {
        if Path::new(&path).is_file() {
            return Some(path);
        }
        eprintln!("EXMT_WRITE_BUNDLE is not a file: {path}; falling back to the GF2 sample");
    }
    support::gf2_bundle().map(|path| path.display().to_string())
}

#[test]
fn untouched_rebuild_keeps_every_object() {
    let Some(path) = sample() else { return };
    let bundle = asset::open(&path).expect("open");
    let saved = bundle.save_patched(&BTreeMap::new(), false).expect("rebuild");
    let back = asset::open_bytes(&saved).expect("re-open rebuilt");
    assert_eq!(back.serialized.objects.len(), bundle.serialized.objects.len());
    for (path_id, obj) in &bundle.serialized.objects {
        let again = back.serialized.object(*path_id).expect("same path ids");
        assert_eq!(again.type_id, obj.type_id, "type of object {path_id}");
        assert_eq!(
            back.serialized.object_bytes(again),
            bundle.serialized.object_bytes(obj),
            "payload of object {path_id} changed"
        );
    }
}

#[test]
fn a_texture_patch_changes_only_that_texture() {
    let Some(path) = sample() else { return };
    let bundle = asset::open(&path).expect("open");

    // Pick the first texture this crate can decode.
    let mut chosen = None;
    for (path_id, tex) in bundle.serialized.objects.iter().filter_map(|(id, obj)| {
        let ty = bundle.serialized.type_of(obj)?;
        if ty.class_id != asset::CLASS_TEXTURE {
            return None;
        }
        let value = typetree::read_value(&bundle.serialized, obj).ok()?;
        let tex = texture::Texture::from_value(&value).ok()?;
        Some((*id, tex))
    }) {
        if !matches!(tex.format, 10 | 12 | 25 | 26 | 27 | 4) || tex.width == 0 || tex.height == 0 {
            continue;
        }
        chosen = Some((path_id, tex));
        break;
    }
    let Some((path_id, tex)) = chosen else {
        eprintln!("skip: bundle has no decodable Texture2D");
        return;
    };

    let payload = tex.payload(&bundle.nodes).expect("payload").into_owned();
    let before = tex.decode_rgba(&bundle.nodes).expect("decode");

    // Store the pixels back through the type tree. Feeding the stored bytes
    // through `flip_vertical` first reproduces the top-down form an encoder
    // would hand us, so the round trip must be lossless.
    let top_down = texture::flip_vertical(
        &payload,
        tex.format,
        tex.width,
        tex.height,
        tex.mip_count,
    )
    .expect("flip");
    let obj = *bundle.serialized.object(path_id).unwrap();
    let mut tree = typetree::read_value(&bundle.serialized, &obj).expect("read tree");
    let stored = texture::store_image(&mut tree, &top_down).expect("store");
    assert!(stored.stream.is_none(), "patch must inline the payload");
    let bytes = typetree::write_value(&bundle.serialized, &obj, &tree).expect("write tree");

    let mut replacements = BTreeMap::new();
    replacements.insert(path_id, bytes);
    let saved = bundle.save_patched(&replacements, false).expect("rebuild");
    let back = asset::open_bytes(&saved).expect("re-open");

    let again = back.texture(&tex.name).expect("texture still there");
    let after = again.decode_rgba(&back.nodes).expect("decode patched");
    assert_eq!(before, after, "pixels changed across the patch");

    // No other object may move.
    for (id, original) in &bundle.serialized.objects {
        if *id == path_id {
            continue;
        }
        let other = back.serialized.object(*id).expect("object survives");
        assert_eq!(
            back.serialized.object_bytes(other),
            bundle.serialized.object_bytes(original),
            "object {id} changed because of a texture patch"
        );
    }
}
