//! Rebuild a bundle end to end and write it back, for cross-implementation checks.
//!
//! Usage: `cargo run --example rebuild_bundle -- <input.bundle> <output.bundle>`
//!
//! Reads a NEP2-encrypted bundle, decrypts it, parses the container, extracts
//! the node payloads, rebuilds the container from them, and re-encrypts with the
//! original ciphertext head. The output should be loadable by the game and
//! readable by any other correct implementation.

use std::process::ExitCode;

use exmt_core::container;

fn run(src: &str, dst: &str) -> Result<(), String> {
    let raw = std::fs::read(src).map_err(|e| format!("read {src}: {e}"))?;
    if raw.len() < 4 {
        return Err("input too short".into());
    }
    let head4 = raw[..4].to_vec();

    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("parse: {e}"))?;
    let nodes = container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;

    let rebuilt = container::build(&nodes, &info, true).map_err(|e| format!("build: {e}"))?;
    let cipher = container::encrypt(&rebuilt, &head4).map_err(|e| format!("encrypt: {e}"))?;
    std::fs::write(dst, &cipher).map_err(|e| format!("write {dst}: {e}"))?;

    println!(
        "{} ({} bytes) -> {} ({} bytes), {} node(s), {} block(s)",
        src,
        raw.len(),
        dst,
        cipher.len(),
        nodes.len(),
        info.blocks.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: {} <input.bundle> <output.bundle>", args[0]);
        return ExitCode::FAILURE;
    }
    match run(&args[1], &args[2]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
