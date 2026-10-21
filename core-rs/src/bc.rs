//! Block-compressed texture decoding: BC1, BC3, BC4, BC5 and BC7 to BGRA8.
//!
//! This is a port of `texture2ddecoder` 1.0.6 (MIT, Copyright (c) 2020 K0lb3;
//! its BC code in turn comes from AssetStudio), the decoder the retired script
//! core used. It is ported rather than rewritten from the D3D spec on purpose:
//! the acceptance values are that decoder's pixels, so replacing the script
//! core did not change what the previewer shows. Two consequences are visible
//! and pinned by tests:
//!
//! * BC1's three-colour mode yields **opaque black**, not transparent black.
//!   The spec says alpha 0; this decoder writes `(0, 0, 0, 255)` and therefore
//!   so does this module.
//! * BC3's colour half still honours `c0 > c1` even though BC3 is defined as a
//!   four-colour block. Same reason.
//!
//! Output is **BGRA, packed the way the file stores it**: the first row is the
//! bottom row of the picture. [`crate::texture`] applies the flip and channel
//! swap that turn this into an upright RGBA image.
//!
//! Every function is total over its input slice but returns an error for a
//! truncated payload; the decoders themselves accept any bit pattern.

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bc {
    Bc1,
    Bc3,
    Bc4,
    Bc5,
    Bc7,
}

impl Bc {
    pub fn block_bytes(self) -> usize {
        match self {
            Bc::Bc1 | Bc::Bc4 => 8,
            Bc::Bc3 | Bc::Bc5 | Bc::Bc7 => 16,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Bc::Bc1 => "BC1",
            Bc::Bc3 => "BC3",
            Bc::Bc4 => "BC4",
            Bc::Bc5 => "BC5",
            Bc::Bc7 => "BC7",
        }
    }
}

/// Bytes a block-compressed image occupies, including every mip level.
pub fn image_bytes(bc: Bc, width: usize, height: usize) -> usize {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    bw * bh * bc.block_bytes()
}

/// Decode a full image. The result is `width * height * 4` BGRA bytes.
pub fn decode(bc: Bc, src: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || width > 65_536 || height > 65_536 {
        return Err(Error::Invalid {
            what: "bc image size",
            detail: format!("{width}x{height}"),
        });
    }
    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    let need = blocks_x * blocks_y * bc.block_bytes();
    if src.len() < need {
        return Err(Error::Truncated {
            what: bc.name(),
            offset: 0,
            wanted: need,
            available: src.len(),
        });
    }

    let mut out = vec![0u8; width * height * 4];
    let stride = bc.block_bytes();
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let at = (by * blocks_x + bx) * stride;
            let block = decode_block(bc, &src[at..at + stride])?;
            for y in 0..4 {
                let py = by * 4 + y;
                if py >= height {
                    break;
                }
                for x in 0..4 {
                    let px = bx * 4 + x;
                    if px >= width {
                        break;
                    }
                    let src_at = (y * 4 + x) * 4;
                    let dst = (py * width + px) * 4;
                    out[dst..dst + 4].copy_from_slice(&block[src_at..src_at + 4]);
                }
            }
        }
    }
    Ok(out)
}

/// Decode one 4x4 block into 16 BGRA pixels, row-major.
pub fn decode_block(bc: Bc, block: &[u8]) -> Result<[u8; 64]> {
    let need = bc.block_bytes();
    if block.len() < need {
        return Err(Error::Truncated {
            what: bc.name(),
            offset: 0,
            wanted: need,
            available: block.len(),
        });
    }
    Ok(match bc {
        Bc::Bc1 => bc1_block(block),
        Bc::Bc3 => bc3_block(block),
        Bc::Bc4 => {
            let mut out = [[0u8, 0, 0, 255]; 16];
            write_alpha(&mut out, block, 2);
            flatten(&out)
        }
        Bc::Bc5 => {
            let mut out = [[0u8, 0, 0, 255]; 16];
            write_alpha(&mut out, block, 2);
            write_alpha(&mut out, &block[8..], 1);
            flatten(&out)
        }
        Bc::Bc7 => bc7_block(block),
    })
}

