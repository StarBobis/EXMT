//! Write a small NPZ fixture with the crate's writer, for a Python check.
//!
//! `cargo run --example write_npz -- <out.npz>`; load it with
//! `numpy.load(path, allow_pickle=False)`.

use exmt_core::npz::Npz;

fn main() {
    let out = std::env::args().nth(1).expect("usage: write_npz <out.npz>");
    let mut npz = Npz::new();
    npz.add_f32("positions", &[2, 3], &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    npz.add_u32("indices", &[2], &[4, 5]);
    npz.add_u8("raw", &[1, 2, 3]);
    npz.add_string("meta", "{\"format\":\"exmt-mesh-2\",\"name\":\"tri\"}");
    std::fs::write(&out, npz.write()).expect("write npz");
    println!("wrote {out}");
}
