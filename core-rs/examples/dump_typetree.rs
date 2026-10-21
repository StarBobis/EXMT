//! Dump every type-tree node of a bundle, one per line, for cross-checking.
//!
//! Usage: `cargo run --quiet --example dump_typetree -- <bundle>`
//!
//! Output format (tab-separated, stable for diffing):
//!   `<type_index> <node_index> <level> <type> <name> <byte_size> <meta_flag>`

use std::process::ExitCode;

use exmt_core::{container, serialized};

fn run(path: &str) -> Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("container: {e}"))?;
    let nodes = container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;
    let sf = serialized::SerializedFile::parse(&nodes[0].data).map_err(|e| format!("serialized: {e}"))?;

    let mut out = String::new();
    out.push_str(&format!(
        "# header\tversion={} engine={} rev={} endian={} platform={} types={} objects={}\n",
        sf.version,
        "n/a",
        "n/a",
        match sf.endian {
            serialized::Endian::Little => "little",
            serialized::Endian::Big => "big",
        },
        sf.platform,
        sf.types.len(),
        sf.objects.len()
    ));
    for (ti, ty) in sf.types.iter().enumerate() {
        out.push_str(&format!("# type\t{ti}\tclass_id={}\tnodes={}\n", ty.class_id, ty.nodes.len()));
        for (ni, n) in ty.nodes.iter().enumerate() {
            out.push_str(&format!(
                "{ti}\t{ni}\t{}\t{}\t{}\t{}\t{}\n",
                n.level, n.type_name, n.name, n.byte_size, n.meta_flag
            ));
        }
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