fn flatten(px: &[[u8; 4]; 16]) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (i, p) in px.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(p);
    }
    out
}

/// Little-endian BGRA pixel, matching the decoder this module ports.
fn color(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    [b, g, r, a]
}

/// Expand a 16-bit 5-6-5 colour to 8 bits per channel by bit replication.
fn rgb565(d: u16) -> (u8, u8, u8) {
    (
        (((d >> 8) & 0xF8) | (d >> 13)) as u8,
        (((d >> 3) & 0xFC) | ((d >> 9) & 0x03)) as u8,
        (((d << 3) | ((d >> 2) & 0x07)) as u8),
    )
}

fn bc1_block(src: &[u8]) -> [u8; 64] {
    let q0 = u16::from_le_bytes([src[0], src[1]]);
    let q1 = u16::from_le_bytes([src[2], src[3]]);
    let (r0, g0, b0) = rgb565(q0);
    let (r1, g1, b1) = rgb565(q1);

    let mut palette = [color(0, 0, 0, 255); 4];
    palette[0] = color(r0, g0, b0, 255);
    palette[1] = color(r1, g1, b1, 255);
    let mix = |a: u8, b: u8, wa: u32, wb: u32, div: u32| -> u8 {
        ((a as u32 * wa + b as u32 * wb) / div) as u8
    };
    if q0 > q1 {
        palette[2] = color(
            mix(r0, r1, 2, 1, 3),
            mix(g0, g1, 2, 1, 3),
            mix(b0, b1, 2, 1, 3),
            255,
        );
        palette[3] = color(
            mix(r0, r1, 1, 2, 3),
            mix(g0, g1, 1, 2, 3),
            mix(b0, b1, 1, 2, 3),
            255,
        );
    } else {
        palette[2] = color(
            ((r0 as u32 + r1 as u32) / 2) as u8,
            ((g0 as u32 + g1 as u32) / 2) as u8,
            ((b0 as u32 + b1 as u32) / 2) as u8,
            255,
        );
        // The oracle writes opaque black here, not transparent black.
        palette[3] = color(0, 0, 0, 255);
    }

    let mut indices = u32::from_le_bytes([src[4], src[5], src[6], src[7]]);
    let mut out = [[0u8; 4]; 16];
    for px in out.iter_mut() {
        *px = palette[(indices & 3) as usize];
        indices >>= 2;
    }
    flatten(&out)
}

/// Expand the eight-alpha palette BC3/BC4/BC5 share.
fn alpha_palette(a0: u8, a1: u8) -> [u8; 8] {
    let mut a = [0u8; 8];
    a[0] = a0;
    a[1] = a1;
    if a0 > a1 {
        a[2] = ((a0 as u32 * 6 + a1 as u32) / 7) as u8;
        a[3] = ((a0 as u32 * 5 + a1 as u32 * 2) / 7) as u8;
        a[4] = ((a0 as u32 * 4 + a1 as u32 * 3) / 7) as u8;
        a[5] = ((a0 as u32 * 3 + a1 as u32 * 4) / 7) as u8;
        a[6] = ((a0 as u32 * 2 + a1 as u32 * 5) / 7) as u8;
        a[7] = ((a0 as u32 + a1 as u32 * 6) / 7) as u8;
    } else {
        a[2] = ((a0 as u32 * 4 + a1 as u32) / 5) as u8;
        a[3] = ((a0 as u32 * 3 + a1 as u32 * 2) / 5) as u8;
        a[4] = ((a0 as u32 * 2 + a1 as u32 * 3) / 5) as u8;
        a[5] = ((a0 as u32 + a1 as u32 * 4) / 5) as u8;
        a[6] = 0;
        a[7] = 255;
    }
    a
}

