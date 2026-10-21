//! Differential driver for the `workspace` command.
//!
//! `cargo run --example workspace_cli -- <outdir> <spec.json>`

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [outdir, spec] = &args[..] else {
        eprintln!("usage: workspace_cli <outdir> <spec.json>");
        std::process::exit(2);
    };
    let Some(tool) = exmt_core::texconv::Texconv::locate() else {
        eprintln!("texconv.exe not found");
        std::process::exit(2);
    };
    match exmt_modbox_lib::workspace::run(Path::new(outdir), Path::new(spec), &tool) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
