//! Raw DEFLATE decoder (RFC 1951), std-only.
//!
//! `.exmtmesh` is an NPZ archive: a ZIP whose entries are raw DEFLATE streams.
//! Reading one therefore needs an inflater, and this is it: stored, fixed and
//! dynamic Huffman blocks, with the same length/distance tables the encoder in
//! the Python core relies on.
//!
//! The decoder is deliberately defensive: Unity files and Blender exports are
//! untrusted input, so every code, repeat count and output length is bounded,
//! and the caller supplies a hard output limit.

use crate::error::{Error, Result};

const MAX_BITS: usize = 15;

/// Inflate a raw DEFLATE stream, refusing to produce more than `max_output`.
pub fn inflate(src: &[u8], max_output: usize) -> Result<Vec<u8>> {
    let mut bits = Bits::new(src);
    let mut out = Vec::<u8>::new();
    loop {
        let last = bits.bit()? == 1;
        match bits.take(2)? {
            0 => stored(&mut bits, &mut out, max_output)?,
            1 => {
                let lit = Huffman::new(&FIXED_LIT_LENGTHS, "fixed literal tree")?;
                let dist = Huffman::new(&[5u8; 32], "fixed distance tree")?;
                block(&mut bits, &mut out, &lit, &dist, max_output)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut bits)?;
                block(&mut bits, &mut out, &lit, &dist, max_output)?;
            }
            _ => {
                return Err(Error::Invalid {
                    what: "deflate block",
                    detail: "reserved block type".into(),
                })
            }
        }
        if last {
            break;
        }
    }
    Ok(out)
}

struct Bits<'a> {
    data: &'a [u8],
    /// Bit position from the start of the stream.
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    /// One bit, least-significant-first as DEFLATE packs raw bits.
    fn bit(&mut self) -> Result<u8> {
        let index = self.pos / 8;
        if index >= self.data.len() {
            return Err(Error::Truncated {
                what: "deflate bits",
                offset: self.pos,
                wanted: 1,
                available: 0,
            });
        }
        let bit = (self.data[index] >> (self.pos % 8)) & 1;
        self.pos += 1;
        Ok(bit)
    }

    /// `n` raw bits, little-endian value (extra bits, header fields).
    fn take(&mut self, n: u8) -> Result<u32> {
        let mut value = 0u32;
        for i in 0..n {
            value |= (self.bit()? as u32) << i;
        }
        Ok(value)
    }

    fn align(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(8)? as u8)
    }
}

fn stored(bits: &mut Bits<'_>, out: &mut Vec<u8>, max_output: usize) -> Result<()> {
    bits.align();
    let len = bits.byte()? as usize | ((bits.byte()? as usize) << 8);
    let nlen = bits.byte()? as usize | ((bits.byte()? as usize) << 8);
    if nlen != (!len & 0xFFFF) {
        return Err(Error::Invalid {
            what: "deflate stored block",
            detail: format!("length {len} does not match complement {nlen}"),
        });
    }
    if out.len() + len > max_output {
        return Err(Error::Implausible {
            what: "deflate output",
            value: (out.len() + len) as u64,
        });
    }
    for _ in 0..len {
        out.push(bits.byte()?);
    }
    Ok(())
}

