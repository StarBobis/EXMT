//! Contract tests against a real shipped bundle.
//!
//! These read a bundle from the developer machine and return early when it is
//! absent, so the suite still runs on a clean checkout. The assertions pin the
//! values an independent implementation produced for the same file, which is
//! what makes this a cross-check rather than a self-consistency check.

use exmt_core::container;

mod support;

fn sample() -> Option<Vec<u8>> {
    let path = support::gf2_bundle()?;
    Some(std::fs::read(path).expect("read sample bundle"))
}

#[test]
fn parses_header_and_tables() {
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    // Decrypted bundles must start with the archive signature.
    assert_eq!(&plain[..8], b"UnityFS\0");

    let info = container::parse(&plain).expect("parse");
    assert_eq!(info.version, 7);
    assert_eq!(info.engine, "5.x.x");
    assert_eq!(info.revision, "2019.4.29f1");
    assert_eq!(info.size as usize, plain.len());
    assert_eq!(info.blocks.len(), 21);
    assert_eq!(info.nodes.len(), 1);
    assert_eq!(info.nodes[0].path, "CAB-da88ca698e51a177b5a2f870531774a8");
    assert_eq!(info.nodes[0].flags, 4, "node 0 is the SerializedFile");
    // 21 blocks of 128 KiB plus a tail.
    assert_eq!(info.blocks.iter().map(|b| b.uncompressed as u64).sum::<u64>(), 2_740_916);
    assert!(info.blocks.iter().all(|b| b.is_lz4()));
}

#[test]
fn decompresses_the_whole_stream() {
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse");
    let stream = container::read_stream(&plain, &info).expect("read_stream");
    assert_eq!(stream.len(), 2_740_916);
    // The one node covers the whole stream.
    assert_eq!(info.nodes[0].offset, 0);
    assert_eq!(info.nodes[0].size as usize, stream.len());
    // A SerializedFile starts with its metadata size, then file size; the file
    // size is the node length, which is a cheap independent consistency check.
    let declared = u32::from_be_bytes([stream[4], stream[5], stream[6], stream[7]]);
    assert_eq!(declared as usize, stream.len());
}

#[test]
fn extracts_node_payloads() {
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].data.len(), 2_740_916);
    assert_eq!(nodes[0].path, "CAB-da88ca698e51a177b5a2f870531774a8");
}

#[test]
fn rebuild_preserves_every_node_payload() {
    // The container-level acceptance criterion: rebuilding from the extracted
    // payloads must produce a bundle that parses back to the same payloads.
    // Byte equality is not achievable here because our LZ4 encoder settles at a
    // slightly different size than the game's high-compression encoder.
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");

    let rebuilt = container::build(&nodes, &info, true).expect("build");
    // build() already returns plaintext; decrypting again would XOR a
    // near-zero keystream into the first 32 KiB and corrupt the block table.
    let rebuilt_info = container::parse(&rebuilt).expect("parse rebuilt");
    let rebuilt_nodes =
        container::node_payloads(&rebuilt, &rebuilt_info).expect("nodes rebuilt");

    assert_eq!(rebuilt_nodes.len(), nodes.len());
    for (a, b) in nodes.iter().zip(rebuilt_nodes.iter()) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.flags, b.flags);
        assert_eq!(a.data, b.data, "payload changed across rebuild");
    }

    // Header fields the rebuild must carry over verbatim.
    assert_eq!(rebuilt_info.version, info.version);
    assert_eq!(rebuilt_info.engine, info.engine);
    assert_eq!(rebuilt_info.revision, info.revision);
    assert_eq!(rebuilt_info.flags, info.flags);
}

#[test]
fn rebuild_reencrypts_with_the_original_key() {
    // Re-encryption must use the ciphertext's first four bytes. Deriving from
    // plaintext yields a zero key and a bundle only we can read, so this pins
    // the round-trip back to a decryptable header.
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");
    let rebuilt = container::build(&nodes, &info, true).expect("build");

    let head4 = &raw[..4];
    let cipher = container::encrypt(&rebuilt, head4).expect("encrypt");
    assert_eq!(&cipher[..4], head4, "first four ciphertext bytes must be stable");
    let again = container::decrypt(&cipher).expect("decrypt again");
    assert_eq!(&again[..8], b"UnityFS\0");
    container::parse(&again).expect("rebuilt+encrypted bundle parses");
}

#[test]
fn stored_mode_rebuild_also_round_trips() {
    let Some(raw) = sample() else { return };
    let plain = container::decrypt(&raw).expect("decrypt");
    let info = container::parse(&plain).expect("parse");
    let nodes = container::node_payloads(&plain, &info).expect("nodes");

    let rebuilt = container::build(&nodes, &info, false).expect("build stored");
    let rebuilt_info = container::parse(&rebuilt).expect("parse stored");
    assert!(
        rebuilt_info.blocks.iter().all(|b| !b.is_lz4()),
        "stored mode must not emit LZ4 blocks"
    );
    let rebuilt_nodes = container::node_payloads(&rebuilt, &rebuilt_info).expect("nodes stored");
    assert_eq!(rebuilt_nodes[0].data, nodes[0].data);
}

