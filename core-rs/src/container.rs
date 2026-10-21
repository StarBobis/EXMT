//! UnityFS container: header, block table, node table, and the NEP2 prefix
//! cipher that GF2 applies to the first 32 KiB of every hot-update bundle.
//!
//! Header layout (all big-endian):
//!
//! ```text
//! "UnityFS\0"
//! u32   version
//! cstr  engine version      e.g. "5.x.x"
//! cstr  engine revision     e.g. "2019.4.29f1"
//! i64   total file size
//! u32   compressed blocksInfo size
//! u32   uncompressed blocksInfo size
//! u32   flags
//! pad   to a 16-byte boundary
//! <blocksInfo, LZ4-compressed>
//! <data blocks, concatenated>
//! ```
//!
//! The blocksInfo decodes to four hash bytes that this format leaves unused,
//! followed by the block table and node table (see [`parse`]).
//!
//! NEP2 is not standard Unity. It XORs the first [`ENC_PREFIX`] bytes with a
//! 16-byte keystream derived from the four ciphertext bytes at offset zero, so
//! the key is recoverable from the file itself and the prefix bytes double as
//! the key material. Only the prefix is affected; the rest of the file is
//! plaintext, and encrypting the whole thing leaves a valid header over garbage
//! data blocks -- a failure mode that looks like corruption, not decryption.

use crate::error::{Error, Result};
use crate::lz4;

/// Bytes at the start of a bundle that NEP2 covers.
pub const ENC_PREFIX: usize = 0x8000;
/// Uncompressed size of one data block.
pub const BLOCK_SIZE: usize = 0x20000;
/// Block flag bits 0-1 carry the compression method: 2 and 3 are LZ4.
const BLOCK_FLAG_LZ4_MASK: u16 = 0x3F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub uncompressed: u32,
    pub compressed: u32,
    pub flags: u16,
}