fn dynamic_tables(bits: &mut Bits<'_>) -> Result<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = bits.take(5)? as usize + 257;
    let hdist = bits.take(5)? as usize + 1;
    let hclen = bits.take(4)? as usize + 4;
    if hlit > 286 || hdist > 32 {
        return Err(Error::Implausible {
            what: "deflate code counts",
            value: (hlit.max(hdist)) as u64,
        });
    }

    let mut cl_lengths = [0u8; 19];
    for &slot in ORDER.iter().take(hclen) {
        cl_lengths[slot] = bits.take(3)? as u8;
    }
    let cl = Huffman::new(&cl_lengths, "code-length tree")?;

    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let symbol = cl.decode(bits)?;
        match symbol {
            0..=15 => lengths.push(symbol as u8),
            16 => {
                let previous = *lengths.last().ok_or(Error::Invalid {
                    what: "deflate repeat",
                    detail: "code 16 with no previous length".into(),
                })?;
                let count = 3 + bits.take(2)? as usize;
                if lengths.len() + count > total {
                    return Err(Error::Implausible {
                        what: "deflate repeat",
                        value: count as u64,
                    });
                }
                for _ in 0..count {
                    lengths.push(previous);
                }
            }
            17 => {
                let count = 3 + bits.take(3)? as usize;
                if lengths.len() + count > total {
                    return Err(Error::Implausible {
                        what: "deflate repeat",
                        value: count as u64,
                    });
                }
                lengths.extend(std::iter::repeat_n(0u8, count));
            }
            18 => {
                let count = 11 + bits.take(7)? as usize;
                if lengths.len() + count > total {
                    return Err(Error::Implausible {
                        what: "deflate repeat",
                        value: count as u64,
                    });
                }
                lengths.extend(std::iter::repeat_n(0u8, count));
            }
            _ => {
                return Err(Error::Invalid {
                    what: "deflate code length",
                    detail: symbol.to_string(),
                })
            }
        }
    }

    let lit = Huffman::new(&lengths[..hlit], "literal tree")?;
    let dist_lengths = &lengths[hlit..];
    // A single distance code of length 0 means "no matches"; keep an empty tree
    // and let `block` reject an actual match rather than failing up front.
    let dist = Huffman::new(dist_lengths, "distance tree")?;
    Ok((lit, dist))
}

fn block(
    bits: &mut Bits<'_>,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    max_output: usize,
) -> Result<()> {
    loop {
        let symbol = lit.decode(bits)?;
        match symbol {
            0..=255 => {
                if out.len() + 1 > max_output {
                    return Err(Error::Implausible {
                        what: "deflate output",
                        value: out.len() as u64,
                    });
                }
                out.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let (base, extra) = LENGTHS[(symbol - 257) as usize];
                let length = base as usize + bits.take(extra)? as usize;
                let distance_symbol = dist.decode(bits)?;
                if distance_symbol > 29 {
                    return Err(Error::Invalid {
                        what: "deflate distance",
                        detail: format!("reserved code {distance_symbol}"),
                    });
                }
                let (dbase, dextra) = DISTANCES[distance_symbol as usize];
                let distance = dbase as usize + bits.take(dextra)? as usize;
                if distance == 0 || distance > out.len() {
                    return Err(Error::Invalid {
                        what: "deflate distance",
                        detail: format!("{distance} with {} bytes produced", out.len()),
                    });
                }
                if out.len() + length > max_output {
                    return Err(Error::Implausible {
                        what: "deflate output",
                        value: (out.len() + length) as u64,
                    });
                }
                let start = out.len() - distance;
                for i in 0..length {
                    let byte = out[start + i];
                    out.push(byte);
                }
            }
            _ => {
                return Err(Error::Invalid {
                    what: "deflate literal",
                    detail: format!("reserved code {symbol}"),
                })
            }
        }
    }
}

/// Canonical Huffman decoder: counts/first-code/symbol table per length.
struct Huffman {
    counts: [u16; MAX_BITS + 1],
    first: [u16; MAX_BITS + 1],
    offsets: [u16; MAX_BITS + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8], what: &'static str) -> Result<Self> {
        let mut counts = [0u16; MAX_BITS + 1];
        for &len in lengths {
            if len as usize > MAX_BITS {
                return Err(Error::Invalid {
                    what,
                    detail: format!("code length {len}"),
                });
            }
            if len > 0 {
                counts[len as usize] += 1;
            }
        }
        let mut offsets = [0u16; MAX_BITS + 1];
        let mut total = 0u16;
        for len in 1..=MAX_BITS {
            offsets[len] = total;
            total += counts[len];
        }
        let mut symbols = vec![0u16; total as usize];
        let mut next = offsets;
        for (symbol, &len) in lengths.iter().enumerate() {
            if len > 0 {
                symbols[next[len as usize] as usize] = symbol as u16;
                next[len as usize] += 1;
            }
        }
        let mut first = [0u16; MAX_BITS + 1];
        let mut code = 0u16;
        for len in 1..=MAX_BITS {
            first[len] = code;
            code = (code + counts[len]) << 1;
        }
        Ok(Huffman {
            counts,
            first,
            offsets,
            symbols,
        })
    }

    fn decode(&self, bits: &mut Bits<'_>) -> Result<u16> {
        let mut code = 0u32;
        for len in 1..=MAX_BITS {
            code = (code << 1) | bits.bit()? as u32;
            let count = self.counts[len] as u32;
            if count == 0 {
                continue;
            }
            let first = self.first[len] as u32;
            if code >= first && code < first + count {
                let index = self.offsets[len] as usize + (code - first) as usize;
                return Ok(self.symbols[index]);
            }
        }
        Err(Error::Invalid {
            what: "deflate huffman code",
            detail: "no match in 15 bits".into(),
        })
    }
}