/// Write one interpolated channel of a BC3/BC4/BC5 block into `channel`
/// (0 = B, 1 = G, 2 = R, 3 = A in this module's BGRA layout).
fn write_alpha(out: &mut [[u8; 4]; 16], src: &[u8], channel: usize) {
    let palette = alpha_palette(src[0], src[1]);
    let mut indices = u64::from_le_bytes([
        src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
    ]) >> 16;
    for px in out.iter_mut() {
        px[channel] = palette[(indices & 7) as usize];
        indices >>= 3;
    }
}

fn bc3_block(src: &[u8]) -> [u8; 64] {
    let mut out = bc1_block(&src[8..16]);
    let palette = alpha_palette(src[0], src[1]);
    let mut indices = u64::from_le_bytes([
        src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
    ]) >> 16;
    for i in 0..16 {
        out[i * 4 + 3] = palette[(indices & 7) as usize];
        indices >>= 3;
    }
    out
}

fn bc7_block(src: &[u8]) -> [u8; 64] {
    let mut bits = Bits::new(src);
    let mut mode = 0usize;
    while mode < 8 && bits.read(1) == 0 {
        mode += 1;
    }
    if mode == 8 {
        // No mode bit inside the first eight: the oracle clears the block.
        return [0u8; 64];
    }
    let mi = &tables::MODES[mode];
    let pbits = if mi.endpoint_pbits != 0 {
        mi.endpoint_pbits
    } else {
        mi.shared_pbits
    };

    let partition = bits.read(mi.partition_bits) as usize;
    let rotation = bits.read(mi.rotation_bits);
    let index_selection = bits.read(mi.index_selection_bits) as usize;

    let mut ep = [[0u8; 4]; 6]; // R, G, B, A per endpoint
    for channel in 0..3 {
        for i in 0..mi.num_subsets * 2 {
            let raw = bits.read(mi.color_bits) << pbits;
            ep[i][channel] = raw as u8;
        }
    }
    if mi.alpha_bits != 0 {
        for i in 0..mi.num_subsets * 2 {
            let raw = bits.read(mi.alpha_bits) << pbits;
            ep[i][3] = raw as u8;
        }
    } else {
        for i in 0..6 {
            ep[i][3] = 255;
        }
    }

    if pbits != 0 {
        for i in 0..mi.num_subsets {
            let pda = bits.read(pbits) as u8;
            let pdb = if mi.shared_pbits == 0 {
                bits.read(pbits) as u8
            } else {
                pda
            };
            ep[i * 2][0] |= pda;
            ep[i * 2 + 1][0] |= pdb;
            ep[i * 2][1] |= pda;
            ep[i * 2 + 1][1] |= pdb;
            ep[i * 2][2] |= pda;
            ep[i * 2 + 1][2] |= pdb;
            ep[i * 2][3] |= pda;
            ep[i * 2 + 1][3] |= pdb;
        }
    }

    let color_bits = mi.color_bits + pbits;
    for i in 0..mi.num_subsets * 2 {
        for channel in 0..3 {
            ep[i][channel] = expand_quantized(ep[i][channel], color_bits);
        }
    }
    if mi.alpha_bits != 0 {
        let alpha_bits = mi.alpha_bits + pbits;
        for i in 0..mi.num_subsets * 2 {
            ep[i][3] = expand_quantized(ep[i][3], alpha_bits);
        }
    }

    let has_secondary = mi.index_bits[1] != 0;
    let primary = &tables::FACTORS[(mi.index_bits[0] - 2) as usize];
    let secondary = if has_secondary {
        &tables::FACTORS[(mi.index_bits[1] - 2) as usize]
    } else {
        primary
    };

    // Index planes are not interleaved: the secondary plane starts after the
    // whole primary plane, minus one anchor bit per subset.
    let mut offset = [0u16; 2];
    offset[1] = (mi.num_subsets as u16) * (16 * mi.index_bits[0] as u16 - 1);
    let plane_start = bits.pos;

    let mut out = [[0u8; 4]; 16];
    for idx in 0..16usize {
        let (subset, index_anchor) = match mi.num_subsets {
            2 => {
                let s = ((tables::P2[partition] >> idx) & 1) as usize;
                let anchor = if s != 0 {
                    tables::A2[partition] as usize
                } else {
                    0
                };
                (s, anchor)
            }
            3 => {
                let s = ((tables::P3[partition] >> (2 * idx)) & 3) as usize;
                let anchor = if s != 0 {
                    tables::A3[s - 1][partition] as usize
                } else {
                    0
                };
                (s, anchor)
            }
            _ => (0, 0),
        };
        let anchor = idx == index_anchor;
        let num = [
            mi.index_bits[0] - anchor as u8,
            if has_secondary {
                mi.index_bits[1] - anchor as u8
            } else {
                0
            },
        ];
        let i0 = bits.peek(plane_start + offset[0], num[0]);
        let i1 = if has_secondary {
            bits.peek(plane_start + offset[1], num[1])
        } else {
            i0
        };
        offset[0] += num[0] as u16;
        offset[1] += num[1] as u16;

        let pick = [i0, i1];
        // The selection bit chooses both the index plane and the weights table.
        let planes = [primary, secondary];
        let fc = planes[index_selection][pick[index_selection] as usize];
        let fa = planes[1 - index_selection][pick[1 - index_selection] as usize];
        let (fca, faa) = (64 - fc, 64 - fa);

        let a = &ep[subset * 2];
        let b = &ep[subset * 2 + 1];
        let mut r = ((a[0] as u32 * fca as u32 + b[0] as u32 * fc as u32 + 32) >> 6) as u8;
        let mut g = ((a[1] as u32 * fca as u32 + b[1] as u32 * fc as u32 + 32) >> 6) as u8;
        let mut bl = ((a[2] as u32 * fca as u32 + b[2] as u32 * fc as u32 + 32) >> 6) as u8;
        let mut al = ((a[3] as u32 * faa as u32 + b[3] as u32 * fa as u32 + 32) >> 6) as u8;

        match rotation {
            1 => std::mem::swap(&mut al, &mut r),
            2 => std::mem::swap(&mut al, &mut g),
            3 => std::mem::swap(&mut al, &mut bl),
            _ => {}
        }
        out[idx] = color(r, g, bl, al);
    }
    flatten(&out)
}

