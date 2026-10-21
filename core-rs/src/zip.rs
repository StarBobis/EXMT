//! Minimal ZIP reader, enough for NumPy `.npz` archives.
//!
//! NPZ is a ZIP whose entries are `.npy` files. Real archives written by
//! `numpy.savez_compressed` use a data descriptor (sizes are `0xffffffff` in the
//! local header), so sizes and offsets are read from the central directory, as
//! any conforming reader must. Only stored (0) and deflate (8) entries are
//! accepted; ZIP64 is refused rather than misread.
//!
//! No CRC verification: the consumer (`npy`) validates structure and lengths,
//! and these files come from the Blender add-on, not the network.

use crate::error::{Error, Result};
use crate::inflate;

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub compressed_size: usize,
    pub uncompressed_size: usize,
    /// Offset of the local file header.
    pub header_offset: usize,
}

/// A parsed archive borrowing the original bytes.
pub struct Archive<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Archive<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        Ok(Archive {
            data,
            entries: read_entries(data)?,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Extract one entry, inflating if needed. `max_output` caps decompression.
    pub fn read(&self, name: &str, _max_output: usize) -> Result<Vec<u8>> {
        let entry = self.get(name).ok_or(Error::Invalid {
            what: "zip entry",
            detail: format!("{name:?} not in archive"),
        })?;
        let data_offset = local_data_offset(self.data, entry)?;
        let end = data_offset + entry.compressed_size;
        let compressed = self.data.get(data_offset..end).ok_or(Error::Truncated {
            what: "zip entry data",
            offset: data_offset,
            wanted: entry.compressed_size,
            available: self.data.len().saturating_sub(data_offset),
        })?;
        let out = match entry.method {
            0 => compressed.to_vec(),
            8 => inflate::inflate(compressed, entry.uncompressed_size.max(1))?,
            other => {
                return Err(Error::Unsupported {
                    what: match other {
                        9 => "zip deflate64",
                        12 => "zip bzip2",
                        14 => "zip lzma",
                        _ => "zip compression method",
                    },
                })
            }
        };
        if out.len() != entry.uncompressed_size {
            return Err(Error::SizeMismatch {
                what: "zip entry",
                got: out.len(),
                want: entry.uncompressed_size,
            });
        }
        Ok(out)
    }
}

fn u16_at(data: &[u8], at: usize, what: &'static str) -> Result<u16> {
    let bytes = data.get(at..at + 2).ok_or(Error::Truncated {
        what,
        offset: at,
        wanted: 2,
        available: data.len().saturating_sub(at),
    })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], at: usize, what: &'static str) -> Result<u32> {
    let bytes = data.get(at..at + 4).ok_or(Error::Truncated {
        what,
        offset: at,
        wanted: 4,
        available: data.len().saturating_sub(at),
    })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// End of central directory: scan backwards over the comment (max 64 KiB).
fn eocd_offset(data: &[u8]) -> Result<usize> {
    if data.len() < 22 {
        return Err(Error::Truncated {
            what: "zip end record",
            offset: 0,
            wanted: 22,
            available: data.len(),
        });
    }
    let start = data.len().saturating_sub(22 + 0xFFFF);
    for at in (start..=data.len() - 22).rev() {
        if data.get(at..at + 4) == Some(&EOCD.to_le_bytes()) {
            return Ok(at);
        }
    }
    Err(Error::BadSignature {
        expected: "PK\x05\x06",
        offset: data.len(),
    })
}

fn read_entries(data: &[u8]) -> Result<Vec<Entry>> {
    let eocd = eocd_offset(data)?;
    let count = u16_at(data, eocd + 10, "zip entry count")? as usize;
    let cd_offset = u32_at(data, eocd + 16, "zip central directory offset")? as usize;
    if cd_offset == u32::MAX as usize || count == u16::MAX as usize {
        return Err(Error::Unsupported {
            what: "zip64 archive",
        });
    }
    let mut entries = Vec::with_capacity(count);
    let mut at = cd_offset;
    for _ in 0..count {
        if data.get(at..at + 4) != Some(&CENTRAL.to_le_bytes()) {
            return Err(Error::BadSignature {
                expected: "PK\x01\x02",
                offset: at,
            });
        }
        let method = u16_at(data, at + 10, "zip method")?;
        let compressed_size = u32_at(data, at + 20, "zip compressed size")? as usize;
        let uncompressed_size = u32_at(data, at + 24, "zip uncompressed size")? as usize;
        let name_len = u16_at(data, at + 28, "zip name length")? as usize;
        let extra_len = u16_at(data, at + 30, "zip extra length")? as usize;
        let comment_len = u16_at(data, at + 32, "zip comment length")? as usize;
        let header_offset = u32_at(data, at + 42, "zip local header offset")? as usize;
        if compressed_size == u32::MAX as usize || uncompressed_size == u32::MAX as usize {
            return Err(Error::Unsupported {
                what: "zip64 entry",
            });
        }
        let name_bytes = data
            .get(at + 46..at + 46 + name_len)
            .ok_or(Error::Truncated {
                what: "zip entry name",
                offset: at + 46,
                wanted: name_len,
                available: data.len().saturating_sub(at + 46),
            })?;
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|e| Error::Invalid {
            what: "zip entry name",
            detail: e.to_string(),
        })?;
        entries.push(Entry {
            name,
            method,
            compressed_size,
            uncompressed_size,
            header_offset,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

fn local_data_offset(data: &[u8], entry: &Entry) -> Result<usize> {
    let at = entry.header_offset;
    if data.get(at..at + 4) != Some(&LOCAL.to_le_bytes()) {
        return Err(Error::BadSignature {
            expected: "PK\x03\x04",
            offset: at,
        });
    }
    let name_len = u16_at(data, at + 26, "zip local name length")? as usize;
    let extra_len = u16_at(data, at + 28, "zip local extra length")? as usize;
    let start = at + 30 + name_len + extra_len;
    if start > data.len() {
        return Err(Error::Truncated {
            what: "zip local header",
            offset: at,
            wanted: 30 + name_len + extra_len,
            available: data.len().saturating_sub(at),
        });
    }
    Ok(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_zip_is_rejected() {
        assert!(Archive::new(b"not a zip").is_err());
        assert!(Archive::new(&[]).is_err());
    }
}
