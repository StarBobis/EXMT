//! DDS headers around raw Unity texture payloads.
//!
//! `texconv` speaks DDS, while a Unity Texture2D stores bare block data. This
//! module is the adapter: it wraps a payload in a minimal header so texconv can
//! decode it, and parses the DDS that texconv writes when encoding a replacement.
//!
//! Only the subset the pipeline needs is modelled: 2D textures, optional mip
//! chain, BC1/BC3/BC4/BC5/BC7 and straight RGBA8. Header field values follow
//! DirectXTex's own writer so the files are accepted without `--permissive`.

use crate::error::{Error, Result};

pub const MAGIC: &[u8; 4] = b"DDS ";
const HEADER_SIZE: u32 = 124;
const DX10_SIZE: usize = 20;
const DX10_FOURCC: &[u8; 4] = b"DX10";

const DDSD_CAPS: u32 = 0x1;
const DDSD_HEIGHT: u32 = 0x2;
const DDSD_WIDTH: u32 = 0x4;
const DDSD_PIXELFORMAT: u32 = 0x1000;
const DDSD_MIPMAPCOUNT: u32 = 0x2_0000;
const DDSD_LINEARSIZE: u32 = 0x8_0000;

const DDPF_ALPHAPIXELS: u32 = 0x1;
const DDPF_FOURCC: u32 = 0x4;
const DDPF_RGB: u32 = 0x40;

const DDSCAPS_COMPLEX: u32 = 0x8;
const DDSCAPS_TEXTURE: u32 = 0x1000;
const DDSCAPS_MIPMAP: u32 = 0x40_0000;

const D3D10_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;

/// How the DDS pixel format is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdsFormat {
    /// Legacy FOURCC: `DXT1`, `DXT5`, `ATI1`, `BC5U`, ...
    FourCc([u8; 4]),
    /// Extended header with a DXGI format code (needed for BC7).
    Dx10(u32),
    /// Uncompressed 8-bit RGBA.
    Rgba32,
}

/// Unity TextureFormat -> DDS declaration, for formats texconv can read.
pub fn from_unity(code: i32) -> Option<DdsFormat> {
    Some(match code {
        10 => DdsFormat::FourCc(*b"DXT1"),
        12 => DdsFormat::FourCc(*b"DXT5"),
        26 => DdsFormat::FourCc(*b"ATI1"),
        27 => DdsFormat::FourCc(*b"BC5U"),
        25 => DdsFormat::Dx10(98), // DXGI_FORMAT_BC7_UNORM
        4 => DdsFormat::Rgba32,
        _ => return None,
    })
}

/// DDS declaration -> Unity TextureFormat, for parsing texconv output.
pub fn to_unity(format: DdsFormat) -> Option<i32> {
    Some(match format {
        DdsFormat::FourCc(fourcc) => match &fourcc {
            b"DXT1" => 10,
            b"DXT5" => 12,
            b"ATI1" | b"BC4U" => 26,
            b"BC5U" | b"ATI2" => 27,
            _ => return None,
        },
        DdsFormat::Dx10(dxgi) => match dxgi {
            71 => 10, // BC1_UNORM
            77 => 12, // BC3_UNORM
            80 => 26, // BC4_UNORM
            83 => 27, // BC5_UNORM
            98 => 25, // BC7_UNORM
            28 => 4,  // R8G8B8A8_UNORM
            87 => 5,  // B8G8R8A8_UNORM
            _ => return None,
        },
        DdsFormat::Rgba32 => 4,
    })
}

/// Bytes one 4x4 block (or one pixel for RGBA32) occupies.
fn unit_bytes(format: DdsFormat) -> Option<usize> {
    Some(match format {
        DdsFormat::FourCc(fourcc) => match &fourcc {
            b"DXT1" | b"ATI1" | b"BC4U" => 8,
            b"DXT5" | b"BC5U" | b"ATI2" => 16,
            _ => return None,
        },
        DdsFormat::Dx10(dxgi) => match dxgi {
            71 | 80 => 8,
            77 | 83 | 98 => 16,
            28 | 87 => 4,
            _ => return None,
        },
        DdsFormat::Rgba32 => 4,
    })
}

/// Bytes of mip 0 for a given size.
pub fn mip0_bytes(width: u32, height: u32, format: DdsFormat) -> Option<usize> {
    let unit = unit_bytes(format)?;
    Some(match format {
        DdsFormat::Rgba32 | DdsFormat::Dx10(28) | DdsFormat::Dx10(87) => {
            width as usize * height as usize * unit
        }
        _ => width.div_ceil(4) as usize * height.div_ceil(4) as usize * unit,
    })
}

/// Build a DDS file that carries `payload` verbatim.
///
/// `payload` must be exactly one mip level; use [`build_chain`] for a full
/// chain. `mips` is written into the header so texconv does not treat the tail
/// as garbage.
pub fn build(width: u32, height: u32, format: DdsFormat, payload: &[u8]) -> Result<Vec<u8>> {
    let want =
        mip0_bytes(width, height, format).ok_or(Error::Unsupported { what: "dds format" })?;
    if payload.len() != want {
        return Err(Error::SizeMismatch {
            what: "dds mip 0",
            got: payload.len(),
            want,
        });
    }
    write_header(width, height, format, 1, payload.len(), true, payload)
}