/// Length codes 257..=285: (base, extra bits).
const LENGTHS: [(u16, u8); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// Distance codes 0..=29: (base, extra bits).
const DISTANCES: [(u16, u8); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// Literal/length code lengths for fixed Huffman blocks.
const FIXED_LIT_LENGTHS: [u8; 288] = fixed_lit_lengths();

const fn fixed_lit_lengths() -> [u8; 288] {
    let mut lengths = [8u8; 288];
    let mut i = 144;
    while i < 256 {
        lengths[i] = 9;
        i += 1;
    }
    let mut i = 256;
    while i < 280 {
        lengths[i] = 7;
        i += 1;
    }
    lengths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_python_zlib_vectors() {
        // Produced with `zlib.compressobj(6, DEFLATED, -15)`.
        let vectors: [(&str, &str); 5] = [
            ("cb48cdc9c95728cf2fca49c9c0ca0400", "hello worldhello worldhello world"),
            ("edc1010d000000c2a0f74f6d0f0714000000f06e", "zeros"),
            (
                "edca470180301045412b5f016a628092d0d910084d3d88e0f8ce33aef35a735f8faa929d8b825d1af21c37d9e193f68fa7f2b9d5585bc891c96432994c2693c96432994c2693ffc82f",
                "text",
            ),
            (
                "6360646266616563e7e0e4e2e6e1e5e3171014121611151397909492969195935750545256515553d7d0d4d2d6d1d5d33730343236313533b7b0b4b2b6b1b5b30700",
                "bytes",
            ),
            (
                "6360646266616563e7e0e4e2e6e1e5e3171014121611151397909492969195935750545256515553d7d0d4d2d6d1d5d33730343236313533b7b0b4b2b6b1b5b37770747276717573f7f0f4f2f6f1f5f30f080c0a0e090d0b8f888c8a8e898d8b4f484c4a4e494d4bcfc8cccacec9cdcb2f282c2a2e292d2bafa8acaaaea9adab6f686c6a6e696d6befe8eceaeee9edeb9f3071d2e42953a74d9f3173d6ec3973e7cd5fb070d1e2254b972d5fb172d5ea356bd7addfb071d3e62d5bb76ddfb173d7ee3d7bf7ed3f70f0d0e123478f1d3f71f2d4e93367cf9dbf70f1d2e52b57af5dbf71f3d6ed3b77efdd7ff0f0d1e3274f9f3d7ff1f2d5eb376fdfbdfff0f1d3e72f5fbf7dfff1f317c3a8d747bd3eeaf551af8f7a7dd4eba35e1ff5faa8d747bd3e54bc0e00",
                "mixed",
            ),
        ];
        for (compressed, expected) in vectors {
            let got = inflate(&unhex(compressed), 1 << 20).unwrap();
            let want = match expected {
                "zeros" => vec![0u8; 4096],
                "text" => b"The quick brown fox jumps over the lazy dog. ".repeat(100),
                "bytes" => (0..=63u8).collect(),
                "mixed" => (0..3000).map(|i| (i % 251) as u8).collect(),
                other => other.as_bytes().to_vec(),
            };
            assert_eq!(got, want, "vector {expected}");
        }
    }

    #[test]
    fn output_limit_is_enforced() {
        let zeros = unhex("edc1010d000000c2a0f74f6d0f0714000000f06e");
        assert!(inflate(&zeros, 100).is_err());
    }

    #[test]
    fn truncated_stream_is_an_error() {
        assert!(inflate(&[0x03], 16).is_err());
        assert!(inflate(&[], 16).is_err());
    }

    #[test]
    fn empty_stream_is_rejected() {
        // A valid empty final block: BFINAL=1, BTYPE=01, end-of-block code.
        let empty = [0x03u8, 0x00];
        assert_eq!(inflate(&empty, 8).unwrap(), Vec::<u8>::new());
    }
}