fn expand_quantized(v: u8, bits: u8) -> u8 {
    if bits >= 8 {
        // Eight-bit endpoints (mode 6) are already full range.
        return v;
    }
    let v = v << (8 - bits);
    v | (v >> bits)
}

/// Little-endian bit reader over the 16-byte BC7 block.
struct Bits {
    data: u128,
    pos: u16,
}

impl Bits {
    fn new(src: &[u8]) -> Self {
        let mut raw = [0u8; 16];
        raw.copy_from_slice(&src[..16]);
        Bits {
            data: u128::from_le_bytes(raw),
            pos: 0,
        }
    }

    fn read(&mut self, n: u8) -> u16 {
        let v = self.peek(self.pos, n) as u16;
        self.pos += n as u16;
        v
    }

    fn peek(&self, bit: u16, n: u8) -> u8 {
        if n == 0 {
            return 0;
        }
        let mask = (1u128 << n) - 1;
        ((self.data >> bit) & mask) as u8
    }
}

mod tables {
    //! BC7 partition tables and interpolation weights.
    //!
    //! Generated from `texture2ddecoder` 1.0.6's `bcn.cpp` (MIT, Copyright (c)
    //! 2020 K0lb3; the BC7 tables originate in AssetStudio) rather than
    //! retyped, so a transcription slip cannot hide here.
    pub(crate) struct ModeInfo {
        pub num_subsets: usize,
        pub partition_bits: u8,
        pub rotation_bits: u8,
        pub index_selection_bits: u8,
        pub color_bits: u8,
        pub alpha_bits: u8,
        pub endpoint_pbits: u8,
        pub shared_pbits: u8,
        pub index_bits: [u8; 2],
    }