/// Like [`build`], but `payload` is a complete mip chain and `mips` describes it.
pub fn build_chain(
    width: u32,
    height: u32,
    format: DdsFormat,
    mips: u32,
    payload: &[u8],
) -> Result<Vec<u8>> {
    if mips == 0 || mips > 20 {
        return Err(Error::Invalid {
            what: "dds mips",
            detail: mips.to_string(),
        });
    }
    let mip0 =
        mip0_bytes(width, height, format).ok_or(Error::Unsupported { what: "dds format" })?;
    write_header(width, height, format, mips, mip0, false, payload)
}

fn write_header(
    width: u32,
    height: u32,
    format: DdsFormat,
    mips: u32,
    linear_size: usize,
    exact_payload: bool,
    payload: &[u8],
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid {
            what: "dds size",
            detail: format!("{width}x{height}"),
        });
    }
    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT | DDSD_LINEARSIZE;
    let mut caps = DDSCAPS_TEXTURE;
    if mips > 1 {
        flags |= DDSD_MIPMAPCOUNT;
        caps |= DDSCAPS_COMPLEX | DDSCAPS_MIPMAP;
    }

    let mut pf = [0u8; 32];
    pf[0..4].copy_from_slice(&32u32.to_le_bytes());
    match format {
        DdsFormat::FourCc(fourcc) => {
            pf[4..8].copy_from_slice(&DDPF_FOURCC.to_le_bytes());
            pf[8..12].copy_from_slice(&fourcc);
        }
        DdsFormat::Dx10(_) => {
            pf[4..8].copy_from_slice(&DDPF_FOURCC.to_le_bytes());
            pf[8..12].copy_from_slice(DX10_FOURCC);
        }
        DdsFormat::Rgba32 => {
            pf[4..8].copy_from_slice(&(DDPF_RGB | DDPF_ALPHAPIXELS).to_le_bytes());
            pf[8..12].copy_from_slice(&0u32.to_le_bytes());
            pf[12..16].copy_from_slice(&32u32.to_le_bytes());
            pf[16..20].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
            pf[20..24].copy_from_slice(&0x0000_FF00u32.to_le_bytes());
            pf[24..28].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
            pf[28..32].copy_from_slice(&0xFF00_0000u32.to_le_bytes());
        }
    }

    let mut out = Vec::with_capacity(148 + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&(linear_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // depth
    out.extend_from_slice(&mips.to_le_bytes());
    out.extend_from_slice(&[0u8; 44]); // reserved1
    out.extend_from_slice(&pf);
    out.extend_from_slice(&caps.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // caps2
    out.extend_from_slice(&0u32.to_le_bytes()); // caps3
    out.extend_from_slice(&0u32.to_le_bytes()); // caps4
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved2

    if let DdsFormat::Dx10(dxgi) = format {
        out.extend_from_slice(&dxgi.to_le_bytes());
        out.extend_from_slice(&D3D10_RESOURCE_DIMENSION_TEXTURE2D.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // misc flag
        out.extend_from_slice(&1u32.to_le_bytes()); // array size
        out.extend_from_slice(&0u32.to_le_bytes()); // misc flags 2
    }

    if exact_payload {
        out.extend_from_slice(payload);
    } else {
        let total = mip_chain_bytes(width, height, format, mips);
        let want = total.ok_or(Error::Unsupported {
            what: "dds mip chain",
        })?;
        if payload.len() != want {
            return Err(Error::SizeMismatch {
                what: "dds mip chain",
                got: payload.len(),
                want,
            });
        }
        out.extend_from_slice(payload);
    }
    Ok(out)
}

fn mip_chain_bytes(width: u32, height: u32, format: DdsFormat, mips: u32) -> Option<usize> {
    let mut total = 0;
    for level in 0..mips {
        total += mip0_bytes((width >> level).max(1), (height >> level).max(1), format)?;
    }
    Some(total)
}

/// A parsed DDS header plus its pixel payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dds {
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub format: DdsFormat,
    pub payload_offset: usize,
}

impl Dds {
    /// Unity format id for the payload, when the mapping is known.
    pub fn unity_format(&self) -> Option<i32> {
        to_unity(self.format)
    }

    /// Payload bytes, checking that the file actually carries them.
    pub fn payload<'a>(&self, file: &'a [u8]) -> Result<&'a [u8]> {
        if self.payload_offset > file.len() {
            return Err(Error::Truncated {
                what: "dds payload",
                offset: self.payload_offset,
                wanted: 0,
                available: file.len(),
            });
        }
        Ok(&file[self.payload_offset..])
    }
}

