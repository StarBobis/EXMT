//! Run one skinpack spec through the same code the app uses.
//!
//! `cargo run --example skinpack_cli -- <spec.json>`
//!
//! Useful to time a single skin while tuning the analysis, and to rebuild one
//! pack by hand without running a full scan.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(spec) = args.first() else {
        eprintln!("usage: skinpack_cli <spec.json>");
        std::process::exit(2);
    };
    let Some(tool) = exmt_core::texconv::Texconv::locate() else {
        eprintln!("texconv not found");
        std::process::exit(2);
    };
    match exmt_modbox_lib::skinpack::run(Path::new(spec), &tool) {
        Ok(line) => println!("{line}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