    pub(crate) static MODES: [ModeInfo; 8] = [
        ModeInfo {
            num_subsets: 3,
            partition_bits: 4,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 4,
            alpha_bits: 0,
            endpoint_pbits: 1,
            shared_pbits: 0,
            index_bits: [3, 0],
        },
        ModeInfo {
            num_subsets: 2,
            partition_bits: 6,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 6,
            alpha_bits: 0,
            endpoint_pbits: 0,
            shared_pbits: 1,
            index_bits: [3, 0],
        },
        ModeInfo {
            num_subsets: 3,
            partition_bits: 6,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 5,
            alpha_bits: 0,
            endpoint_pbits: 0,
            shared_pbits: 0,
            index_bits: [2, 0],
        },
        ModeInfo {
            num_subsets: 2,
            partition_bits: 6,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 7,
            alpha_bits: 0,
            endpoint_pbits: 1,
            shared_pbits: 0,
            index_bits: [2, 0],
        },
        ModeInfo {
            num_subsets: 1,
            partition_bits: 0,
            rotation_bits: 2,
            index_selection_bits: 1,
            color_bits: 5,
            alpha_bits: 6,
            endpoint_pbits: 0,
            shared_pbits: 0,
            index_bits: [2, 3],
        },
        ModeInfo {
            num_subsets: 1,
            partition_bits: 0,
            rotation_bits: 2,
            index_selection_bits: 0,
            color_bits: 7,
            alpha_bits: 8,
            endpoint_pbits: 0,
            shared_pbits: 0,
            index_bits: [2, 2],
        },
        ModeInfo {
            num_subsets: 1,
            partition_bits: 0,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 7,
            alpha_bits: 7,
            endpoint_pbits: 1,
            shared_pbits: 0,
            index_bits: [4, 0],
        },
        ModeInfo {
            num_subsets: 2,
            partition_bits: 6,
            rotation_bits: 0,
            index_selection_bits: 0,
            color_bits: 5,
            alpha_bits: 5,
            endpoint_pbits: 1,
            shared_pbits: 0,
            index_bits: [2, 0],
        },
    ];

    pub(crate) static FACTORS: [[u8; 16]; 3] = [
        [0, 21, 43, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 9, 18, 27, 37, 46, 55, 64, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64],
    ];

    /// Two-subset partition masks, bit `i` = subset of texel `i`.
    pub(crate) static P2: [u16; 64] = [
        0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80,
        0xe800, 0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310,
        0x3100, 0x8cce, 0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa,
        0xf0f0, 0x5a5a, 0x33cc, 0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc,
        0x6996, 0xc33c, 0x9966, 0x0660, 0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6,
        0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718, 0xccf0, 0x0fcc, 0x7744, 0xee22,
    ];

    /// Anchor texel of subset 1 for each two-subset partition.
    pub(crate) static A2: [u8; 64] = [
        15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15,
        2, 8, 2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8,
        15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15,
    ];

    /// Three-subset partition masks, two bits per texel.
    pub(crate) static P3: [u32; 64] = [
        0xaa685050, 0x6a5a5040, 0x5a5a4200, 0x5450a0a8, 0xa5a50000, 0xa0a05050, 0x5555a0a0,
        0x5a5a5050, 0xaa550000, 0xaa555500, 0xaaaa5500, 0x90909090, 0x94949494, 0xa4a4a4a4,
        0xa9a59450, 0x2a0a4250, 0xa5945040, 0x0a425054, 0xa5a5a500, 0x55a0a0a0, 0xa8a85454,
        0x6a6a4040, 0xa4a45000, 0x1a1a0500, 0x0050a4a4, 0xaaa59090, 0x14696914, 0x69691400,
        0xa08585a0, 0xaa821414, 0x50a4a450, 0x6a5a0200, 0xa9a58000, 0x5090a0a8, 0xa8a09050,
        0x24242424, 0x00aa5500, 0x24924924, 0x24499224, 0x50a50a50, 0x500aa550, 0xaaaa4444,
        0x66660000, 0xa5a0a5a0, 0x50a050a0, 0x69286928, 0x44aaaa44, 0x66666600, 0xaa444444,
        0x54a854a8, 0x95809580, 0x96969600, 0xa85454a8, 0x80959580, 0xaa141414, 0x96960000,
        0xaaaa1414, 0xa05050a0, 0xa0a5a5a0, 0x96000000, 0x40804080, 0xa9a8a9a8, 0xaaaaaa44,
        0x2a4a5254,
    ];

