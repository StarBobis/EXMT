//! Export every Texture2D of a bundle as a DDS that carries the payload verbatim.
//!
//! Usage: `cargo run --release --example export_dds -- <bundle> <out_dir> [name_filter]`
//!
//! A Unity Texture2D stores its pixels as a bare block-compressed payload (the
//! `.resS` bytes); DDS is only that payload with a header in front, so this tool
//! writes the whole mip chain unchanged and checks the header parses back. No
//! texconv is involved: converting PNG -> DDS in texconv decodes and re-encodes,
//! which is lossy and must not be called "export".
//!
//! Unity stores rows bottom-up, DDS viewers assume top-down, so the image looks
//! upside down in a normal viewer; the workspace export relies on the same bytes
//! and lets the Blender add-on flip V in the material instead.
//!
//! One line per texture: `<name>|<w>x<h>|<unity format>|<mips>|<dds bytes>|eq`.

use std::process::ExitCode;

use exmt_core::container;
use exmt_core::dds;
use exmt_core::serialized::SerializedFile;
use exmt_core::texture::Texture;
use exmt_core::typetree;

fn sanitize(name: &str) -> String {
    if name.is_empty() {
        return "unnamed".to_string();
    }
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect()
}

fn run(args: &[String]) -> Result<(), String> {
    // args[0] is the executable path; only the operands are positional.
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() < 2 {
        return Err(format!(
            "usage: {} <bundle> <out_dir> [name_filter]",
            args.first().map(String::as_str).unwrap_or("export_dds")
        ));
    }
    let bundle = positional[0].as_str();
    let out_dir = std::path::PathBuf::from(positional[1].as_str());
    let filter = positional.get(2).map(|s| s.as_str()).unwrap_or("");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir: {e}"))?;

    let raw = std::fs::read(bundle).map_err(|e| format!("read: {e}"))?;
    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("container: {e}"))?;
    let nodes = container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;
    let sf = SerializedFile::parse(&nodes[0].data).map_err(|e| format!("serialized: {e}"))?;

    let mut exported = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    for (path_id, obj) in &sf.objects {
        let Some(ty) = sf.type_of(obj) else { continue };
        if ty.class_id != 28 {
            continue;
        }
        let tree = match typetree::read_value(&sf, obj) {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("object {path_id}: typetree: {e}");
                failed += 1;
                continue;
            }
        };
        let texture = match Texture::from_value(&tree) {
            Ok(texture) => texture,
            Err(e) => {
                eprintln!("object {path_id}: texture: {e}");
                failed += 1;
                continue;
            }
        };
        if !filter.is_empty() && !texture.name.contains(filter) {
            continue;
        }
        let dds = match texture.to_dds(&nodes) {
            Ok(dds) => dds,
            Err(e) => {
                eprintln!("{}: {e}", texture.name);
                if matches!(e, exmt_core::Error::Unsupported { .. }) {
                    skipped += 1;
                } else {
                    failed += 1;
                }
                continue;
            }
        };
        let parsed = dds::parse(&dds).expect("header written by to_dds must parse");
        let payload = parsed.payload(&dds).expect("payload written by to_dds");
        let eq = parsed.width == texture.width as u32
            && parsed.height == texture.height as u32
            && parsed.mip_count as usize == texture.mip_count
            && payload.len() == texture.expected_bytes().unwrap_or(0);
        let path = out_dir.join(format!("{}.dds", sanitize(&texture.name)));
        if let Err(e) = std::fs::write(&path, &dds) {
            eprintln!("{}: write: {e}", texture.name);
            failed += 1;
            continue;
        }
        println!(
            "{}|{}x{}|{}|{}|{}|{}",
            texture.name,
            texture.width,
            texture.height,
            texture.format,
            texture.mip_count,
            dds.len(),
            if eq { "eq" } else { "MISMATCH" }
        );
        exported += 1;
    }
    eprintln!(
        "exported {exported}, skipped {skipped}, failed {failed} -> {}",
        out_dir.display()
    );
    if failed > 0 {
        return Err(format!("{failed} texture(s) failed"));
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
