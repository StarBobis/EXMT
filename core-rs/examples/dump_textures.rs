//! Decode every Texture2D of a bundle into a stable text projection.
//!
//! Usage: `cargo run --release --example dump_textures -- <bundle>`
//!
//! One line per texture: `<name>|<w>x<h>|<format>|<payload bytes>|<fnv64 of RGBA>`,
//! or `<name>|...|ERR <reason>` when mip 0 cannot be decoded. The hash covers
//! the upright RGBA image the previewer would show, so it can be compared
//! against the Python core's `decode_texture` output for the same asset:
//! identical lines mean identical pixels, not merely identical lengths.

use std::process::ExitCode;

use exmt_core::container::Node;
use exmt_core::serialized::SerializedFile;
use exmt_core::texture::Texture;
use exmt_core::{container, typetree};

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn run(path: &str) -> Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("container: {e}"))?;
    let nodes: Vec<Node> =
        container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;
    let sf = SerializedFile::parse(&nodes[0].data).map_err(|e| format!("serialized: {e}"))?;

    let mut out = String::new();
    for (path_id, obj) in &sf.objects {
        let Some(ty) = sf.type_of(obj) else { continue };
        if ty.class_id != 28 {
            continue;
        }
        let tree = typetree::read_value(&sf, obj).map_err(|e| format!("object {path_id}: {e}"))?;
        let tex = Texture::from_value(&tree).map_err(|e| format!("object {path_id}: {e}"))?;
        let head = format!("{}|{}x{}|{}", tex.name, tex.width, tex.height, tex.format);
        let payload_len = tex.payload(&nodes).map(|p| p.len()).unwrap_or(0);
        let line = match tex.decode_rgba(&nodes) {
            Ok(rgba) => format!("{head}|{payload_len}|{:016x}", fnv64(&rgba)),
            Err(e) => format!("{head}|{payload_len}|ERR {e}"),
        };
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} <bundle>", args[0]);
        return ExitCode::FAILURE;
    }
    match run(&args[1]) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