    /// Anchor texel per subset (rows 1 and 2) for each three-subset partition.
    pub(crate) static A3: [[u8; 64]; 2] = [
        [
            3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8,
            6, 8, 5, 15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5,
            5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3,
        ],
        [
            15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8,
            3, 15, 6, 10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15,
            3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
        ],
    ];
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

    /// Expected pixels come from `texture2ddecoder` 1.0.6, the decoder the
    /// Python core uses. The blocks are deliberately arbitrary: for block
    /// compression any byte pattern is valid input, and the point is that both
    /// implementations agree on it.
    const CORPUS: &[(Bc, &str, &str)] = &[
        // fmt1#0
        (Bc::Bc1, "0af58f2a1df4f9e91e4a28f6c95a0284", "7b5129ff6d6c6dff7b5129ff52a2f7ff52a2f7ff7b5129ff6d6c6dff6d6c6dff7b5129ff5f87b2ff6d6c6dff6d6c6dff7b5129ff5f87b2ff5f87b2ff6d6c6dff"),
        // fmt1#1
        (Bc::Bc1, "1c3dc34bdc135beda23936fd8dcc2086", "e7a239ff000000ff18794aff000000ff000000ffe7a239ff18794affe7a239ff000000ff7f8d41ff18794aff18794aff18794aff000000ff7f8d41ff000000ff"),
        // fmt1#2
        (Bc::Bc1, "2e86f86d9c32bdf126294304503e3f88", "73c784ffaac173ffc6be6bff8ec47bff8ec47bff73c784ffaac173ff73c784ffc6be6bffaac173ffaac173ff8ec47bffc6be6bff73c784ffaac173ffaac173ff"),
        // fmt3#0
        (Bc::Bc3, "d018850f47f3c79a6d4277bd6a513fff", "947d7f66947d7fd0947d7f4cbdaebd32bdaebdd06b4d424cbdaebd18bdaebdb50000009b0000004c000000326b4d429b00000081000000660000004c00000081"),
        // fmt3#1
        (Bc::Bc3, "e261ba310712299ef03285c42dc35d01", "2992c6cf0000007356777b85845d31e2000000bd845d3185845d3161000000e22992c6cf000000cf2992c6aa2992c6aa2992c6cf845d31aa845d3173845d31aa"),
        // fmt3#2
        (Bc::Bc3, "f3a9ee53c6318ba1742193cbf1357b03", "9c71cebea52c21c8000000b3000000a99c71cec89c71ced3000000a9a52c21be000000a9a04e77be000000d39c71cec8000000f3a52c21dda52c21f3a52c21c8"),
        // fmt4#0
        (Bc::Bc4, "963c7bf572f1954abb3bc6840a487b7a", "00007cff000048ff000062ff000089ff000048ff000062ff00006fff00007cff00003cff000055ff000048ff000089ff00003cff000062ff000089ff000089ff"),
        // fmt4#1
        (Bc::Bc4, "a884b0173111f74e3f2ad48bceba9a7c", "0000a8ff00008eff00008eff00009dff000084ff0000a2ff000098ff000084ff000084ff0000a2ff000098ff00009dff000089ff000093ff00009dff0000a2ff"),
        // fmt4#2
        (Bc::Bc4, "b9cde439f0305852c319e292912cb87e", "0000c5ff0000c5ff0000ffff0000c5ff0000c1ff0000b9ff0000c5ff0000ffff0000b9ff000000ff0000b9ff0000c5ff0000c9ff0000c5ff0000c5ff0000bdff"),
        // fmt5#0
        (Bc::Bc5, "5aec7d44f2f97d52e548d35609051180", "00b8ceff00ceffff00b8ecff00b877ff008bb1ff00ceb1ff00ceb1ff00e5ffff008becff00e5ffff00a1ffff00e500ff0048ffff00e5b1ff00e5b1ff00a177ff"),
        // fmt5#1
        (Bc::Bc5, "6c34b166b119df566937e15dcd762f82", "003734ff005344ff003e64ff00455cff004c44ff006164ff005a54ff00454cff004534ff00455cff004c54ff003e3cff00614cff00534cff00694cff005364ff"),
        // fmt5#2
        (Bc::Bc5, "7e7de6877038405aed26ef6491e84e84", "00427dff007b7dff00b47dff00d07dff005e7eff00d07dff00977dff00977dff00ed7eff007b7dff00b47eff00427eff00977dff00ed7dff00267dff00977dff"),
        // BC7 with a forced mode bit, so every mode is exercised.
        (Bc::Bc7, "01efc6004dfc099363be04ac2d669665", "8c8c8cff8c8c8cff609750ffb34471ff926f7aff926f7aff45b246ffce297bff8f7b81ffe26215ffa8932eff45b246ffa8932effc57b22ffa8932eff4edf56ff"),
        (Bc::Bc7, "02d178d99cf080980745746125168074", "63614eff78088dff414f7fff403c5cff6a4463ff712578ff4264a4ff403c5cff6a4463ff63614eff403c5cff403c5cff6e346eff6e346eff4377c7ff41466eff"),
        (Bc::Bc7, "04b32bb3ebe3f89cabcbe5171dc66983", "d14cb0ffd14cb0ffef39ceffb26091ffd14cb0ffef39ceff947373ff7386e7ff5ae729ff8ece4fffa55ad6ff8c70deff8ece4fff5a9cefff5a9cefff7386e7ff"),
        (Bc::Bc7, "0895dd8c39d66fa0505255cd15765292", "37cad1ff45e4d7ff29b1cbff29b1cbff3428e6ff45e4d7ff53fdddff29b1cbff84165cff5b1fa3ff5b1fa3ff29b1cbff84165cff5b1fa3ffab0d19ff84165cff"),
        (Bc::Bc7, "10778f6588c9e6a4f4d9c5830d273ba1", "2c2ec8a13118bd9a215adeb22c2ec89d2644d39a3118bda42c2ec8a42c2ec89a2644d3b22644d3a8215adea8215adeab3118bda4215adea12644d39a215adeab"),
        (Bc::Bc7, "2059423fd7bd5da8985f353905d724b0", "bbfbb3176e72081dbbfbb3176e7208176e72082a6e72081d879f401d879f402a879f4017879f401da2ce7b24879f4017bbfbb3176e720817a2ce7b2a879f4024"),
        (Bc::Bc7, "403af41825b0d5ac3ce6a5effd870dbf", "5d97cba23393dabc5d97cba2cba1a5604d95d1ac949cb881d8a2a058cba1a560bb9faa6ad8a2a0586a98c79a7799c293bb9faa6a098fe9d5d8a2a058a19db379"),
        (Bc::Bc7, "801ca7f174a34cb0e16c15a5f637f7ce", "41afcfaf7d70ba81b634a6553c343c96b634a6552b3e56921947708e2b3e56921947708e1947708e3c343c96b634a6552b3e5692b634a65508ebe3dbb634a655"),
    ];

