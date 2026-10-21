//! Differential driver for `store` commands.
//!
//! Usage: `cargo run --example store_cli -- backup <src> <dst> <list> [prefer]`
//! Prints the same JSON summary the Tauri command returns, so it can be diffed
//! against `python -m exmt_core backup ...` on a fixture.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match exmt_modbox_lib::store::run(&args) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