impl Block {
    pub fn is_lz4(&self) -> bool {
        matches!(self.flags & BLOCK_FLAG_LZ4_MASK, 2 | 3)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeEntry {
    pub offset: i64,
    pub size: i64,
    pub flags: u32,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct BundleInfo {
    pub version: u32,
    pub engine: String,
    pub revision: String,
    /// Declared total file size, from the header.
    pub size: i64,
    pub flags: u32,
    pub compressed_blocks_info_size: u32,
    pub uncompressed_blocks_info_size: u32,
    pub blocks_info_offset: usize,
    pub data_offset: usize,
    pub blocks: Vec<Block>,
    pub nodes: Vec<NodeEntry>,
    /// Bytes after the declared bundle. Some GF2 hot-update files concatenate
    /// several UnityFS archives; reads use the first one, writes must refuse.
    pub trailing: usize,
}

/// One payload inside the bundle stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub path: String,
    pub data: Vec<u8>,
    pub flags: u32,
}

/// NEP2 keystream for a given 4-byte header prefix.
///
/// The stream is the key bytes, four padding bytes, the key with its first byte
/// flipped by 0x02, then more padding. The padding byte is 0xFF when the key's
/// last byte has its top bit set and 0x00 otherwise.
pub fn keystream(head4: &[u8]) -> Result<[u8; 16]> {
    if head4.len() < 4 {
        return Err(Error::Truncated {
            what: "nep2 header",
            offset: 0,
            wanted: 4,
            available: head4.len(),
        });
    }
    let mut key = [0u8; 4];
    for i in 0..4 {
        key[i] = head4[i] ^ b"Unit"[i];
    }
    let pad: u8 = if key[3] & 0x80 != 0 { 0xFF } else { 0x00 };
    let mut ks = [0u8; 16];
    ks[0..4].copy_from_slice(&key);
    ks[4..8].fill(pad);
    ks[8] = key[0] ^ 0x02;
    ks[9..12].copy_from_slice(&key[1..4]);
    ks[12..16].fill(pad);
    Ok(ks)
}

fn xor_prefix(buf: &mut [u8], ks: &[u8; 16]) {
    let n = buf.len().min(ENC_PREFIX);
    for i in 0..n {
        buf[i] ^= ks[i % 16];
    }
}

/// Decrypt the NEP2 prefix. The inverse is [`encrypt`].
pub fn decrypt(raw: &[u8]) -> Result<Vec<u8>> {
    let ks = keystream(raw)?;
    let mut out = raw.to_vec();
    xor_prefix(&mut out, &ks);
    Ok(out)
}

/// Re-encrypt the NEP2 prefix using the key material in `head4`.
///
/// `head4` must be the four ciphertext bytes you want the output to carry --
/// normally the original file's first four bytes, so the game derives the same
/// keystream it used before.
///
/// Do not pass the *plaintext's* first four bytes: NEP2 plaintext always begins
/// with `"Unit"` (the keystream cancels it there), so deriving from plaintext
/// gives `X = "Unit" ^ "Unit" = 0`, a zero keystream, and a bundle that only
/// this program can read. This is the single easiest way to corrupt a mod.
pub fn encrypt(plain: &[u8], head4: &[u8]) -> Result<Vec<u8>> {
    let ks = keystream(head4)?;
    let mut out = plain.to_vec();
    xor_prefix(&mut out, &ks);
    Ok(out)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated {
            what,
            offset: self.pos,
            wanted: n,
            available: self.data.len().saturating_sub(self.pos),
        })?;
        if end > self.data.len() {
            return Err(Error::Truncated {
                what,
                offset: self.pos,
                wanted: n,
                available: self.data.len() - self.pos,
            });
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u32(&mut self, what: &'static str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i64(&mut self, what: &'static str) -> Result<i64> {
        let b = self.take(8, what)?;
        Ok(i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn u16(&mut self, what: &'static str) -> Result<u16> {
        let b = self.take(2, what)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn cstr(&mut self, what: &'static str) -> Result<String> {
        let rest = &self.data[self.pos..];
        let end = rest.iter().position(|&b| b == 0).ok_or(Error::Invalid {
            what,
            detail: format!("unterminated string at {}", self.pos),
        })?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(text)
    }
}

/// Parse a decrypted bundle's header and tables.
pub fn parse(plain: &[u8]) -> Result<BundleInfo> {
    if plain.len() < 8 || &plain[..8] != b"UnityFS\0" {
        return Err(Error::BadSignature { expected: "UnityFS\\0", offset: 0 });
    }
    let mut c = Cursor::new(plain);
    c.pos = 8;

    let version = c.u32("bundle version")?;
    let engine = c.cstr("engine version")?;
    let revision = c.cstr("engine revision")?;
    let size = c.i64("bundle size")?;
    let cbis = c.u32("blocksInfo compressed size")?;
    let ubis = c.u32("blocksInfo uncompressed size")?;
    let flags = c.u32("bundle flags")?;

    let blocks_info_offset = (c.pos + 15) & !15;
    if blocks_info_offset > plain.len() {
        return Err(Error::Truncated {
            what: "blocksInfo header alignment",
            offset: c.pos,
            wanted: blocks_info_offset - c.pos,
            available: plain.len() - c.pos,
        });
    }
    let ci = cbis as usize;
    let ui = ubis as usize;
    let bi_end = blocks_info_offset.checked_add(ci).ok_or(Error::Implausible {
        what: "blocksInfo size",
        value: ci as u64,
    })?;
    if bi_end > plain.len() {
        return Err(Error::Truncated {
            what: "blocksInfo",
            offset: blocks_info_offset,
            wanted: ci,
            available: plain.len() - blocks_info_offset,
        });
    }
    let raw_bi = lz4::decompress(&plain[blocks_info_offset..bi_end], Some(ui))?;

    let mut b = Cursor::new(&raw_bi);
    b.pos = 16; // four hash bytes the format leaves unused
    let block_count = b.u32("block count")? as usize;
    if block_count > 1_000_000 {
        return Err(Error::Implausible { what: "block count", value: block_count as u64 });
    }
    let mut blocks = Vec::with_capacity(block_count.min(4096));
    for _ in 0..block_count {
        let uncompressed = b.u32("block uncompressed size")?;
        let compressed = b.u32("block compressed size")?;
        let flags = b.u16("block flags")?;
        blocks.push(Block { uncompressed, compressed, flags });
    }
    let node_count = b.u32("node count")? as usize;
    if node_count > 1_000_000 {
        return Err(Error::Implausible { what: "node count", value: node_count as u64 });
    }
    let mut nodes = Vec::with_capacity(node_count.min(4096));
    for _ in 0..node_count {
        let offset = b.i64("node offset")?;
        let size = b.i64("node size")?;
        let flags = b.u32("node flags")?;
        let path = b.cstr("node path")?;
        nodes.push(NodeEntry { offset, size, flags, path });
    }

    let info = BundleInfo {
        version,
        engine,
        revision,
        size,
        flags,
        compressed_blocks_info_size: cbis,
        uncompressed_blocks_info_size: ubis,
        blocks_info_offset,
        data_offset: bi_end,
        blocks,
        nodes,
        trailing: plain.len().saturating_sub(size.max(0) as usize),
    };
    info.validate(plain.len())?;
    Ok(info)
}

impl BundleInfo {
    fn validate(&self, file_len: usize) -> Result<()> {
        if self.size < 0 || self.size as usize > file_len {
            return Err(Error::SizeMismatch {
                what: "bundle",
                got: file_len,
                want: self.size.max(0) as usize,
            });
        }
        let total_compressed: u64 = self.blocks.iter().map(|b| b.compressed as u64).sum();
        let end = self.data_offset as u64 + total_compressed;
        if end > file_len as u64 {
            return Err(Error::Truncated {
                what: "data blocks",
                offset: self.data_offset,
                wanted: total_compressed as usize,
                available: file_len.saturating_sub(self.data_offset),
            });
        }
        Ok(())
    }
}

/// Decompress every data block and concatenate them.
pub fn read_stream(plain: &[u8], info: &BundleInfo) -> Result<Vec<u8>> {
    let total: usize = info.blocks.iter().map(|b| b.uncompressed as usize).sum();
    let mut out = Vec::with_capacity(total);
    let mut pos = info.data_offset;
    for (i, block) in info.blocks.iter().enumerate() {
        let c = block.compressed as usize;
        let end = pos.checked_add(c).ok_or(Error::Truncated {
            what: "data block",
            offset: pos,
            wanted: c,
            available: 0,
        })?;
        if end > plain.len() {
            return Err(Error::Truncated {
                what: "data block",
                offset: pos,
                wanted: c,
                available: plain.len() - pos,
            });
        }
        let chunk = &plain[pos..end];
        if block.is_lz4() {
            let decoded = lz4::decompress(chunk, Some(block.uncompressed as usize)).map_err(|e| {
                Error::Invalid {
                    what: "data block",
                    detail: format!("block {i} at {pos}: {e}"),
                }
            })?;
            out.extend_from_slice(&decoded);
        } else {
            out.extend_from_slice(chunk);
        }
        pos = end;
    }
    Ok(out)
}

/// Split the decompressed stream into the bundle's node payloads.
pub fn node_payloads(plain: &[u8], info: &BundleInfo) -> Result<Vec<Node>> {
    let stream = read_stream(plain, info)?;
    let mut out = Vec::with_capacity(info.nodes.len());
    for entry in &info.nodes {
        if entry.offset < 0 || entry.size < 0 {
            return Err(Error::Implausible {
                what: "node range",
                value: entry.offset.max(entry.size) as u64,
            });
        }
        let start = entry.offset as usize;
        let end = start
            .checked_add(entry.size as usize)
            .ok_or(Error::Implausible { what: "node end", value: entry.size as u64 })?;
        if end > stream.len() {
            return Err(Error::Truncated {
                what: "node payload",
                offset: start,
                wanted: entry.size as usize,
                available: stream.len().saturating_sub(start),
            });
        }
        out.push(Node {
            path: entry.path.clone(),
            data: stream[start..end].to_vec(),
            flags: entry.flags,
        });
    }
    Ok(out)
}

/// Rebuild a bundle from node payloads, reusing `template`'s header fields.
///
/// Header length is computed rather than assumed: the engine and revision
/// strings vary, so a fixed offset would silently misplace the size field.
pub fn build(nodes: &[Node], template: &BundleInfo, compress: bool) -> Result<Vec<u8>> {
    let stream: Vec<u8> = nodes.iter().flat_map(|n| n.data.iter().copied()).collect();

    let mut blocks: Vec<Block> = Vec::new();
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut offset = 0usize;
    while offset < stream.len() {
        let end = (offset + BLOCK_SIZE).min(stream.len());
        let raw = &stream[offset..end];
        let mut stored = false;
        if compress && raw.len() > 32 {
            let packed = lz4::compress(raw);
            if packed.len() < raw.len() {
                blocks.push(Block {
                    uncompressed: raw.len() as u32,
                    compressed: packed.len() as u32,
                    flags: 3,
                });
                chunks.push(packed);
                stored = true;
            }
        }
        if !stored {
            blocks.push(Block {
                uncompressed: raw.len() as u32,
                compressed: raw.len() as u32,
                flags: 0,
            });
            chunks.push(raw.to_vec());
        }
        offset = end;
    }

    // Node offsets are relative to the start of the concatenated stream.
    let mut node_offsets = Vec::with_capacity(nodes.len());
    let mut acc: i64 = 0;
    for n in nodes {
        node_offsets.push(acc);
        acc += n.data.len() as i64;
    }

    let mut bi: Vec<u8> = vec![0u8; 16]; // unused hash slot
    bi.extend_from_slice(&(blocks.len() as u32).to_be_bytes());
    for b in &blocks {
        bi.extend_from_slice(&b.uncompressed.to_be_bytes());
        bi.extend_from_slice(&b.compressed.to_be_bytes());
        bi.extend_from_slice(&b.flags.to_be_bytes());
    }
    bi.extend_from_slice(&(nodes.len() as u32).to_be_bytes());
    for (n, off) in nodes.iter().zip(node_offsets.iter()) {
        bi.extend_from_slice(&off.to_be_bytes());
        bi.extend_from_slice(&(n.data.len() as i64).to_be_bytes());
        bi.extend_from_slice(&n.flags.to_be_bytes());
        bi.extend_from_slice(n.path.as_bytes());
        bi.push(0);
    }

    let packed_bi = lz4::compress(&bi);
    let body_len: usize = packed_bi.len() + chunks.iter().map(|c| c.len()).sum::<usize>();

    let mut head: Vec<u8> = Vec::with_capacity(64);
    head.extend_from_slice(b"UnityFS\0");
    head.extend_from_slice(&template.version.to_be_bytes());
    head.extend_from_slice(template.engine.as_bytes());
    head.push(0);
    head.extend_from_slice(template.revision.as_bytes());
    head.push(0);
    let size_pos = head.len();
    head.extend_from_slice(&0i64.to_be_bytes()); // filled in below
    head.extend_from_slice(&(packed_bi.len() as u32).to_be_bytes());
    head.extend_from_slice(&(bi.len() as u32).to_be_bytes());
    head.extend_from_slice(&template.flags.to_be_bytes());
    while head.len() % 16 != 0 {
        head.push(0);
    }

    let total = head.len() + body_len;
    head[size_pos..size_pos + 8].copy_from_slice(&(total as i64).to_be_bytes());

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&head);
    out.extend_from_slice(&packed_bi);
    for c in chunks {
        out.extend_from_slice(&c);
    }
    if out.len() != total {
        return Err(Error::SizeMismatch {
            what: "rebuilt bundle",
            got: out.len(),
            want: total,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystream_is_periodic_and_keyed() {
        let ks = keystream(b"\xa1\x4cm\xa3").unwrap();
        // key = head ^ "Unit" = f4 22 04 d7
        assert_eq!(&ks[0..4], &[0xf4, 0x22, 0x04, 0xd7]);
        // last key byte 0xd7 has its top bit set, so padding is 0xff
        assert_eq!(&ks[4..8], &[0xff; 4]);
        // second half repeats the key with the first byte flipped by 0x02
        assert_eq!(ks[8], 0xf4 ^ 0x02);
        assert_eq!(&ks[9..12], &ks[1..4]);
        assert_eq!(&ks[12..16], &[0xff; 4]);
    }

    #[test]
    fn keystream_uses_zero_padding_when_top_bit_clear() {
        // head such that key[3] = 0x6e ^ 0x74 ... choose head[3] = 0x00 -> 0x74
        let ks = keystream(b"\x00\x00\x00\x00").unwrap();
        assert_eq!(&ks[0..4], b"Unit");
        assert_eq!(&ks[4..8], &[0x00; 4]);
        assert_eq!(ks[8], b'U' ^ 0x02);
        assert_eq!(&ks[12..16], &[0x00; 4]);
    }

    #[test]
    fn keystream_rejects_short_input() {
        assert!(keystream(b"ab").is_err());
    }

    /// NEP2 plaintext always begins with "Unit" because the keystream cancels
    /// the ciphertext's first four bytes back to that literal.
    fn nep2_plaintext(len: usize) -> Vec<u8> {
        let mut plain: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
        plain[0..4].copy_from_slice(b"Unit");
        plain
    }

    #[test]
    fn chosen_cipher_head_survives_round_trip() {
        let plain = nep2_plaintext(100_000);
        // Any four bytes may be chosen as the ciphertext head; the decryption
        // side recovers the key from exactly those bytes.
        let wanted_head = [0x12u8, 0x34, 0x56, 0x78];
        let cipher = encrypt(&plain, &wanted_head).unwrap();

        assert_eq!(&cipher[..4], &wanted_head, "cipher head must be what we asked for");
        assert_ne!(&cipher[..ENC_PREFIX], &plain[..ENC_PREFIX], "prefix must change");
        assert_eq!(&cipher[ENC_PREFIX..], &plain[ENC_PREFIX..], "suffix must stay plaintext");
        assert_eq!(decrypt(&cipher).unwrap(), plain);
    }

    #[test]
    fn plaintext_head_is_unit_after_decrypt() {
        // Whatever the cipher head is, decryption yields "Unit" at offset 0.
        let plain = nep2_plaintext(64);
        let cipher = encrypt(&plain, &[0xde, 0xad, 0xbe, 0xef]).unwrap();
        let back = decrypt(&cipher).unwrap();
        assert_eq!(&back[..4], b"Unit");
    }

    #[test]
    fn short_bundle_prefix_round_trips() {
        let plain = nep2_plaintext(100);
        let cipher = encrypt(&plain, &[0x01, 0x02, 0x03, 0x04]).unwrap();
        assert_eq!(cipher.len(), plain.len());
        assert_eq!(decrypt(&cipher).unwrap(), plain);
    }

    #[test]
    fn encrypting_with_plaintext_head_gives_a_zero_key() {
        // Documenting the footgun that motivates taking a *cipher* head.
        // Plaintext head is "Unit", so X = "Unit" ^ "Unit" = 0 and the padding
        // is zero too. Only the byte at stream index 8 survives (it is
        // key[0] ^ 0x02), so the prefix comes out essentially unchanged.
        let plain = nep2_plaintext(64);
        let broken = encrypt(&plain, &plain[..4]).unwrap();
        assert_eq!(&broken[..4], b"Unit", "prefix stays plaintext-looking");

        // The damage is not a failed round-trip -- it is that the file reads as
        // plaintext. The game decrypts with the same key and then checks the
        // signature, which is now whatever the payload began with rather than
        // "UnityFS\0".
        let as_game_would_see_it = decrypt(&broken).unwrap();
        assert_ne!(&as_game_would_see_it[..8], b"UnityFS\0");
    }

    #[test]
    fn concatenated_bundle_prefix_is_readable_but_flagged() {
        let template = BundleInfo {
            version: 7,
            engine: "5.x.x".into(),
            revision: "2019.4.29f1".into(),
            size: 0,
            flags: 0,
            compressed_blocks_info_size: 0,
            uncompressed_blocks_info_size: 0,
            blocks_info_offset: 0,
            data_offset: 0,
            blocks: Vec::new(),
            nodes: Vec::new(),
            trailing: 0,
        };
        let nodes = vec![Node {
            path: "CAB-test".into(),
            data: vec![7u8; 64],
            flags: 4,
        }];
        let plain = build(&nodes, &template, false).expect("build");
        let info = parse(&plain).expect("parse");
        assert_eq!(info.trailing, 0);

        // A second UnityFS archive concatenated after the first: the reader
        // uses the prefix and reports the leftover bytes.
        let mut joined = plain.clone();
        joined.extend_from_slice(b"yFS\0second");
        let info = parse(&joined).expect("parse joined");
        assert_eq!(info.trailing, 10);
        assert_eq!(info.size as usize, plain.len());
        let recovered = node_payloads(&joined, &info).expect("nodes");
        assert_eq!(recovered[0].data, vec![7u8; 64]);
    }

    #[test]
    fn parse_rejects_non_unityfs() {
        assert!(matches!(
            parse(b"NOPE\0\0\0\0"),
            Err(Error::BadSignature { .. })
        ));
        assert!(parse(b"UnityF").is_err());
    }
}