    #[test]
    fn matches_the_oracle_block_for_block() {
        for (i, (bc, block, want)) in CORPUS.iter().enumerate() {
            let block = unhex(block);
            let want = unhex(want);
            let got = decode_block(*bc, &block).unwrap();
            for px in 0..16 {
                assert_eq!(
                    got[px * 4..px * 4 + 4],
                    want[px * 4..px * 4 + 4],
                    "corpus #{i} {} pixel {px}",
                    bc.name()
                );
            }
        }
    }

    #[test]
    fn bc1_interpolates_expanded_eight_bit_endpoints() {
        // c0 = red (255,0,0), c1 = blue (0,0,255); indices 0,1,2,3 repeating.
        let block = [0x00, 0xF8, 0x1F, 0x00, 0xE4, 0xE4, 0xE4, 0xE4];
        let got = decode_block(Bc::Bc1, &block).unwrap();
        let want = [
            [0u8, 0, 255, 255],
            [255, 0, 0, 255],
            [85, 0, 170, 255],
            [170, 0, 85, 255],
        ];
        for i in 0..4 {
            assert_eq!(got[i * 4..i * 4 + 4], want[i % 4], "pixel {i}");
            assert_eq!(got[(i + 4) * 4..(i + 4) * 4 + 4], want[i % 4]);
        }
    }

