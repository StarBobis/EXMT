//! Contract test for the NPZ path (ZIP + DEFLATE + `.npy`).
//!
//! The fixture was produced by Python/NumPy itself:
//!
//! ```python
//! np.savez_compressed(buf,
//!     positions=np.array([[0,0,0],[1,0,0],[0,1,0]], dtype=np.float32),
//!     indices=np.array([0,1,2], dtype=np.uint32),
//!     weights=np.zeros((3,4), dtype=np.float32),
//!     meta=np.array('{"format":"exmt-mesh-2","name":"tri","vertex_count":3}'))
//! ```
//!
//! Reading it back through this crate's ZIP/inflate/npy readers therefore
//! checks them against an independent writer, not against themselves.

use exmt_core::npy::Npy;
use exmt_core::zip::Archive;

const NPZ_HEX: &str = concat!(
    "504b03042d00000008000000210088ad6f20ffffffffffffffff0d001400706f736974696f6e732e6e707901001000a4",
    "000000000000004d000000000000009bec17ea1b10c9c850c650ad9e925a9c5ca46ea5a06e9366a2aea3a09e965f5452",
    "9498179f5f94920a12774bcc294e058a17672416a402f91ac63a0ac69a3a0ab50a64032e0614d0608f8d0f00504b0304",
    "2d0000000800000021001d372ab9ffffffffffffffff0b001400696e64696365732e6e7079010010008c000000000000",
    "004c000000000000009bec17ea1b10c9c850c650ad9e925a9c5ca46ea5a06e536aa2aea3a09e965f54529498179f5f94",
    "920a12774bcc294e058a17672416a402f91ac63a9a3a0ab50a14002e062060046226200600504b03042d000000080000",
    "002100c0a1cceaffffffffffffffff0b001400776569676874732e6e707901001000b000000000000000490000000000",
    "00009bec17ea1b10c9c850c650ad9e925a9c5ca46ea5a06e9366a2aea3a09e965f54529498179f5f94920a12774bcc29",
    "4e058a17672416a402f91ac63a0a269a3a0ab50a64032e06120100504b03042d0000000800000021003cbac63bffffff",
    "ffffffffff080014006d6574612e6e707901001000580100000000000099000000000000009bec17ea1b10c9c850c650",
    "ad9e925a9c5ca46ea5a06e136a6aa2aea3a09e965f54529498179f5f94920a92704bcc294e058a17672416a402f91a9a",
    "3a0ab50a9400ae6a06060625204e03e27c202e02e25c204e04e212a89c15944e05e20aa83c484e17ca06891703710654",
    "cc08aa5e074ae741cd83a94536b3046a67269a9e32a8da22a81a98ddf1409c0c756b29d46c64771a03712d100300504b",
    "01022d002d00000008000000210088ad6f204d000000a40000000d0000000000000000000000800100000000706f7369",
    "74696f6e732e6e7079504b01022d002d0000000800000021001d372ab94c0000008c0000000b00000000000000000000",
    "0080018c000000696e64696365732e6e7079504b01022d002d000000080000002100c0a1ccea49000000b00000000b00",
    "00000000000000000000800115010000776569676874732e6e7079504b01022d002d0000000800000021003cbac63b99",
    "0000005801000008000000000000000000000080019b0100006d6574612e6e7079504b05060000000004000400e30000",
    "006e0200000000"
);

fn fixture() -> Vec<u8> {
    let text: String = NPZ_HEX.chars().filter(|c| !c.is_whitespace()).collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn reads_every_array_numpy_wrote() {
    let data = fixture();
    let archive = Archive::new(&data).expect("parse archive");
    let names: Vec<&str> = archive.names().collect();
    assert_eq!(names, ["positions.npy", "indices.npy", "weights.npy", "meta.npy"]);
    // `savez_compressed` deflates every array.
    assert!(archive.entries().iter().all(|e| e.method == 8));

    let positions = Npy::parse(&archive.read("positions.npy", 1 << 20).unwrap()).unwrap();
    assert_eq!(positions.shape, vec![3, 3]);
    assert_eq!(positions.descr, "<f4");
    assert_eq!(
        positions.as_f32().unwrap(),
        vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    );

    let indices = Npy::parse(&archive.read("indices.npy", 1 << 20).unwrap()).unwrap();
    assert_eq!(indices.shape, vec![3]);
    assert_eq!(indices.as_u32().unwrap(), vec![0, 1, 2]);

    let weights = Npy::parse(&archive.read("weights.npy", 1 << 20).unwrap()).unwrap();
    assert_eq!(weights.shape, vec![3, 4]);
    assert_eq!(weights.as_f32().unwrap(), vec![0.0; 12]);

    let meta = Npy::parse(&archive.read("meta.npy", 1 << 20).unwrap()).unwrap();
    let text = meta.as_string().unwrap();
    assert!(text.contains("\"name\":\"tri\""), "{text}");
    assert!(text.contains("exmt-mesh-2"), "{text}");
}

#[test]
fn a_damaged_archive_is_rejected() {
    assert!(Archive::new(b"PK").is_err());
    let mut data = fixture();
    // Corrupt the end-of-central-directory signature.
    let last = data.len() - 22;
    data[last] ^= 0xFF;
    assert!(Archive::new(&data).is_err());
}
