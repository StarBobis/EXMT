//! Run the full Rust GameCache analysis (packs included).
//!
//! `cargo run --example analyze_run_cli -- <bundle> <cache> <limit> <workers> [packs] [bones]`

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: analyze_run_cli <bundle> <cache> <limit> <workers> [packs] [bones]");
        std::process::exit(2);
    }
    let limit: Option<usize> = match args[2].parse::<usize>() {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    };
    let options = exmt_modbox_lib::analyze::Options {
        workers: args[3].parse().unwrap_or(6),
        limit,
        with_packs: args.get(4).map(|v| v == "packs").unwrap_or(false),
        with_bones: args.get(5).map(|v| v == "bones").unwrap_or(false),
    };
    let Some(tool) = exmt_core::texconv::Texconv::locate() else {
        eprintln!("texconv not found");
        std::process::exit(2);
    };
    match exmt_modbox_lib::analyze::run(Path::new(&args[0]), Path::new(&args[1]), &options, &tool) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