    #[test]
    fn bc1_three_colour_mode_is_opaque_black_like_the_oracle() {
        // c0 < c1 selects the three-colour mode; the fourth palette entry is
        // (0,0,0,255) in this decoder, not the spec's (0,0,0,0).
        let block = [0x1F, 0x00, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF];
        let got = decode_block(Bc::Bc1, &block).unwrap();
        for px in 0..16 {
            assert_eq!(&got[px * 4..px * 4 + 4], &[0, 0, 0, 255], "pixel {px}");
        }
    }

    #[test]
    fn bc4_and_bc5_fill_their_channels() {
        let mut block = [0u8; 16];
        block[0] = 0x40;
        block[1] = 0x40;
        let got = decode_block(Bc::Bc4, &block).unwrap();
        assert_eq!(&got[0..4], &[0, 0, 64, 255]);

        block[8] = 0x20;
        block[9] = 0x20;
        let got = decode_block(Bc::Bc5, &block).unwrap();
        assert_eq!(&got[0..4], &[0, 32, 64, 255]);
    }

    #[test]
    fn bc7_without_a_mode_bit_is_cleared() {
        let got = decode_block(Bc::Bc7, &[0u8; 16]).unwrap();
        assert_eq!(got, [0u8; 64]);
    }

    #[test]
    fn decoding_crops_partial_blocks() {
        // 6x5 image: two block columns, two block rows; right and bottom edges
        // are clipped to 2 and 1 pixels.
        let mut src = Vec::new();
        for i in 0..4 {
            let mut block = [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
            block[4] = i as u8 * 0x11;
            src.extend_from_slice(&block);
        }
        let got = decode(Bc::Bc1, &src, 6, 5).unwrap();
        assert_eq!(got.len(), 6 * 5 * 4);
        let block = |i: usize| decode_block(Bc::Bc1, &src[i * 8..i * 8 + 8]).unwrap();
        let (b00, b10, b01, b11) = (block(0), block(1), block(2), block(3));
        let row0 = &got[0..6 * 4];
        assert_eq!(&row0[0..4], &b00[0..4]);
        assert_eq!(&row0[4 * 4..6 * 4], &b10[0..8]);
        // Row 4 is the first pixel row of the second block row.
        let row4 = &got[4 * 6 * 4..5 * 6 * 4];
        assert_eq!(&row4[0..4], &b01[0..4]);
        assert_eq!(&row4[4 * 4..6 * 4], &b11[0..8]);
    }

    #[test]
    fn short_payload_is_an_error() {
        assert!(decode(Bc::Bc7, &[0u8; 15], 4, 4).is_err());
        assert!(decode_block(Bc::Bc1, &[0u8; 7]).is_err());
        assert!(decode(Bc::Bc1, &[], 0, 4).is_err());
    }
}
