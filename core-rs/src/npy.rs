//! NumPy `.npy` reader, enough for the arrays `.exmtmesh` carries.
//!
//! Only little-endian, C-ordered arrays are accepted. That is what NumPy writes
//! on Windows and what the Blender add-on produces; refusing anything else is
//! safer than silently byte-swapping a mesh. Typed accessors check the declared
//! dtype before reinterpreting bytes.

use crate::error::{Error, Result};

const MAGIC: &[u8; 6] = b"\x93NUMPY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Npy {
    /// NumPy dtype string, e.g. `<f4`, `<u4`, `|u1`, `<U54`.
    pub descr: String,
    pub shape: Vec<usize>,
    pub fortran_order: bool,
    data: Vec<u8>,
}

impl Npy {
    pub fn parse(bytes: &[u8]) -> Result<Npy> {
        if bytes.len() < 10 || &bytes[..6] != MAGIC {
            return Err(Error::BadSignature {
                expected: "NUMPY",
                offset: 0,
            });
        }
        let major = bytes[6];
        let header_len = match major {
            1 => u16::from_le_bytes([bytes[8], bytes[9]]) as usize,
            _ => {
                let raw = bytes.get(8..12).ok_or(Error::Truncated {
                    what: "npy header length",
                    offset: 8,
                    wanted: 4,
                    available: bytes.len().saturating_sub(8),
                })?;
                u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize
            }
        };
        let header_start = if major == 1 { 10 } else { 12 };
        let header_end = header_start + header_len;
        let header_bytes = bytes
            .get(header_start..header_end)
            .ok_or(Error::Truncated {
                what: "npy header",
                offset: header_start,
                wanted: header_len,
                available: bytes.len().saturating_sub(header_start),
            })?;
        let header = String::from_utf8_lossy(header_bytes);

        let descr = extract(&header, "'descr': '", '\'')?.to_string();
        let fortran_order = extract(&header, "'fortran_order': ", ',')?.trim() == "True";
        let shape_text = extract(&header, "'shape': (", ')')?;
        let shape = shape_text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<usize>().map_err(|e| Error::Invalid {
                    what: "npy shape",
                    detail: format!("{s:?}: {e}"),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        if descr.starts_with('>') {
            return Err(Error::Unsupported {
                what: "big-endian npy array",
            });
        }
        if fortran_order {
            return Err(Error::Unsupported {
                what: "Fortran-ordered npy array",
            });
        }

        let data = bytes.get(header_end..).ok_or(Error::Truncated {
            what: "npy data",
            offset: header_end,
            wanted: 0,
            available: 0,
        })?;
        Ok(Npy {
            descr,
            shape,
            fortran_order,
            data: data.to_vec(),
        })
    }

    /// Number of elements the shape describes.
    pub fn count(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn item_size(&self) -> Result<usize> {
        kind_and_size(&self.descr)
    }

    pub fn expected_bytes(&self) -> Result<usize> {
        Ok(self.count() * self.item_size()?)
    }

    /// The declared dtype must match before a typed view makes sense.
    fn require(&self, accepted: &[&str]) -> Result<()> {
        if accepted.iter().any(|k| self.descr.contains(k)) {
            return Ok(());
        }
        Err(Error::Invalid {
            what: "npy dtype",
            detail: format!("expected {accepted:?}, got {}", self.descr),
        })
    }

    pub fn as_f32(&self) -> Result<Vec<f32>> {
        self.require(&["f4"])?;
        let need = self.expected_bytes()?;
        if self.data.len() < need {
            return Err(Error::SizeMismatch {
                what: "npy f32",
                got: self.data.len(),
                want: need,
            });
        }
        Ok(self.data[..need]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    pub fn as_u32(&self) -> Result<Vec<u32>> {
        self.require(&["u4"])?;
        let need = self.expected_bytes()?;
        if self.data.len() < need {
            return Err(Error::SizeMismatch {
                what: "npy u32",
                got: self.data.len(),
                want: need,
            });
        }
        Ok(self.data[..need]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    pub fn as_u8(&self) -> Result<Vec<u8>> {
        self.require(&["u1", "i1"])?;
        let need = self.expected_bytes()?;
        if self.data.len() < need {
            return Err(Error::SizeMismatch {
                what: "npy u8",
                got: self.data.len(),
                want: need,
            });
        }
        Ok(self.data[..need].to_vec())
    }

    /// Decode a string array (`<U…` UTF-32LE or `|S…` UTF-8) into a `String`.
    pub fn as_string(&self) -> Result<String> {
        if self.descr.contains("U") {
            let mut out = String::new();
            for chunk in self.data.chunks_exact(4) {
                let code = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if code == 0 {
                    break;
                }
                let ch = char::from_u32(code).ok_or(Error::Invalid {
                    what: "npy string",
                    detail: format!("bad code point {code}"),
                })?;
                out.push(ch);
            }
            return Ok(out);
        }
        if self.descr.contains("S") {
            let end = self
                .data
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(self.data.len());
            let text =
                String::from_utf8(self.data[..end].to_vec()).map_err(|e| Error::Invalid {
                    what: "npy string",
                    detail: e.to_string(),
                })?;
            return Ok(text);
        }
        Err(Error::Invalid {
            what: "npy string",
            detail: format!("dtype {}", self.descr),
        })
    }
}

fn extract<'a>(header: &'a str, key: &str, end: char) -> Result<&'a str> {
    let start = header.find(key).ok_or(Error::Invalid {
        what: "npy header",
        detail: format!("missing {key:?}"),
    })? + key.len();
    let rest = &header[start..];
    let stop = rest.find(end).ok_or(Error::Invalid {
        what: "npy header",
        detail: format!("unterminated {key:?}"),
    })?;
    Ok(&rest[..stop])
}

/// `(kind, total bytes per element)` from a dtype string.
fn kind_and_size(descr: &str) -> Result<usize> {
    let body = descr.trim_start_matches(['<', '|', '=']);
    let mut chars = body.chars();
    let kind = chars.next().ok_or(Error::Invalid {
        what: "npy dtype",
        detail: descr.to_string(),
    })?;
    let digits: String = chars.collect();
    let width = if digits.is_empty() {
        1
    } else {
        digits.parse::<usize>().map_err(|e| Error::Invalid {
            what: "npy dtype",
            detail: format!("{descr}: {e}"),
        })?
    };
    Ok(match kind {
        'f' | 'i' | 'u' | 'c' => width,
        'b' => 1,
        'U' => width * 4,
        'S' => width,
        _ => {
            return Err(Error::Unsupported { what: "npy dtype" });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(descr: &str, shape: &str, fortran: &str, data: &[u8]) -> Vec<u8> {
        let header =
            format!("{{'descr': '{descr}', 'fortran_order': {fortran}, 'shape': {shape}, }}");
        let mut header = header.into_bytes();
        // NumPy pads so the data starts 64-byte aligned.
        let pad = 64 - ((10 + header.len() + 1) % 64);
        header.extend(std::iter::repeat_n(b' ', pad));
        header.push(b'\n');
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.push(0);
        out.extend_from_slice(&(header.len() as u16).to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
        out
    }

    fn npy(descr: &str, shape: &str, data: &[u8]) -> Vec<u8> {
        build(descr, shape, "False", data)
    }

    #[test]
    fn parses_f32_and_shape() {
        let data = [0u8, 0, 0, 0, 0, 0, 0x80, 0x3f];
        let file = npy("<f4", "(2,)", &data);
        let parsed = Npy::parse(&file).unwrap();
        assert_eq!(parsed.descr, "<f4");
        assert_eq!(parsed.shape, vec![2]);
        assert_eq!(parsed.as_f32().unwrap(), vec![0.0, 1.0]);
        assert!(parsed.as_u32().is_err());
    }

    #[test]
    fn parses_utf32_strings() {
        let mut data = Vec::new();
        for ch in "hi".chars() {
            data.extend_from_slice(&(ch as u32).to_le_bytes());
        }
        data.extend_from_slice(&[0u8; 8]);
        let file = npy("<U4", "()", &data);
        assert_eq!(Npy::parse(&file).unwrap().as_string().unwrap(), "hi");
    }

    #[test]
    fn rejects_big_endian_and_fortran() {
        let big = build(">f4", "(1,)", "False", &[0u8; 4]);
        assert!(matches!(Npy::parse(&big), Err(Error::Unsupported { .. })));
        let fortran = build("<f4", "(1,)", "True", &[0u8; 4]);
        assert!(matches!(
            Npy::parse(&fortran),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn truncated_arrays_are_errors() {
        let file = npy("<f4", "(4,)", &[0u8; 8]);
        assert!(Npy::parse(&file).unwrap().as_f32().is_err());
    }
}