fn u32_at(data: &[u8], at: usize, what: &'static str) -> Result<u32> {
    let slice = data.get(at..at + 4).ok_or(Error::Truncated {
        what,
        offset: at,
        wanted: 4,
        available: data.len().saturating_sub(at),
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Parse a DDS header produced by texconv (or by [`build`]).
pub fn parse(data: &[u8]) -> Result<Dds> {
    if data.len() < 128 || &data[..4] != MAGIC {
        return Err(Error::BadSignature {
            expected: "DDS ",
            offset: 0,
        });
    }
    if u32_at(data, 4, "dds header size")? != HEADER_SIZE {
        return Err(Error::Invalid {
            what: "dds header size",
            detail: u32_at(data, 4, "dds header size")?.to_string(),
        });
    }
    let height = u32_at(data, 12, "dds height")?;
    let width = u32_at(data, 16, "dds width")?;
    let mip_count = u32_at(data, 28, "dds mip count")?.max(1);

    let pf_flags = u32_at(data, 80, "dds pf flags")?;
    let fourcc = &data[84..88];
    let format = if pf_flags & DDPF_FOURCC != 0 {
        if fourcc == DX10_FOURCC {
            if data.len() < 128 + DX10_SIZE {
                return Err(Error::Truncated {
                    what: "dds dx10 header",
                    offset: 128,
                    wanted: DX10_SIZE,
                    available: data.len() - 128,
                });
            }
            DdsFormat::Dx10(u32_at(data, 128, "dds dxgi format")?)
        } else {
            let mut tag = [0u8; 4];
            tag.copy_from_slice(fourcc);
            DdsFormat::FourCc(tag)
        }
    } else if pf_flags & DDPF_RGB != 0 {
        DdsFormat::Rgba32
    } else {
        return Err(Error::Unsupported {
            what: "dds pixel format",
        });
    };

    let payload_offset = if matches!(format, DdsFormat::Dx10(_)) {
        148
    } else {
        128
    };
    if data.len() < payload_offset {
        return Err(Error::Truncated {
            what: "dds payload",
            offset: payload_offset,
            wanted: 0,
            available: data.len(),
        });
    }
    Ok(Dds {
        width,
        height,
        mip_count,
        format,
        payload_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_bc1_and_parses_back() {
        let payload = vec![0xABu8; 64 / 4 * 32 / 4 * 8];
        let dds = build(64, 32, DdsFormat::FourCc(*b"DXT1"), &payload).unwrap();
        assert_eq!(&dds[..4], b"DDS ");
        let parsed = parse(&dds).unwrap();
        assert_eq!((parsed.width, parsed.height, parsed.mip_count), (64, 32, 1));
        assert_eq!(parsed.format, DdsFormat::FourCc(*b"DXT1"));
        assert_eq!(parsed.unity_format(), Some(10));
        assert_eq!(parsed.payload_offset, 128);
        assert_eq!(parsed.payload(&dds).unwrap(), payload.as_slice());
    }

    #[test]
    fn bc7_gets_a_dx10_header() {
        let payload = vec![0u8; 16];
        let dds = build(4, 4, DdsFormat::Dx10(98), &payload).unwrap();
        assert_eq!(&dds[84..88], b"DX10");
        assert_eq!(u32_at(&dds, 128, "dxgi").unwrap(), 98);
        let parsed = parse(&dds).unwrap();
        assert_eq!(parsed.format, DdsFormat::Dx10(98));
        assert_eq!(parsed.unity_format(), Some(25));
        assert_eq!(parsed.payload_offset, 148);
    }

    #[test]
    fn rgba32_uses_rgb_masks() {
        let mut px = vec![0u8; 2 * 3 * 4];
        for (i, b) in px.iter_mut().enumerate() {
            *b = i as u8;
        }
        let dds = build(2, 3, DdsFormat::Rgba32, &px).unwrap();
        let pf_flags = u32_at(&dds, 80, "flags").unwrap();
        assert_eq!(pf_flags, DDPF_RGB | DDPF_ALPHAPIXELS);
        assert_eq!(u32_at(&dds, 88, "bitcount").unwrap(), 32);
        let parsed = parse(&dds).unwrap();
        assert_eq!(parsed.format, DdsFormat::Rgba32);
        assert_eq!(parsed.payload(&dds).unwrap(), px.as_slice());
    }

    #[test]
    fn mip_chain_size_is_checked() {
        // 4x4 BC1 with 3 mips: 4x4, 2x2, 1x1 -> 8 + 8 + 8.
        let payload = vec![0u8; 24];
        let dds = build_chain(4, 4, DdsFormat::FourCc(*b"DXT1"), 3, &payload).unwrap();
        let parsed = parse(&dds).unwrap();
        assert_eq!(parsed.mip_count, 3);
        assert!(build_chain(4, 4, DdsFormat::FourCc(*b"DXT1"), 3, &payload[..16]).is_err());
    }

    #[test]
    fn bad_headers_are_rejected() {
        assert!(parse(b"DDS ").is_err());
        assert!(parse(&[0u8; 128]).is_err());
        let mut dds = build(4, 4, DdsFormat::FourCc(*b"DXT1"), &[0u8; 8]).unwrap();
        dds[4] = 0;
        assert!(parse(&dds).is_err());
    }
}
